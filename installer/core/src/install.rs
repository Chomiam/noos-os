//! Orchestrating a full installation.
//!
//! The step order is not arbitrary; each entry below records *why* it has to
//! happen where it does. Anything that can be expressed as a pure function
//! (fstab, limine.conf, snapper settings) lives in its own module and is
//! tested there — this file is the part that actually touches the machine.

use std::path::Path;

use crate::btrfs;
use crate::config::{Firmware, InstallConfig};
use crate::desktop;
use crate::error::{Error, Result};
use crate::events::{EventSink, FinishInfo, InstallEvent, ProgressInfo, StepInfo};
use crate::exec::{self, Runner};
use crate::flatpak;
use crate::gpu::{self, CodecPlan, Gpu};
use crate::hardware;
use crate::layout::{self, PartitionPlan};
use crate::keyboard;
use crate::limine;
use crate::locale;
use crate::mkinitcpio;
use crate::powersave;
use crate::services;
use crate::snapshots;

/// Where the target system is assembled.
pub const MOUNT_ROOT: &str = "/mnt";

/// Temporary non-root account used to build AUR packages.
///
/// `makepkg` refuses to run as root, and so do yay and paru. This is not a
/// nicety — without it, no AUR package can be built at all. It is deleted in
/// the AUR step.
const BUILDER_USER: &str = "noosbuilder";

/// Where AUR packages are cloned and built inside the target.
///
/// **Deliberately not `/tmp`.** Every command the installer runs in the target
/// is its own `arch-chroot` invocation, and `arch-chroot` mounts a fresh tmpfs
/// on `/tmp` each time. Cloning, chowning and building are three separate
/// invocations, so anything left under `/tmp` by the first is gone by the
/// second: the chown fails on a directory that no longer exists, `?` aborts
/// the whole AUR step, and the installation ends with neither yay nor the
/// snapshot tooling installed.
///
/// The builder account survives that abort too — the end of the AUR step is
/// what removes it — which is why [`Self::cleanup`] removes it as well.
/// `/var/tmp` is a real filesystem, so it persists across invocations.
const AUR_WORKDIR: &str = "/var/tmp";

/// The AUR helper installed on the target.
///
/// The `-bin` variant is a prebuilt binary; the source package would compile
/// Go inside the chroot, which costs minutes on every installation.
pub const AUR_HELPER: &str = "yay";
pub const AUR_HELPER_PACKAGE: &str = "yay-bin";

/// Filesystem labels, so a live medium can be told apart from a target disk.
const ESP_LABEL: &str = "NOOS-ESP";
const ROOT_LABEL: &str = "noos-root";
const SWAP_LABEL: &str = "noos-swap";

/// Whether a step's failure should abort the installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criticality {
    /// The installed system is unusable without it. Failure aborts.
    Required,
    /// The system is already complete and bootable before this step runs.
    /// Failure is reported and the installation carries on.
    Optional,
}

/// One step of the installation, as shown in the UI.
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub criticality: Criticality,
}

/// The sequence, in execution order.
///
/// The order is not a matter of taste — each position is forced by what the
/// step needs to already exist:
///
/// * `preflight` and `gpu` come first because they are the only two that can
///   fail without having touched anything. Everything after them destroys
///   data, so anything checkable has to be checked before.
/// * `partition` creates the filesystems; `subvolumes` mounts them. Nothing
///   can be written before that.
/// * `pacstrap` populates the target. Every step from `fstab` to `desktop`
///   writes into a tree that pacstrap created, or runs a program it installed.
/// * `fstab`, `locale` and `initramfs` come before `limine`, which references
///   the kernel and initramfs paths they produce.
/// * `cleanup` is last, and also runs on failure.
///
/// `aur` is the only optional step: it is the only one that needs the network,
/// and by the time it runs the machine is already installed.
pub const STEPS: &[Step] = &[
    Step { id: "preflight", title: "Vérification du système", criticality: Criticality::Required },
    Step { id: "gpu", title: "Détection du matériel graphique", criticality: Criticality::Required },
    Step { id: "partition", title: "Partitionnement du disque", criticality: Criticality::Required },
    Step { id: "subvolumes", title: "Création des sous-volumes btrfs", criticality: Criticality::Required },
    Step { id: "pacstrap", title: "Installation des paquets", criticality: Criticality::Required },
    Step { id: "fstab", title: "Génération de /etc/fstab", criticality: Criticality::Required },
    Step { id: "locale", title: "Langue, fuseau horaire et nom de machine", criticality: Criticality::Required },
    Step { id: "initramfs", title: "Configuration de l'initramfs", criticality: Criticality::Required },
    Step { id: "users", title: "Création du compte utilisateur", criticality: Criticality::Required },
    Step { id: "limine", title: "Installation du chargeur limine", criticality: Criticality::Required },
    Step { id: "snapshots", title: "Configuration des instantanés btrfs", criticality: Criticality::Required },
    Step { id: "services", title: "Activation des services", criticality: Criticality::Required },
    Step { id: "powersave", title: "Gestion d'énergie des cartes réseau", criticality: Criticality::Required },
    Step { id: "aur", title: "Support AUR et Shelly", criticality: Criticality::Optional },
    Step { id: "desktop", title: "Configuration du bureau", criticality: Criticality::Required },
    Step { id: "cleanup", title: "Finalisation", criticality: Criticality::Required },
];

/// UUIDs read back after formatting, needed by later steps.
#[derive(Default, Clone)]
struct Uuids {
    btrfs: String,
    esp: String,
    swap: Option<String>,
}

pub struct Installer<'a> {
    config: InstallConfig,
    sink: &'a dyn EventSink,
    runner: Runner<'a>,
    /// Filled in by the preflight step, once the disk size is known.
    plan: Option<PartitionPlan>,
    uuids: Uuids,
    /// Whether the target disk rotates. Read once, from sysfs.
    rotational: bool,
    /// Filled in by the GPU step, before anything touches the disk.
    gpus: Vec<Gpu>,
    codec: CodecPlan,
}

impl<'a> Installer<'a> {
    pub fn new(config: InstallConfig, sink: &'a dyn EventSink) -> Self {
        Self {
            config,
            sink,
            runner: Runner::new(sink),
            plan: None,
            uuids: Uuids::default(),
            rotational: false,
            gpus: Vec::new(),
            codec: CodecPlan::default(),
        }
    }

    /// Runs the whole sequence.
    ///
    /// A required step that fails aborts: the target is unmounted and the
    /// error is reported. An optional step that fails is recorded and the
    /// installation continues — by the time those run, the system is complete
    /// and bootable, and failing the whole installation over a package that
    /// could not be downloaded would be wrong.
    pub fn run(mut self) -> Result<()> {
        let total = STEPS.len();
        let mut warnings: Vec<String> = Vec::new();

        for (index, step) in STEPS.iter().enumerate() {
            self.sink.emit(InstallEvent::Step(StepInfo {
                id: step.id.to_string(),
                index: index + 1,
                total,
                title: step.title.to_string(),
            }));

            let result = match step.id {
                "preflight" => self.preflight(),
                "gpu" => self.detect_gpu(),
                "partition" => self.partition(),
                "subvolumes" => self.subvolumes(),
                "pacstrap" => self.pacstrap(),
                "fstab" => self.write_fstab(),
                "locale" => self.configure_locale(),
                "initramfs" => self.configure_initramfs(),
                "users" => self.create_user(),
                "limine" => self.install_limine(),
                "snapshots" => self.configure_snapshots(),
                "services" => self.configure_services(),
                "powersave" => self.configure_powersave(),
                "aur" => self.install_aur_support(),
                "desktop" => self.configure_desktop(),
                "cleanup" => self.cleanup(),
                other => Err(Error::Config(format!("étape inconnue : {other}"))),
            };

            if let Err(err) = result {
                match step.criticality {
                    Criticality::Required => {
                        // A half-mounted target is worse than a failed install.
                        let _ = self.cleanup();
                        self.sink.emit(InstallEvent::Finished(FinishInfo {
                            success: false,
                            message: err.to_string(),
                        }));
                        return Err(err);
                    }
                    Criticality::Optional => {
                        // The machine is already installed; say what was
                        // skipped and carry on.
                        let message = format!("{} ignorée : {err}", step.title);
                        self.note(&message);
                        warnings.push(message);
                    }
                }
            }
        }

        let message = if warnings.is_empty() {
            "Installation terminée. Tu peux redémarrer.".to_string()
        } else {
            format!(
                "Installation terminée, avec {} étape(s) ignorée(s) :\n{}",
                warnings.len(),
                warnings.join("\n")
            )
        };

        self.sink.emit(InstallEvent::Finished(FinishInfo {
            success: true,
            message,
        }));
        Ok(())
    }

    fn progress(&self, percent: f32, message: impl Into<String>) {
        self.sink.emit(InstallEvent::Progress(ProgressInfo {
            percent: Some(percent),
            message: message.into(),
        }));
    }

    fn note(&self, message: impl Into<String>) {
        self.sink.emit(InstallEvent::Progress(ProgressInfo {
            percent: None,
            message: message.into(),
        }));
    }

    // ----------------------------------------------------------- guards

    /// Fails unless `/mnt` holds a target system pacstrap has populated.
    ///
    /// Every step from `fstab` onwards writes into, or runs a program from,
    /// the tree pacstrap created. Checking is cheap and the alternative is a
    /// confusing error from a tool inside a chroot that is not there.
    fn require_target(&self) -> Result<()> {
        let marker = format!("{MOUNT_ROOT}/etc/pacman.conf");
        if !Path::new(&marker).exists() {
            return Err(Error::Config(format!(
                "{marker} est absent : les paquets n'ont pas été installés, ou la cible a été démontée"
            )));
        }
        Ok(())
    }

    /// Fails unless `/mnt` is a real mount point.
    fn require_mounted(&self) -> Result<()> {
        let fs = self
            .runner
            .capture("findmnt", &["-n", "-o", "FSTYPE", "--target", MOUNT_ROOT])
            .unwrap_or_default();
        if fs.trim().is_empty() {
            return Err(Error::Config(format!(
                "{MOUNT_ROOT} n'est monté sur rien : l'étape précédente a échoué ou la cible a été démontée"
            )));
        }
        Ok(())
    }

    /// Fails unless `/mnt/boot` is the FAT32 boot partition.
    ///
    /// The single most damaging mistake this installer can make. If `/boot`
    /// ends up inside the btrfs root subvolume instead of on the ESP, pacstrap
    /// writes the kernel where limine cannot read it, and the installation
    /// reports success on a machine that will not boot.
    fn require_boot_is_esp(&self) -> Result<()> {
        let fs = self
            .runner
            .capture(
                "findmnt",
                &["-n", "-o", "FSTYPE", "--target", &format!("{MOUNT_ROOT}/boot")],
            )
            .unwrap_or_default();

        match fs.trim() {
            "vfat" => Ok(()),
            "" => Err(Error::Layout(format!(
                "{MOUNT_ROOT}/boot n'est pas monté : le noyau serait écrit hors de la partition de démarrage"
            ))),
            other => Err(Error::Layout(format!(
                "{MOUNT_ROOT}/boot est en {other}, pas en FAT32 : limine ne saurait pas le lire"
            ))),
        }
    }

    /// Fails unless the kernel image is present on the boot partition.
    fn require_kernel(&self) -> Result<()> {
        let image = format!("{MOUNT_ROOT}/boot/{}", self.config.kernel.image());
        if !Path::new(&image).exists() {
            return Err(Error::Config(format!(
                "{image} est absent : le noyau {} n'a pas été installé",
                self.config.kernel.package()
            )));
        }
        Ok(())
    }

    // ---------------------------------------------------------------- steps

    /// Checks everything that can be checked before destroying data.
    fn preflight(&mut self) -> Result<()> {
        self.config.validate()?;

        if self.runner.capture_line("id", &["-u"])? != "0" {
            return Err(Error::Config(
                "l'installateur doit tourner en root pour partitionner le disque".into(),
            ));
        }

        for tool in ["sgdisk", "mkfs.btrfs", "mkfs.fat", "mkswap", "pacstrap", "arch-chroot", "blkid"] {
            if !self.runner.probe("sh", &["-c", &format!("command -v {tool}")]) {
                return Err(Error::Config(format!(
                    "outil manquant sur le système live : {tool}"
                )));
            }
        }

        if !Path::new(&self.config.disk).exists() {
            return Err(Error::Layout(format!("{} n'existe pas", self.config.disk)));
        }

        // The firmware the machine is actually running, not the one the user
        // selected.
        //
        // The two have to agree, and the failure is not obvious: choosing UEFI
        // on a BIOS machine writes limine's EFI binary to a FAT partition and
        // then calls `efibootmgr`, which fails, and the machine has no boot
        // path at all. Choosing BIOS on a UEFI machine leaves an MBR that
        // firmware in UEFI-only mode will not read.
        //
        // `efibootmgr` needs the EFI runtime variables, which only exist when
        // the kernel was booted by UEFI firmware — `/sys/firmware/efi` is the
        // test.
        let running_uefi = Path::new("/sys/firmware/efi").exists();
        let selected_uefi = matches!(self.config.firmware, Firmware::Uefi);

        if running_uefi != selected_uefi {
            let (running, selected) = if running_uefi {
                ("UEFI", "BIOS")
            } else {
                ("BIOS", "UEFI")
            };
            return Err(Error::Config(format!(
                "cette machine a démarré en {running}, mais l'installation est configurée pour \
                 {selected} : le système installé ne démarrerait pas"
            )));
        }

        self.note(if running_uefi {
            "démarrage UEFI détecté"
        } else {
            "démarrage BIOS détecté"
        });

        // Refuse to install over the medium this system booted from.
        //
        // A live USB has no partition table — the ISO's filesystem is written
        // straight to the device and the squashfs is loop-mounted from a file
        // inside it — so `lsblk` shows it as a whole disk, its partitions are
        // not mounted, and it would be offered as a perfectly free target.
        // Installing there wipes the medium mid-installation.
        //
        // A virtual machine's CD-ROM is type `rom` rather than `disk` and is
        // filtered out earlier, which is why this never came up in testing.
        let iso_type = self
            .runner
            .capture("blkid", &["-o", "value", "-s", "TYPE", self.config.disk.as_str()])
            .unwrap_or_default();
        if iso_type.contains("iso9660") {
            return Err(Error::Layout(format!(
                "{} porte une image ISO : c'est le média d'installation lui-même",
                self.config.disk
            )));
        }

        // Refuse to touch a disk that is currently mounted: that is either the
        // live medium or the user's own system.
        let mounted = self
            .runner
            .capture("lsblk", &["-rno", "MOUNTPOINT", &self.config.disk])
            .unwrap_or_default();
        if mounted.lines().any(|l| !l.trim().is_empty()) {
            return Err(Error::Layout(format!(
                "{} porte des partitions montées — refuse d'écraser un disque en cours d'utilisation",
                self.config.disk
            )));
        }

        let bytes: u64 = self
            .runner
            .capture_line("blockdev", &["--getsize64", &self.config.disk])?
            .parse()
            .map_err(|_| Error::Layout("taille du disque illisible".into()))?;
        let disk_mib = bytes / (1024 * 1024);

        let plan = layout::plan(disk_mib, self.config.swap)?;
        self.note(format!(
            "disque de {:.1} Gio : {} partitions prévues",
            disk_mib as f64 / 1024.0,
            plan.all().len()
        ));
        self.plan = Some(plan);

        // Read the device type once: the mount options depend on it, and a
        // spinning disk given the SSD options is laid out for hardware that
        // is not there.
        self.rotational = hardware::is_rotational(&self.config.disk);
        if self.rotational {
            self.note("disque mécanique détecté : options de montage adaptées");
        }

        Ok(())
    }

    /// Detects the display hardware, so the right driver and the complete
    /// hardware video-decode stack go in with the rest of the packages.
    ///
    /// Deliberately runs before the partition step: if detection fails, the
    /// user still has a machine to retry on rather than a wiped disk.
    fn detect_gpu(&mut self) -> Result<()> {
        let gpus = gpu::detect(&self.runner)?;

        if gpus.is_empty() {
            self.note("aucun adaptateur graphique détecté");
        } else {
            for gpu in &gpus {
                self.note(format!(
                    "{} — {} [{}]",
                    gpu.vendor.label(),
                    gpu.name,
                    gpu.pci_id
                ));
            }
        }

        let plan = gpu::codec_plan(&gpus);

        // Report the NVIDIA decision explicitly: for a legacy card the
        // installer cannot choose a branch on its own, and silence here would
        // look like the detection simply failed.
        if let Some(branch) = plan.nvidia {
            self.note(match branch {
                gpu::NvidiaPlan::OpenModules => {
                    "GPU NVIDIA Turing ou plus récent : modules ouverts".to_string()
                }
                gpu::NvidiaPlan::Legacy => match self.config.nvidia_legacy {
                    Some(legacy) => format!(
                        "GPU NVIDIA antérieur à Turing : branche {}",
                        legacy.label()
                    ),
                    None => "GPU NVIDIA antérieur à Turing — aucune branche choisie, \
                             aucun pilote propriétaire ne sera installé"
                        .to_string(),
                },
            });
        }

        self.note(format!(
            "{} paquets de pilotes et codecs retenus",
            plan.packages.len()
        ));

        self.gpus = gpus;
        self.codec = plan;
        Ok(())
    }

    fn plan(&self) -> Result<PartitionPlan> {
        self.plan
            .clone()
            .ok_or_else(|| Error::Config("le plan de partitions n'a pas été calculé".into()))
    }

    /// Releases anything still holding the disk, before it is wiped.
    ///
    /// Belt and braces: `preflight` already refuses a disk with mounted
    /// partitions, but something can be mounted in the window between that
    /// check and this one — udev auto-mounting an ESP, a desktop automounter.
    ///
    /// Two things this has to get right, and the first version got both wrong:
    ///
    ///  * **`umount` takes a mountpoint, not a device.** `umount /dev/vda1`
    ///    means "unmount whatever is mounted at the path /dev/vda1", which is
    ///    nothing. It reported "not mounted" and silently did nothing. The
    ///    mountpoints have to be looked up with `findmnt -S`.
    ///  * **`swapoff` on a device that is not swap fails.** On a fresh disk
    ///    none of the partitions is swap, so asking anyway produced "Invalid
    ///    argument" once per partition — six alarming lines that described no
    ///    problem at all. The type is checked first.
    fn release_disk(&self, disk: &str, plan: &PartitionPlan) -> Result<()> {
        for part in plan.all() {
            let path = layout::partition_path(disk, part.number);

            let kind = self
                .runner
                .capture("blkid", &["-o", "value", "-s", "TYPE", path.as_str()])
                .unwrap_or_default();
            if kind.trim() == "swap" {
                let _ = self.runner.stream("swapoff", &[path.as_str()]);
            }

            let targets = self
                .runner
                .capture("findmnt", &["-rno", "TARGET", "-S", path.as_str()])
                .unwrap_or_default();
            // Deepest first: unmounting a parent before its children fails.
            for target in targets.lines().rev() {
                let target = target.trim();
                if target.is_empty() {
                    continue;
                }
                let _ = self.runner.stream("umount", &["-R", target]);
            }
        }

        Ok(())
    }

    fn partition(&mut self) -> Result<()> {
        let disk = self.config.disk.clone();
        let plan = self.plan()?;

        self.release_disk(&disk, &plan)?;

        self.progress(0.0, "Effacement de la table de partitions");
        let _ = self.runner.stream("wipefs", &["-a", disk.as_str()]);
        self.runner
            .stream("sgdisk", &["--zap-all", disk.as_str()])?;

        self.progress(0.3, "Création des partitions");
        for part in plan.all() {
            let type_code = match part.kind {
                layout::PartitionKind::Esp => "ef00",
                layout::PartitionKind::Swap => "8200",
                layout::PartitionKind::Btrfs => "8300",
            };
            let type_arg = format!("{}:{}", part.number, type_code);
            let name_arg = format!("{}:{}", part.number, part.name);
            let range = part.sgdisk_range();

            self.runner.stream(
                "sgdisk",
                &[
                    "-n",
                    range.as_str(),
                    "-t",
                    type_arg.as_str(),
                    "-c",
                    name_arg.as_str(),
                    disk.as_str(),
                ],
            )?;
        }

        // Wait for the kernel and udev to publish the partition nodes.
        //
        // This is the difference between a virtual disk and a real one. A
        // virtio device re-reads its partition table instantly; a SATA or NVMe
        // disk has to be re-scanned, the kernel has to emit the uevents, and
        // udev has to create the nodes. `partprobe` and `udevadm settle` start
        // that work but do not guarantee it is finished, and the tools below
        // address the partitions by path — `mkfs.fat` on a node that does not
        // exist yet fails with "No such file or directory", which says nothing
        // about timing.
        self.progress(0.4, "Attente des partitions");
        self.wait_for_partitions(&disk, &plan)?;

        self.progress(0.5, "Formatage de la partition de démarrage");
        let esp = layout::partition_path(&disk, plan.esp.number);
        self.runner
            .stream("mkfs.fat", &["-F32", "-n", ESP_LABEL, esp.as_str()])?;

        self.progress(0.7, "Formatage de la partition racine");
        let root = layout::partition_path(&disk, plan.root.number);
        self.runner
            .stream("mkfs.btrfs", &["-L", ROOT_LABEL, "-f", root.as_str()])?;

        if let Some(swap) = &plan.swap {
            self.progress(0.9, "Formatage de la partition d'échange");
            let swap_path = layout::partition_path(&disk, swap.number);
            self.runner
                .stream("mkswap", &["-L", SWAP_LABEL, swap_path.as_str()])?;
            self.uuids.swap = Some(self.uuid_of(&swap_path)?);
        }

        self.uuids.esp = self.uuid_of(&esp)?;
        self.uuids.btrfs = self.uuid_of(&root)?;
        self.progress(1.0, "Partitionnement terminé");
        Ok(())
    }

    /// Asks the kernel to re-read the table, then waits for the nodes.
    ///
    /// Polls rather than sleeping a fixed time: the delay is a fraction of a
    /// second on an NVMe disk and can be several seconds on a spinning disk
    /// that has to spin up, and a fixed sleep would be either too short to be
    /// safe or long enough to be annoying.
    fn wait_for_partitions(&self, disk: &str, plan: &PartitionPlan) -> Result<()> {
        // Both are best effort: on a busy disk `partprobe` can return non-zero
        // even though the table was re-read, and the polling below is what
        // actually decides.
        let _ = self.runner.stream("partprobe", &[disk]);
        let _ = self.runner.stream("udevadm", &["settle"]);

        const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        let deadline = std::time::Instant::now() + TIMEOUT;

        let expected: Vec<String> = plan
            .all()
            .iter()
            .map(|part| layout::partition_path(disk, part.number))
            .collect();

        while std::time::Instant::now() < deadline {
            let missing: Vec<&String> = expected
                .iter()
                .filter(|path| !Path::new(path).exists())
                .collect();

            if missing.is_empty() {
                self.note(format!("{} partitions prêtes", expected.len()));
                return Ok(());
            }

            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        // Name every missing node: a partial appearance is the interesting
        // case, and "some partitions are missing" would not say which.
        let missing: Vec<&String> = expected
            .iter()
            .filter(|path| !Path::new(path).exists())
            .collect();

        Err(Error::Layout(format!(
            "les partitions ne sont jamais apparues après {TIMEOUT:?} : {}",
            missing
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    fn uuid_of(&self, device: &str) -> Result<String> {
        let uuid = self
            .runner
            .capture_line("blkid", &["-s", "UUID", "-o", "value", device])?;
        if uuid.is_empty() {
            return Err(Error::Layout(format!("aucun UUID pour {device}")));
        }
        Ok(uuid)
    }

    fn subvolumes(&mut self) -> Result<()> {
        let disk = self.config.disk.clone();
        let plan = self.plan()?;
        let root = layout::partition_path(&disk, plan.root.number);

        self.progress(0.1, "Création des sous-volumes");
        self.runner
            .stream("mount", &[root.as_str(), MOUNT_ROOT])?;

        for subvolume in btrfs::SUBVOLUMES {
            let path = format!("{MOUNT_ROOT}/{}", subvolume.name);

            // Remove a leftover from an interrupted run, best effort.
            //
            // Silenced on purpose: `btrfs subvolume delete` on a path that does
            // not exist prints "ERROR: Could not statfs: No such file or
            // directory", and on a fresh filesystem that is the *normal* case —
            // one alarming line per subvolume, for no reason.
            if Path::new(&path).exists() {
                let _ = self
                    .runner
                    .stream("btrfs", &["subvolume", "delete", path.as_str()]);
            }

            self.runner
                .stream("btrfs", &["subvolume", "create", path.as_str()])?;
        }

        self.runner.stream("umount", &[MOUNT_ROOT])?;

        self.progress(0.6, "Montage de la racine");
        let root_options = btrfs::SUBVOLUMES[0].mount_options(self.rotational);
        self.runner.stream(
            "mount",
            &["-o", root_options.as_str(), root.as_str(), MOUNT_ROOT],
        )?;

        for subvolume in &btrfs::SUBVOLUMES[1..] {
            let target = format!("{MOUNT_ROOT}{}", subvolume.mount);
            let options = subvolume.mount_options(self.rotational);
            exec::make_dir(&target, 0o755)?;
            self.runner
                .stream("mount", &["-o", options.as_str(), root.as_str(), target.as_str()])?;
        }

        // The boot partition, mounted at /boot.
        //
        // This is not optional and not implied by anything above. Without it,
        // pacstrap writes the kernel to /boot *of the btrfs root subvolume*,
        // mkinitcpio writes the initramfs beside it, and limine.conf lands in
        // the same place — a tree limine cannot read, since it only speaks
        // FAT12/16/32 and ISO9660. The installation would report success and
        // the machine would not boot.
        let esp = layout::partition_path(&disk, plan.esp.number);
        let boot_dir = format!("{MOUNT_ROOT}/boot");
        exec::make_dir(&boot_dir, 0o755)?;
        self.runner.stream(
            "mount",
            &["-o", btrfs::ESP_MOUNT_OPTIONS, esp.as_str(), boot_dir.as_str()],
        )?;

        // Prove it: the kernel will be written here, and a silently wrong
        // mount point is the difference between a bootable system and a
        // bricked install.
        let mounted = self
            .runner
            .capture("findmnt", &["-n", "-o", "FSTYPE", "--target", boot_dir.as_str()])
            .unwrap_or_default();
        if !mounted.trim().starts_with("vfat") {
            return Err(Error::Layout(format!(
                "{boot_dir} n'est pas la partition FAT32 de démarrage (type détecté : {})",
                mounted.trim()
            )));
        }

        self.progress(1.0, "Sous-volumes et partition de démarrage prêts");
        Ok(())
    }

    fn pacstrap(&mut self) -> Result<()> {
        self.require_mounted()?;
        // One transaction rather than several: pacstrap is by far the slowest
        // step, and every group shares most of its dependency tree with the
        // others.
        let packages = package_list(&self.config, &self.codec);

        // No `-K`, deliberately — and this is worth stating, because `-K`
        // looks like the modern, careful choice.
        //
        // `pacstrap -K` runs exactly this, from arch-install-scripts:
        //
        //     pacman-key --gpgdir "$newroot/etc/pacman.d/gnupg/" --init
        //
        // `--init` alone. It creates the directory and the trust database but
        // imports no keys, and `-K` implies `-G`, which skips the copy of the
        // host keyring. The target is left with an *empty* keyring, and pacman
        // cannot verify a single package:
        //
        //     warning: Public keyring not found; have you run 'pacman-key --init'?
        //     downloading required keys...
        //     error: keyring is not writable
        //     error: required key missing from keyring
        //
        // Dropping `-K` takes the other branch of the same script: pacstrap
        // copies the live keyring — which `pacman-init.service` has already
        // initialised and populated at boot — into the target. That is the
        // classic behaviour, and it works.
        //
        // Both halves are needed: without `pacman-init.service` on the live
        // medium there is no keyring to copy, and with `-K` the copy is
        // skipped even when there is one.

        self.progress(0.0, format!("Installation de {} paquets", packages.len()));

        let args = pacstrap_args(&packages);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.runner.stream("pacstrap", &args)?;

        self.progress(1.0, "Paquets installés");
        Ok(())
    }

    fn write_fstab(&mut self) -> Result<()> {
        self.require_mounted()?;
        let fstab = btrfs::fstab(
            &self.uuids.btrfs,
            &self.uuids.esp,
            self.uuids.swap.as_deref(),
            self.rotational,
        );
        exec::write_file(format!("{MOUNT_ROOT}/etc/fstab"), &fstab, 0o644)?;
        self.note("fstab écrit");
        Ok(())
    }

    fn configure_locale(&mut self) -> Result<()> {
        self.require_target()?;
        let locale = self.config.locale;
        let wanted = locale.glibc_name();

        // locale.gen ships fully commented out and its lines carry two fields,
        // the locale and its charset. See [`locale::enable`] for what comparing
        // the whole line against `fr_FR.UTF-8` quietly cost: nothing was ever
        // enabled, locale-gen had nothing to do, and it exited 0.
        let locale_gen = format!("{MOUNT_ROOT}/etc/locale.gen");
        let contents =
            std::fs::read_to_string(&locale_gen).map_err(|e| Error::io(&locale_gen, e))?;
        exec::write_file(&locale_gen, &locale::enable(&contents, wanted), 0o644)?;

        self.runner.stream_in(Some(MOUNT_ROOT), "locale-gen", &[])?;

        // `locale-gen` succeeds either way — a file with every line still
        // commented out gives it nothing to complain about. Asking glibc which
        // locales it now holds is what turns that silent no-op into a failure
        // here, instead of a target where LANG names a locale that does not
        // exist and every program falls back to C.
        let listed = self.runner.capture_in(MOUNT_ROOT, "locale", &["-a"])?;
        if !locale::available(&listed, wanted) {
            return Err(Error::Config(format!(
                "la locale {wanted} n'a pas été générée : /etc/locale.conf la nommerait \
                 sans qu'elle existe, et tout tomberait en C"
            )));
        }

        exec::write_file(
            format!("{MOUNT_ROOT}/etc/locale.conf"),
            &format!("LANG={wanted}\n"),
            0o644,
        )?;

        // Keyboard, from the user's choice rather than from the locale. The
        // two are independent: someone reading the installer in French may
        // well be typing on a US keyboard.
        let keyboard = keyboard::by_id(&self.config.keyboard)
            .ok_or_else(|| Error::Config(format!("clavier inconnu : {}", self.config.keyboard)))?;

        exec::write_file(
            format!("{MOUNT_ROOT}/etc/vconsole.conf"),
            &keyboard::vconsole_conf(keyboard),
            0o644,
        )?;

        // The X11 layout file. Wayland compositors read it too when the
        // session does not set its own, which is what makes the desktop agree
        // with the console.
        exec::make_dir(format!("{MOUNT_ROOT}/etc/X11/xorg.conf.d"), 0o755)?;
        exec::write_file(
            format!("{MOUNT_ROOT}/etc/X11/xorg.conf.d/00-keyboard.conf"),
            &keyboard::x11_conf(keyboard),
            0o644,
        )?;

        // timedatectl does not work inside a chroot, so the symlink is made by
        // hand — and it must be a symlink: localtime(5) requires it.
        let localtime = format!("{MOUNT_ROOT}/etc/localtime");
        let _ = std::fs::remove_file(&localtime);
        std::os::unix::fs::symlink(
            format!("/usr/share/zoneinfo/{}", self.config.timezone),
            &localtime,
        )
        .map_err(|e| Error::io(&localtime, e))?;
        self.runner
            .stream_in(Some(MOUNT_ROOT), "hwclock", &["--systohc"])?;

        exec::write_file(
            format!("{MOUNT_ROOT}/etc/hostname"),
            &format!("{}\n", self.config.hostname),
            0o644,
        )?;
        exec::write_file(
            format!("{MOUNT_ROOT}/etc/hosts"),
            &format!(
                "127.0.0.1\tlocalhost\n\
                 ::1\t\tlocalhost\n\
                 127.0.1.1\t{hostname}.localdomain\t{hostname}\n",
                hostname = self.config.hostname
            ),
            0o644,
        )?;

        self.note("langue et fuseau configurés");
        Ok(())
    }

    fn configure_initramfs(&mut self) -> Result<()> {
        self.require_target()?;
        self.require_boot_is_esp()?;
        self.require_kernel()?;
        let conf = mkinitcpio::config(self.uuids.swap.is_some());
        exec::write_file(format!("{MOUNT_ROOT}/etc/mkinitcpio.conf"), &conf, 0o644)?;

        // -P rebuilds every preset the kernel packages installed.
        self.runner
            .stream_in(Some(MOUNT_ROOT), "mkinitcpio", &["-P"])?;
        self.note("initramfs généré");
        Ok(())
    }

    fn create_user(&mut self) -> Result<()> {
        self.require_target()?;
        let username = self.config.username.clone();

        // `-U` creates a group with the same name as the user.
        //
        // Arch's convention, and not the default: without it the account has no
        // group of its own and everything it creates lands in `users`. Desktops
        // and systemd user sessions expect a private group — `systemd --user`
        // takes it, `logind` derives the session from it, and a home directory
        // owned by `user:users` behaves oddly with some permission checks.
        //
        // The full name goes to `-c`, the GECOS field. Commas are stripped:
        // GECOS is comma-separated (name, room, work phone, home phone), so a
        // full name containing one would silently turn the rest into fields
        // nobody asked for.
        let gecos = self.config.full_name.replace(',', " ");

        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "useradd",
            &[
                "-m",
                "-U",
                "-G",
                "wheel",
                "-s",
                "/bin/bash",
                "-c",
                gecos.trim(),
                username.as_str(),
            ],
        )?;

        // chpasswd writes straight into /etc/shadow and never consults PAM, so
        // the policy enforced in `InstallConfig::validate` is the only check
        // that ever runs on this password. It also reads from stdin only —
        // passing it as an argument would expose it in the process list.
        let credentials = format!("{username}:{}\n", self.config.password);
        self.runner
            .stream_with_stdin(Some(MOUNT_ROOT), "chpasswd", &[], &credentials)?;

        // Prove the account exists before the steps that assume it does.
        //
        // `create_user` is the first step whose failure takes every step after
        // it down, and the tools involved report their own problems in their own
        // words. Reading the target's own passwd file is the direct check, and it
        // says exactly what is wrong if the account is not there.
        let passwd = std::fs::read_to_string(format!("{MOUNT_ROOT}/etc/passwd"))
            .map_err(|e| Error::io(format!("{MOUNT_ROOT}/etc/passwd"), e))?;
        if !passwd.lines().any(|line| line.starts_with(&format!("{username}:"))) {
            return Err(Error::Config(format!(
                "le compte {username} est absent de /etc/passwd après useradd"
            )));
        }

        // Also check the group `-U` was supposed to create: a silent failure
        // there would leave a home directory owned by the wrong group.
        let group = std::fs::read_to_string(format!("{MOUNT_ROOT}/etc/group"))
            .unwrap_or_default();
        if !group.lines().any(|line| line.starts_with(&format!("{username}:"))) {
            return Err(Error::Config(format!(
                "le groupe {username} n'a pas été créé : /home/{username} appartiendrait au mauvais groupe"
            )));
        }

        exec::write_file(
            format!("{MOUNT_ROOT}/etc/sudoers.d/10-noos-wheel"),
            "%wheel ALL=(ALL:ALL) ALL\n",
            0o440,
        )?;

        self.note(format!("compte {username} créé (groupe {username}, membre de wheel)"));
        Ok(())
    }

    fn install_limine(&mut self) -> Result<()> {
        self.require_target()?;
        self.require_boot_is_esp()?;
        self.require_kernel()?;
        let conf = limine::config(
            &self.uuids.btrfs,
            self.uuids.swap.as_deref(),
            self.config.kernel,
        );
        exec::write_file(format!("{MOUNT_ROOT}/boot/limine.conf"), &conf, 0o644)?;

        match self.config.firmware {
            Firmware::Uefi => {
                exec::make_dir(format!("{MOUNT_ROOT}/boot/EFI/limine"), 0o755)?;
                exec::make_dir(format!("{MOUNT_ROOT}/boot/EFI/BOOT"), 0o755)?;

                // The firmware executable, at both paths.
                //
                // `/EFI/BOOT/BOOTX64.EFI` is the removable-media fallback the
                // firmware looks for when no NVRAM entry points anywhere. It
                // matters more than it looks: it is what makes the machine
                // bootable when the NVRAM entry could not be written, which
                // happens on real firmware and never in a VM.
                let source = format!("{MOUNT_ROOT}/usr/share/limine/BOOTX64.EFI");
                for target in [
                    format!("{MOUNT_ROOT}/boot/EFI/limine/limine_x64.efi"),
                    format!("{MOUNT_ROOT}/boot/EFI/BOOT/BOOTX64.EFI"),
                ] {
                    std::fs::copy(&source, &target).map_err(|e| Error::io(&source, e))?;
                }

                // The NVRAM entry, which is a *convenience*, not the boot path.
                //
                // Deliberately not fatal. On real hardware `efibootmgr
                // --create` fails for reasons that have nothing to do with the
                // installation: NVRAM full — a well-known problem, and one a
                // VM never has — the variable store mounted read-only, or
                // firmware that ignores new entries. Aborting here would leave
                // a fully written and perfectly bootable system reported as a
                // failed installation.
                //
                // The fallback copy above already guarantees the machine
                // boots; this only makes it appear by name in the firmware
                // menu.
                let part = self.plan()?.esp.number.to_string();
                match self.runner.stream(
                    "efibootmgr",
                    &[
                        "--create",
                        "--disk",
                        self.config.disk.as_str(),
                        "--part",
                        part.as_str(),
                        "--label",
                        "noos-os",
                        "--loader",
                        "\\EFI\\limine\\limine_x64.efi",
                    ],
                ) {
                    Ok(()) => self.note("entrée de démarrage UEFI créée"),
                    Err(err) => {
                        self.note(format!(
                            "entrée NVRAM non créée ({err}) — le système démarrera par le chemin de \
                             repli /EFI/BOOT/BOOTX64.EFI ; l'entrée pourra être ajoutée après coup \
                             avec efibootmgr"
                        ));
                    }
                }
            }
            Firmware::Bios => {
                // limine needs its stage-3 binary reachable on a FAT volume,
                // plus its boot code written to the disk's MBR.
                exec::make_dir(format!("{MOUNT_ROOT}/boot/limine"), 0o755)?;
                let source = format!("{MOUNT_ROOT}/usr/share/limine/limine-bios.sys");
                std::fs::copy(&source, format!("{MOUNT_ROOT}/boot/limine/limine-bios.sys"))
                    .map_err(|e| Error::io(&source, e))?;

                self.runner
                    .stream("limine", &["bios-install", self.config.disk.as_str()])?;
            }
        }

        self.note("limine installé");
        Ok(())
    }

    /// Configures snapper and the limine snapshot integration.
    ///
    /// Every snapper call carries `--no-dbus`, and that is not optional.
    ///
    /// `snapper` talks to `snapperd` over the system bus by default. The
    /// `snapper` package ships that daemon, and on a running system it is
    /// started on demand — but an `arch-chroot` has no D-Bus at all, so the
    /// call fails with:
    ///
    /// ```text
    /// Failure (org.freedesktop.DBus.Error.ServiceUnknown).
    /// ```
    ///
    /// which names the bus error and nothing about the cause. `--no-dbus` is
    /// the documented way to operate without a bus; the manual warns that a
    /// running snapperd will not know about changes made this way, which is
    /// exactly right here — there is no daemon to inform, and the configuration
    /// is written for the *next* boot, not for the running system.
    fn configure_snapshots(&mut self) -> Result<()> {
        self.require_target()?;

        // Both paths are absolute and belong to the *target*, so they need the
        // mount prefix like every other config written here.
        //
        // Without it they land in the live medium's own `/etc` — an overlayfs
        // thrown away at reboot — and the installed system keeps none of them.
        // `limine-snapper-sync` then starts with no TARGET_OS_NAME, does not
        // recognise the `noos-os` entry, and the snapshot submenu never
        // appears. Nothing logs a word about it: the file was written, just not
        // where anything reads it.
        exec::write_file(
            format!("{MOUNT_ROOT}{}", snapshots::LIMINE_SNAPPER_SYNC_CONF),
            &snapshots::limine_snapper_sync_conf(self.uuids.swap.as_deref()),
            0o644,
        )?;
        exec::write_file(
            format!("{MOUNT_ROOT}{}", snapshots::DEFAULT_LIMINE_CONF),
            &snapshots::default_limine_conf(),
            0o644,
        )?;

        // snapper creates /.snapshots itself. Creating it beforehand makes this
        // command fail, which is exactly why the layout never mounts one there.
        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "snapper",
            &[
                snapshots::NO_DBUS,
                "-c",
                snapshots::SNAPPER_CONFIG_NAME,
                "create-config",
                "/",
            ],
        )?;

        // Prove the configuration was written before the next steps depend on
        // it: `limine-snapper-sync` refuses to do anything without a snapper
        // config named `root`, and it says so only much later.
        let config = format!(
            "{MOUNT_ROOT}/etc/snapper/configs/{}",
            snapshots::SNAPPER_CONFIG_NAME
        );
        if !Path::new(&config).exists() {
            return Err(Error::Config(format!(
                "{config} est absent : create-config n'a pas abouti"
            )));
        }

        for (key, value) in snapshots::snapper_settings() {
            let setting = format!("{key}={value}");
            self.runner.stream_in(
                Some(MOUNT_ROOT),
                "snapper",
                &[
                    snapshots::NO_DBUS,
                    "-c",
                    snapshots::SNAPPER_CONFIG_NAME,
                    "set-config",
                    setting.as_str(),
                ],
            )?;
        }

        self.note("snapper configuré");
        Ok(())
    }

    /// Turns on the services that ship with every installation.
    ///
    /// Not conditional any more: Bluetooth, printing and power management are
    /// installed on every system, so their units are always enabled. Enabling
    /// them is what makes the packages useful — `cups` without `cups.socket`
    /// is present, silent and inert.
    fn configure_services(&mut self) -> Result<()> {
        self.require_target()?;
        let desktop = self.config.desktop;

        for unit in services::BLUETOOTH.units {
            self.runner
                .stream_in(Some(MOUNT_ROOT), "systemctl", &["enable", unit])?;
        }

        for unit in services::PRINTING.units {
            // avahi is best-effort: a machine with no network still gets
            // working local printing, and failing the install over it would be
            // wrong.
            let _ = self
                .runner
                .stream_in(Some(MOUNT_ROOT), "systemctl", &["enable", unit]);
        }

        // nss-mdns does nothing without this file: it is the hook that makes
        // `.local` names resolve, and the default file does not have it.
        exec::write_file(
            format!("{MOUNT_ROOT}/etc/nsswitch.conf"),
            services::nsswitch_conf(),
            0o644,
        )?;

        for unit in services::POWER_PROFILES.units {
            self.runner
                .stream_in(Some(MOUNT_ROOT), "systemctl", &["enable", unit])?;
        }

        // Flathub, as a file rather than a `flatpak remote-add` call: the
        // command would need the system bus inside the chroot, which is what
        // made `snapper` fail three steps away from anything naming it. flatpak
        // reads every `*.flatpakrepo` under this directory at startup.
        let remotes_dir = format!("{MOUNT_ROOT}{}", flatpak::REMOTES_DIR);
        exec::make_dir(&remotes_dir, 0o755)?;
        exec::write_file(
            format!("{remotes_dir}/{}", flatpak::FLATHUB_FILE),
            flatpak::FLATHUB_CONF,
            0o644,
        )?;

        self.note(format!(
            "services activés (Bluetooth, impression, profils d'énergie) pour {}",
            desktop.label()
        ));

        Ok(())
    }
    /// Writes the network power-management configuration.
    ///
    /// Three mechanisms, because no single one covers both kinds of interface
    /// — see [`crate::powersave`] for why each is needed.
    ///
    /// Runs after `services` so the units it writes can be enabled alongside
    /// the others, and before `aur` so a failure here is still fatal while the
    /// machine is being built rather than after.
    fn configure_powersave(&mut self) -> Result<()> {
        self.require_target()?;

        // Wireless: NetworkManager's own setting, which survives reconnects.
        exec::make_dir(format!("{MOUNT_ROOT}/etc/NetworkManager/conf.d"), 0o755)?;
        exec::write_file(
            format!("{MOUNT_ROOT}{}", powersave::NETWORKMANAGER_PATH),
            powersave::NETWORKMANAGER_CONF,
            0o644,
        )?;

        // Wired: a script, because NetworkManager has no EEE setting.
        exec::write_file(
            format!("{MOUNT_ROOT}{}", powersave::EEE_SCRIPT_PATH),
            powersave::EEE_SCRIPT,
            0o755,
        )?;

        exec::write_file(
            format!("{MOUNT_ROOT}{}", powersave::EEE_SERVICE_PATH),
            powersave::EEE_SERVICE,
            0o644,
        )?;

        exec::write_file(
            format!("{MOUNT_ROOT}{}", powersave::EEE_UDEV_PATH),
            powersave::EEE_UDEV_RULE,
            0o644,
        )?;

        // The unit is enabled with the others; the udev rule needs no
        // enabling but udev must be told to re-read its rules for the running
        // system, which the target's own boot will do.
        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "systemctl",
            &["enable", powersave::EEE_UNIT],
        )?;

        self.note(
            "gestion d'énergie des cartes réseau désactivée \
             (Wi-Fi via NetworkManager, EEE via ethtool)",
        );

        Ok(())
    }

    /// Installs AUR support, the limine snapshot tooling and Shelly.
    ///
    /// The only step that needs the network, and the only one that can fail
    /// without dooming the installation: the system is already complete and
    /// bootable before it runs.
    fn install_aur_support(&mut self) -> Result<()> {
        self.require_target()?;
        if !self.config.enable_aur {
            self.note("support AUR non demandé, étape ignorée");
            return Ok(());
        }

        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "useradd",
            &["-m", "-s", "/bin/bash", BUILDER_USER],
        )?;
        exec::write_file(
            format!("{MOUNT_ROOT}/etc/sudoers.d/20-noos-builder"),
            &format!("{BUILDER_USER} ALL=(ALL) NOPASSWD: ALL\n"),
            0o440,
        )?;

        // yay-bin rather than yay: the prebuilt binary avoids compiling Go
        // inside the chroot, which would add minutes to every installation.
        self.build_aur_package(AUR_HELPER_PACKAGE)?;

        let mut wanted: Vec<&str> = snapshots::AUR_PACKAGES.to_vec();
        wanted.extend_from_slice(snapshots::REQUIRED_EXTRA_PACKAGES);
        if self.config.install_shelly {
            wanted.push("shelly");
        }
        // A legacy NVIDIA branch is an AUR package, so it rides along here
        // rather than with the repository packages.
        if let (Some(branch), Some(gpu::NvidiaPlan::Legacy)) =
            (self.config.nvidia_legacy, self.codec.nvidia)
        {
            wanted.push(branch.package());
        }

        for package in wanted {
            self.note(format!("AUR : {package}"));
            self.runner.stream_in(
                Some(MOUNT_ROOT),
                "sudo",
                &[
                    "-H",
                    "-u",
                    BUILDER_USER,
                    AUR_HELPER,
                    "-S",
                    "--noconfirm",
                    package,
                ],
            )?;
        }

        self.remove_builder();
        Ok(())
    }

    /// Removes the temporary build account and its sudoers drop-in.
    ///
    /// Best effort on each part: it is called both at the end of the AUR step
    /// and from [`Self::cleanup`], and by then either may already be gone. The
    /// second call is the point — the AUR step is optional and can abort part
    /// way through, which would otherwise leave `noosbuilder` on the disk with
    /// a live `NOPASSWD: ALL` rule.
    fn remove_builder(&self) {
        let _ = self
            .runner
            .stream_in(Some(MOUNT_ROOT), "userdel", &["-r", BUILDER_USER]);
        let _ = std::fs::remove_file(format!("{MOUNT_ROOT}/etc/sudoers.d/20-noos-builder"));
    }

    /// Clones and builds one AUR package as the non-root builder user.
    ///
    /// Every `sudo -u` here passes `-H`. Without it sudo keeps the *invoking*
    /// user's HOME — root's, since the installer runs as root — so `makepkg`
    /// and the tools it drives write to `/root`, which the builder cannot
    /// touch. Gradle in particular fails with "Could not initialize native
    /// services" and never names the directory it was refused, so the real
    /// cause is three removes away from the message.
    fn build_aur_package(&self, package: &str) -> Result<()> {
        let url = format!("https://aur.archlinux.org/{package}.git");
        let workdir = format!("{AUR_WORKDIR}/{package}");
        let ownership = format!("{BUILDER_USER}:{BUILDER_USER}");
        let build = format!("cd {workdir} && makepkg -si --noconfirm");

        let _ = self
            .runner
            .stream_in(Some(MOUNT_ROOT), "rm", &["-rf", workdir.as_str()]);
        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "git",
            &["clone", "--depth", "1", url.as_str(), workdir.as_str()],
        )?;
        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "chown",
            &["-R", ownership.as_str(), workdir.as_str()],
        )?;
        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "sudo",
            &["-H", "-u", BUILDER_USER, "sh", "-c", build.as_str()],
        )?;

        Ok(())
    }

    /// Takes the desktop profile's unwanted packages back out of the target.
    ///
    /// `pacstrap gnome` installs all 58 members of the group and pacman offers
    /// no way to ask for a subset, so the profile names what it does not want
    /// and it is removed here — about 178 MiB of applications, three media
    /// players among them.
    ///
    /// Two details that matter:
    ///
    /// - Only packages that are actually installed are passed to `-R`. A name
    ///   pacman cannot find is not a warning but a hard error, so an upstream
    ///   rename would abort the installation here rather than leave one extra
    ///   application behind.
    /// - `-s` comes with it deliberately: it removes the dependencies that
    ///   *nothing else* needs — `packagekit` behind `gnome-software`, `grilo`
    ///   behind `gnome-music` — while pacman still refuses to touch a library a
    ///   surviving package depends on.
    fn trim_desktop(&self, desktop: desktop::Desktop) -> Result<()> {
        let wanted = desktop.removals();
        if wanted.is_empty() {
            return Ok(());
        }

        let installed = self.runner.capture_in(MOUNT_ROOT, "pacman", &["-Qq"])?;
        let present: Vec<&str> = wanted
            .iter()
            .copied()
            .filter(|name| installed.lines().any(|line| line == *name))
            .collect();

        if present.is_empty() {
            self.note(format!("{} : rien à retirer", desktop.label()));
            return Ok(());
        }

        let mut args = vec!["-Rns", "--noconfirm"];
        args.extend_from_slice(&present);
        self.runner.stream_in(Some(MOUNT_ROOT), "pacman", &args)?;

        self.note(format!(
            "{} : {} paquets superflus retirés",
            desktop.label(),
            present.len()
        ));
        Ok(())
    }

    fn configure_desktop(&mut self) -> Result<()> {
        self.require_target()?;
        let desktop = self.config.desktop;
        let manager = desktop.display_manager();

        self.trim_desktop(desktop)?;

        // Exactly one display manager may own display-manager.service.
        for unit in manager.others() {
            let _ = self
                .runner
                .stream_in(Some(MOUNT_ROOT), "systemctl", &["disable", unit]);
        }
        self.runner
            .stream_in(Some(MOUNT_ROOT), "systemctl", &["enable", manager.service()])?;

        if let Some(greeter) = desktop.profile().greeter_session {
            exec::write_file(
                format!("{MOUNT_ROOT}/etc/lightdm/lightdm.conf"),
                &desktop::lightdm_conf(greeter),
                0o644,
            )?;
        }

        self.runner.stream_in(
            Some(MOUNT_ROOT),
            "systemctl",
            &["enable", "NetworkManager.service"],
        )?;
        // Weekly TRIM for SSDs; harmless on spinning disks.
        let _ = self.runner.stream_in(
            Some(MOUNT_ROOT),
            "systemctl",
            &["enable", "fstrim.timer"],
        );

        // The snapshot chain only works once /boot is mounted, so these are
        // enabled here, after limine has written to it.
        for unit in snapshots::UNITS {
            let _ = self
                .runner
                .stream_in(Some(MOUNT_ROOT), "systemctl", &["enable", unit]);
        }

        self.note(format!("bureau {} configuré", desktop.label()));
        Ok(())
    }

    fn cleanup(&mut self) -> Result<()> {
        // Unconditional, so the build account cannot outlive the installation
        // even if the AUR step aborted before its own removal ran.
        self.remove_builder();

        self.note("Démontage");
        // `-R` so nested mounts (the ESP under /boot) come down with the root.
        let _ = self.runner.stream("umount", &["-R", MOUNT_ROOT]);
        let _ = self.runner.stream("swapoff", &["-a"]);
        Ok(())
    }
}

/// Packages installed on every target system, regardless of desktop.
///
/// The kernel is deliberately absent: it is chosen by the user, and its name
/// also drives the headers package and the paths limine boots from — see
/// [`crate::kernel::Kernel`] and [`Installer::pacstrap`].
///
/// `base-devel` and `git` are here because the AUR step needs them; `git` is
/// notably *not* a dependency of `base-devel`.
const BASE_PACKAGES: &[&str] = &[
    "base",
    "base-devel",
    "git",
    "linux-firmware",
    "mkinitcpio",
    "btrfs-progs",
    "limine",
    "efibootmgr",
    "dosfstools",
    "e2fsprogs",
    "snapper",
    "snap-pac",
    "sudo",
    "networkmanager",
    "vim",
    "nano",
    "man-db",
    "less",
    "which",
    "tzdata",
];

/// Software sources, and the runtime each one needs to work at all.
///
/// Three different ways a user gets an application no repository package
/// provides, and none of them is useful without its support package:
///
/// - **Flatpak** is how Flathub arrives — the remote itself is shipped rather
///   than fetched, so it exists before the machine has ever been online. See
///   [`crate::flatpak`].
/// - **`fuse3`** is what Flatpak mounts its runtimes with, and also what an
///   AppImage needs to mount itself. `flatpak` depends on it, but an AppImage
///   works on a system with no Flatpak at all, so it is named on its own.
/// - **Java 25**: the LTS. `jre-openjdk` is **27**, the newest release rather
///   than the LTS, so the versioned package has to be spelled out — and it is
///   what provides `java-runtime=25` for anything that asks for a JRE.
const APPLICATION_RUNTIMES: &[&str] = &["flatpak", "fuse3", "jre25-openjdk"];

/// The complete package list for a target system.
///
/// Extracted from `pacstrap` so it can be tested. This list decides whether
/// the installed machine works at all — a missing kernel, headers that do not
/// match it, or a duplicated argument that makes pacstrap abort are all silent
/// until the first boot.
pub fn package_list(config: &InstallConfig, codec: &CodecPlan) -> Vec<String> {
    let mut packages: Vec<String> = BASE_PACKAGES.iter().map(|p| (*p).to_string()).collect();
    packages.extend(config.desktop.packages().iter().map(|p| (*p).to_string()));
    packages.extend(codec.packages.iter().cloned());

    // The kernel and its headers. Both are explicit because neither implies
    // the other: `linux` does not pull `linux-headers`, and DKMS modules
    // cannot build without headers matching the running kernel.
    packages.push(config.kernel.package().to_string());
    if codec.needs_headers {
        packages.push(config.kernel.headers().to_string());
    }

    // Bluetooth, printing and power management are part of the system, not
    // options. A desktop without a printer stack or a Bluetooth panel is a
    // desktop someone has to fix afterwards, and the packages are small.
    extend_str(&mut packages, &services::bluetooth_for(config.desktop));
    extend_str(&mut packages, services::PRINTING.packages);
    extend_str(&mut packages, services::POWER_PROFILES.packages);

    // Network power management. `ethtool` is not part of `base`, and the
    // script that turns Energy Efficient Ethernet off calls it by absolute
    // path — without the package it would fail silently on every interface.
    extend_str(&mut packages, powersave::REQUIRED_PACKAGES);

    // Flatpak, AppImage and Java.
    extend_str(&mut packages, APPLICATION_RUNTIMES);

    // CPU microcode.
    //
    // Not part of `base`, and the `microcode` mkinitcpio hook does nothing
    // without it. A virtual machine boots fine on its launch microcode, which
    // is why this goes unnoticed in testing and surfaces on real hardware as
    // instability under load — exactly the VM-versus-metal class of bug.
    extend_str(&mut packages, hardware::microcode_packages(hardware::cpu_vendor()));

    dedupe(&mut packages);
    packages
}
/// Arguments handed to `pacstrap` for a given package list.
///
/// Extracted so the flags can be tested: one of them is a trap — see the note
/// in [`Installer::pacstrap`] about `-K`.
pub fn pacstrap_args(packages: &[String]) -> Vec<String> {
    let mut args = vec![MOUNT_ROOT.to_string()];
    args.extend(packages.iter().cloned());
    args
}

/// Appends a slice of static package names to a `Vec<String>`.
fn extend_str(target: &mut Vec<String>, source: &[&str]) {
    target.extend(source.iter().map(|p| (*p).to_string()));
}

/// Keeps the first occurrence of each package, preserving order.
///
/// The base, desktop, codec and service lists overlap (`mesa` shows up in more
/// than one place), and a duplicated argument makes pacstrap abort.
fn dedupe(packages: &mut Vec<String>) {
    let mut seen = Vec::with_capacity(packages.len());
    packages.retain(|package| {
        if seen.contains(package) {
            false
        } else {
            seen.push(package.clone());
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_ids_are_unique() {
        let mut ids: Vec<&str> = STEPS.iter().map(|s| s.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "identifiants d'étape en double");
    }

    #[test]
    fn every_step_is_dispatched() {
        // Guards against adding a Step to STEPS and forgetting its arm in run().
        let dispatched = [
            "preflight", "gpu", "partition", "subvolumes", "pacstrap", "fstab", "locale",
            "initramfs", "users", "limine", "snapshots", "services", "powersave", "aur",
            "desktop", "cleanup",
        ];
        for step in STEPS {
            assert!(
                dispatched.contains(&step.id),
                "l'étape {} n'est pas traitée dans run()",
                step.id
            );
        }
        assert_eq!(dispatched.len(), STEPS.len());
    }

    /// Position of a step in the sequence.
    fn position(id: &str) -> usize {
        STEPS
            .iter()
            .position(|step| step.id == id)
            .unwrap_or_else(|| panic!("étape {id} introuvable"))
    }

    fn criticality(id: &str) -> Criticality {
        STEPS
            .iter()
            .find(|step| step.id == id)
            .expect("étape")
            .criticality
    }

    // ------------------------------------------------- ordering invariants ---
    //
    // The order is not a matter of taste: each position is forced by what the
    // step needs to already exist. These tests pin the forced ones, so a
    // future reordering fails loudly instead of producing an installation that
    // reports success on a machine that will not boot.

    #[test]
    fn the_destructive_steps_come_after_every_check() {
        // `partition` is the first step that destroys data. Everything that
        // can fail without having touched anything must come before it, or a
        // user with an unsupported configuration loses their disk to a
        // validation error.
        let partition = position("partition");
        assert!(position("preflight") < partition);
        assert!(position("gpu") < partition);
    }

    #[test]
    fn nothing_is_written_before_the_filesystems_exist() {
        let subvolumes = position("subvolumes");
        for step in [
            "pacstrap", "fstab", "locale", "initramfs", "users", "limine", "snapshots",
            "services", "desktop",
        ] {
            assert!(
                position(step) > subvolumes,
                "{step} écrirait avant que les systèmes de fichiers existent"
            );
        }
    }

    #[test]
    fn nothing_runs_in_the_chroot_before_pacstrap_fills_it() {
        // These all call arch-chroot. Against an empty target they fail with a
        // confusing error from inside a chroot that is not there.
        let pacstrap = position("pacstrap");
        for step in ["locale", "initramfs", "users", "snapshots", "services", "desktop"] {
            assert!(
                position(step) > pacstrap,
                "{step} s'exécuterait dans un chroot vide"
            );
        }
    }

    #[test]
    fn limine_comes_after_the_kernel_and_its_initramfs() {
        // The guards around limine check for both, so the ordering is what
        // makes them meaningful rather than vacuous.
        assert!(position("pacstrap") < position("initramfs"));
        assert!(position("initramfs") < position("limine"));
    }

    #[test]
    fn the_snapshot_config_follows_the_limine_config() {
        // limine-snapper-sync matches TARGET_OS_NAME against the entry in
        // limine.conf. The entry has to exist first.
        assert!(position("limine") < position("snapshots"));
    }

    #[test]
    fn cleanup_is_last() {
        assert_eq!(position("cleanup"), STEPS.len() - 1);
    }

    #[test]
    fn the_network_step_is_the_only_optional_one() {
        // `aur` is the only step that needs the network, and by the time it
        // runs the machine is already installed and bootable. Anything else
        // failing silently would produce a system missing something essential.
        for step in STEPS {
            let expected = if step.id == "aur" {
                Criticality::Optional
            } else {
                Criticality::Required
            };
            assert_eq!(
                step.criticality, expected,
                "criticité inattendue pour {}",
                step.id
            );
        }
    }

    #[test]
    fn a_required_failure_aborts_the_run() {
        // The behavioural half of the test above: run() is driven with a
        // configuration that fails at `preflight` — a required step, since the
        // test suite runs unprivileged — and must return an error rather than
        // a warning.
        let sink = crate::events::NullSink;
        let err = Installer::new(sample_config(), &sink)
            .run()
            .expect_err("preflight doit faire échouer l'installation");
        assert!(err.to_string().contains("root"), "{err}");
    }

    #[test]
    fn base_packages_cover_the_whole_snapshot_chain() {
        for package in ["btrfs-progs", "snapper", "snap-pac", "limine", "mkinitcpio"] {
            assert!(BASE_PACKAGES.contains(&package), "{package} manquant");
        }
    }

    #[test]
    fn git_ships_even_though_base_devel_omits_it() {
        assert!(BASE_PACKAGES.contains(&"git"));
        assert!(BASE_PACKAGES.contains(&"base-devel"));
    }

    #[test]
    fn limine_comes_from_the_target_not_the_live_medium() {
        // The live ISO boots with systemd-boot, so limine has to be in the
        // target's own package list.
        assert!(BASE_PACKAGES.contains(&"limine"));
    }

    #[test]
    fn no_kernel_is_hardcoded() {
        // The kernel comes from the config, and so do its headers. Leaving one
        // in BASE_PACKAGES would install two kernels, or the wrong one.
        for kernel in crate::kernel::Kernel::all() {
            assert!(
                !BASE_PACKAGES.contains(&kernel.package()),
                "{} ne doit pas être dans la liste de base",
                kernel.package()
            );
            assert!(!BASE_PACKAGES.contains(&kernel.headers()));
        }
    }

    #[test]
    fn cleanup_is_safe_to_call_twice() {
        // run() calls it on failure and again at the end of a successful run.
        let sink = crate::events::NullSink;
        let mut installer = Installer::new(sample_config(), &sink);
        assert!(installer.cleanup().is_ok());
        assert!(installer.cleanup().is_ok());
    }

    #[test]
    fn preflight_rejects_a_non_root_user() {
        // The test suite runs unprivileged, which is exactly the case here.
        let sink = crate::events::NullSink;
        let mut installer = Installer::new(sample_config(), &sink);
        let err = installer
            .preflight()
            .expect_err("un utilisateur non-root doit être refusé");
        assert!(err.to_string().contains("root"), "{err}");
    }

    #[test]
    fn package_list_contains_exactly_one_kernel() {
        for kernel in crate::kernel::Kernel::all() {
            let mut config = sample_config();
            config.kernel = kernel;
            let packages = package_list(&config, &CodecPlan::default());

            assert!(packages.contains(&kernel.package().to_string()));

            // And no other kernel: installing two would put two images in
            // /boot, only one of which limine is told about.
            for other in crate::kernel::Kernel::all() {
                if other != kernel {
                    assert!(
                        !packages.contains(&other.package().to_string()),
                        "{} ne devrait pas être installé avec {:?}",
                        other.package(),
                        kernel
                    );
                }
            }
        }
    }

    #[test]
    fn headers_match_the_chosen_kernel() {
        // A DKMS module built against the wrong kernel's headers will not
        // load, and the failure appears at boot rather than at install.
        for kernel in crate::kernel::Kernel::all() {
            let mut config = sample_config();
            config.kernel = kernel;
            let codec = CodecPlan {
                needs_headers: true,
                ..CodecPlan::default()
            };

            let packages = package_list(&config, &codec);
            assert!(packages.contains(&kernel.headers().to_string()));

            for other in crate::kernel::Kernel::all() {
                if other != kernel {
                    assert!(
                        !packages.contains(&other.headers().to_string()),
                        "en-têtes de {} avec le noyau {:?}",
                        other.package(),
                        kernel
                    );
                }
            }
        }
    }

    #[test]
    fn headers_are_omitted_when_nothing_needs_them() {
        let config = sample_config();
        let packages = package_list(&config, &CodecPlan::default());
        assert!(
            !packages.iter().any(|p| p.ends_with("-headers")),
            "aucun paquet d'en-têtes ne devrait être installé : {packages:?}"
        );
    }

    #[test]
    fn services_are_always_installed() {
        // Bluetooth, printing and power management are not options any more.
        // They were, and the switches made a desktop someone had to fix
        // afterwards — so this pins that they are unconditional.
        let config = sample_config();
        let packages = package_list(&config, &CodecPlan::default());

        for package in [
            "bluez",
            "bluez-utils",
            "cups",
            "cups-pk-helper",
            "system-config-printer",
            "avahi",
            "nss-mdns",
            "power-profiles-daemon",
        ] {
            assert!(
                packages.contains(&package.to_string()),
                "{package} devrait toujours être installé"
            );
        }
    }

    #[test]
    fn the_aur_helper_is_yay() {
        assert_eq!(AUR_HELPER, "yay");
        // The -bin variant: the source package compiles Go in the chroot.
        assert_eq!(AUR_HELPER_PACKAGE, "yay-bin");
        assert!(AUR_HELPER_PACKAGE.starts_with(AUR_HELPER));
    }

    #[test]
    fn aur_builds_stay_out_of_the_chroot_tmpfs() {
        // `arch-chroot` gives every invocation its own tmpfs on /tmp, and
        // build_aur_package clones, chowns and builds in three separate
        // invocations. A work directory under /tmp is gone by the second one,
        // so the chown fails on a directory that does not exist and the whole
        // AUR step aborts — with nothing installed and the builder account left
        // behind.
        assert!(
            !AUR_WORKDIR.starts_with("/tmp"),
            "le répertoire de construction AUR ne peut pas être sous /tmp : {AUR_WORKDIR}"
        );
        assert!(AUR_WORKDIR.starts_with('/'), "chemin absolu attendu : {AUR_WORKDIR}");
    }

    #[test]
    fn the_builder_account_is_removed_even_if_the_aur_step_aborts() {
        // finalize is what makes the removal unconditional: the AUR step is
        // optional and can stop half way, before its own cleanup.
        let body = include_str!("install.rs");
        let finalize = body
            .split("fn cleanup(&mut self)")
            .nth(1)
            .expect("cleanup doit exister");
        assert!(
            finalize.contains("remove_builder()"),
            "la finalisation doit supprimer le compte de construction"
        );
    }

    #[test]
    fn pacstrap_is_not_given_the_K_flag() {
        // `pacstrap -K` runs `pacman-key --gpgdir <target> --init` — `--init`
        // alone, which imports no keys — and implies `-G`, skipping the copy of
        // the host keyring. The target ends up with an empty keyring and pacman
        // cannot verify anything:
        //
        //     warning: Public keyring not found; have you run 'pacman-key --init'?
        //     error: keyring is not writable
        //     error: required key missing from keyring
        //
        // Without the flag, pacstrap copies the live keyring instead.
        let args = pacstrap_args(&["base".to_string(), "linux".to_string()]);

        assert!(
            !args.iter().any(|arg| arg == "-K"),
            "pacstrap -K laisse un trousseau vide dans la cible : {args:?}"
        );
        assert!(
            !args.iter().any(|arg| arg == "-G"),
            "pacstrap -G saute la copie du trousseau : {args:?}"
        );

        // The target is still the first argument, and the packages follow it.
        assert_eq!(args[0], MOUNT_ROOT);
        assert_eq!(args[1], "base");
        assert_eq!(args[2], "linux");
    }

    #[test]
    fn the_package_list_has_no_duplicates() {
        // A repeated argument makes pacstrap abort, and the overlap between
        // the base, desktop, codec and service lists is real.
        let config = sample_config();
        let codec = CodecPlan {
            needs_headers: true,
            packages: vec!["mesa".into(), "ffmpeg".into()],
            ..CodecPlan::default()
        };

        let packages = package_list(&config, &codec);
        let mut sorted = packages.clone();
        sorted.sort();
        let count = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "doublon dans {packages:?}");
    }

    #[test]
    fn debug_never_prints_the_password() {
        let rendered = format!("{:?}", sample_config());
        assert!(!rendered.contains("correct horse"), "{rendered}");
        assert!(rendered.contains("<redacted>"));
    }

    fn sample_config() -> InstallConfig {
        InstallConfig {
            locale: crate::config::Locale::Fr,
            timezone: "Europe/Paris".into(),
            keyboard: "fr".into(),
            hostname: "noos".into(),
            full_name: "Jean Dupont".into(),
            username: "jean".into(),
            password: "correct horse battery staple 47!".into(),
            desktop: desktop::Desktop::Plasma,
            disk: "/dev/loop0".into(),
            swap: true,
            require_strong_password: true,
            enable_aur: false,
            install_shelly: false,
            firmware: Firmware::Uefi,
            kernel: crate::kernel::Kernel::Linux,
            nvidia_legacy: None,
        }
    }
}
