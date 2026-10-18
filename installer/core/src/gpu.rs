//! GPU detection, so the installer can pick the right driver and the complete
//! hardware video-decode stack.
//!
//! Detection runs against the machine being installed *onto*, from the live
//! environment — so it must work with no network at all. That constraint
//! shapes the whole module:
//!
//! * `lspci -nn` is parsed rather than `/sys` walked, because the output is
//!   stable and easy to test against real-world samples.
//! * NVIDIA cards are classified from a device-ID table compiled into the
//!   binary ([`crate::nvidia_devices`]). Arch ships no `nvidia-detect` — not in
//!   any repo, not in the AUR — so there is nothing to shell out to.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::exec::Runner;
use crate::nvidia_devices;

/// PCI vendor IDs, as they appear in `lspci -nn` output.
pub const PCI_VENDOR_AMD: u16 = 0x1002;
pub const PCI_VENDOR_NVIDIA: u16 = 0x10de;
pub const PCI_VENDOR_INTEL: u16 = 0x8086;

/// PCI class codes that mean "this is a display adapter".
const CLASS_VGA: &str = "0300";
const CLASS_3D: &str = "0302";

/// The first bracketed 4-hex-digit group on an `lspci` line is the class code.
static CLASS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([0-9a-fA-F]{4})\]").expect("valid regex"));

/// The `[vendor:device]` pair, always the last such group on the line.
static PCI_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([0-9a-fA-F]{4}):([0-9a-fA-F]{4})\]").expect("valid regex"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Amd,
    Nvidia,
    Intel,
    /// A display adapter from a vendor we have no driver recipe for.
    Unknown,
}

impl GpuVendor {
    pub fn label(self) -> &'static str {
        match self {
            GpuVendor::Amd => "AMD",
            GpuVendor::Nvidia => "NVIDIA",
            GpuVendor::Intel => "Intel",
            GpuVendor::Unknown => "inconnu",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gpu {
    pub vendor: GpuVendor,
    /// `vendor:device` in lowercase hex, e.g. `1002:7550`.
    pub pci_id: String,
    /// Human-readable description, for the summary screen.
    pub name: String,
}

impl Gpu {
    /// The PCI device ID, i.e. the part after the colon.
    pub fn device_id(&self) -> Option<u16> {
        u16::from_str_radix(self.pci_id.split(':').nth(1)?, 16).ok()
    }

    /// The `(vendor, device)` pair as numbers.
    pub fn pci_ids(&self) -> Option<(u16, u16)> {
        let (vendor, device) = self.pci_id.split_once(':')?;
        Some((
            u16::from_str_radix(vendor, 16).ok()?,
            u16::from_str_radix(device, 16).ok()?,
        ))
    }
}

/// PCI device IDs of virtual GPUs, which need no vendor driver.
///
/// A virtual machine presents one of these, and installing a driver for the
/// vendor whose name happens to be on it would be wrong twice over: the host's
/// GPU is not the guest's, and the guest is already fully accelerated through
/// the paravirtual device. Worse, `nvidia-open-dkms` in a VM that reports an
/// NVIDIA card would install a driver for hardware the guest cannot see.
const VIRTUAL_DEVICES: &[(u16, u16, &str)] = &[
    // virtio-gpu, what QEMU/KVM and libvirt present by default.
    (0x1AF4, 0x1050, "virtio-gpu"),
    (0x1AF4, 0x1110, "virtio-gpu"),
    // QXL, the SPICE display device.
    (0x1B36, 0x0100, "qxl"),
    // VMware.
    (0x15AD, 0x0405, "VMware SVGA"),
    (0x15AD, 0x0710, "VMware SVGA"),
    // VirtualBox.
    (0x80EE, 0xBEEF, "VirtualBox VGA"),
    // Hyper-V.
    (0x1414, 0x5353, "Hyper-V"),
    // Bochs / QEMU stdvga.
    (0x1234, 0x1111, "QEMU stdvga"),
    // Parallels.
    (0x1AB8, 0x4005, "Parallels"),
];

/// Whether this GPU is a virtual one.
pub fn is_virtual(gpu: &Gpu) -> bool {
    let Some((vendor, device)) = gpu.pci_ids() else {
        return false;
    };
    VIRTUAL_DEVICES
        .iter()
        .any(|(v, d, _)| *v == vendor && *d == device)
}

/// Name of the virtual display device, for the summary screen.
pub fn virtual_device_name(gpu: &Gpu) -> Option<&'static str> {
    let (vendor, device) = gpu.pci_ids()?;
    VIRTUAL_DEVICES
        .iter()
        .find(|(v, d, _)| *v == vendor && *d == device)
        .map(|(_, _, name)| *name)
}

/// Which NVIDIA driver a detected card needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NvidiaPlan {
    /// Turing (GTX 16xx, RTX 20xx) and newer: the open kernel modules, which
    /// are ordinary packages in the official repositories.
    OpenModules,
    /// Older than Turing — Pascal, Volta, Maxwell, Kepler, Fermi. Every legacy
    /// branch lives in the AUR, so this needs the network and a build, and the
    /// exact branch has to be chosen by the user.
    Legacy,
}

/// Legacy NVIDIA branches, newest first.
///
/// All four are AUR packages. The split follows NVIDIA's own support windows,
/// which are keyed to architecture, not to model numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NvidiaLegacy {
    /// Volta, Pascal (GTX 10xx) and Maxwell (GTX 9xx).
    V580,
    /// Kepler (GTX 6xx, plus GTX 760/770/780).
    V470,
    /// Fermi (GTX 400/500).
    V390,
    /// Tesla (GeForce 8 through GTX 200).
    V340,
}

impl NvidiaLegacy {
    pub fn all() -> [NvidiaLegacy; 4] {
        [
            NvidiaLegacy::V580,
            NvidiaLegacy::V470,
            NvidiaLegacy::V390,
            NvidiaLegacy::V340,
        ]
    }

    /// The AUR package to build.
    ///
    /// Note the GTX 700 series straddles two branches: the GTX 750 and 750 Ti
    /// are Maxwell (V580), while the GTX 760 and up are Kepler (V470).
    pub fn package(self) -> &'static str {
        match self {
            NvidiaLegacy::V580 => "nvidia-580xx-dkms",
            NvidiaLegacy::V470 => "nvidia-470xx-dkms",
            NvidiaLegacy::V390 => "nvidia-390xx-dkms",
            NvidiaLegacy::V340 => "nvidia-340xx-dkms",
        }
    }

    /// Which cards this branch covers, for the installer's choice screen.
    pub fn label(self) -> &'static str {
        match self {
            NvidiaLegacy::V580 => "Volta, Pascal (GTX 10xx), Maxwell (GTX 9xx)",
            NvidiaLegacy::V470 => "Kepler (GTX 6xx, GTX 760/770/780)",
            NvidiaLegacy::V390 => "Fermi (GTX 400/500)",
            NvidiaLegacy::V340 => "Tesla (GeForce 8 → GTX 200)",
        }
    }
}

/// Parses `lspci -nn` output, keeping only display adapters.
pub fn parse_lspci(output: &str) -> Vec<Gpu> {
    output.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Gpu> {
    let class = CLASS_RE.captures(line)?.get(1)?.as_str();
    if class != CLASS_VGA && class != CLASS_3D {
        return None;
    }

    // A machine can have several GPUs (hybrid laptops, discrete + integrated),
    // so this returns every display adapter rather than the first.
    // `last()` rather than `next_back()`: the `[vendor:device]` pair is the
    // final bracketed group on the line, but the iterator is not double-ended.
    let caps = PCI_ID_RE.captures_iter(line).last()?;
    let vendor_hex = caps.get(1)?.as_str().to_ascii_lowercase();
    let device_hex = caps.get(2)?.as_str().to_ascii_lowercase();
    let vendor_id = u16::from_str_radix(&vendor_hex, 16).ok()?;

    let vendor = match vendor_id {
        PCI_VENDOR_AMD => GpuVendor::Amd,
        PCI_VENDOR_NVIDIA => GpuVendor::Nvidia,
        PCI_VENDOR_INTEL => GpuVendor::Intel,
        _ => GpuVendor::Unknown,
    };

    Some(Gpu {
        vendor,
        pci_id: format!("{vendor_hex}:{device_hex}"),
        name: describe(line),
    })
}

/// Extracts the human-readable device description from an `lspci` line.
fn describe(line: &str) -> String {
    let after_class = line.split_once("]: ").map_or(line, |(_, rest)| rest);
    match after_class.rfind(" [") {
        Some(index) => after_class[..index].trim().to_string(),
        None => after_class.trim().to_string(),
    }
}

/// Distinct vendors present, in a stable order.
///
/// A hybrid machine reports two GPUs and needs drivers for both; the caller
/// should install for every vendor returned here.
pub fn vendors(gpus: &[Gpu]) -> Vec<GpuVendor> {
    let mut seen = Vec::new();
    for gpu in gpus {
        if !seen.contains(&gpu.vendor) {
            seen.push(gpu.vendor);
        }
    }
    seen
}

/// Classifies an NVIDIA card into a driver branch, offline.
///
/// The bundled table lists every GPU the open kernel modules support, i.e.
/// everything from Turing onwards. Absence from it means Pascal or older —
/// which is why the GTX 1650 and GTX 1660, despite sounding like Pascal-era
/// cards, correctly land on [`NvidiaPlan::OpenModules`].
pub fn classify_nvidia(device_id: u16) -> NvidiaPlan {
    if nvidia_devices::is_supported_by_open_modules(device_id) {
        NvidiaPlan::OpenModules
    } else {
        NvidiaPlan::Legacy
    }
}

/// What to install so that video playback is hardware-accelerated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodecPlan {
    /// Packages from the official repositories.
    pub packages: Vec<String>,
    /// `Some` when an NVIDIA card was detected.
    pub nvidia: Option<NvidiaPlan>,
    /// A DKMS package is in the list, which cannot build without the headers
    /// for the kernel it is built against.
    pub needs_headers: bool,
    /// Every display adapter found is virtual.
    ///
    /// The installer shows this: "you are in a virtual machine" explains why
    /// no vendor driver is offered, which would otherwise look like detection
    /// failing.
    pub virtual_machine: bool,
}

/// Packages every system gets, whatever the GPU.
///
/// The GStreamer plugins are what actually decode; `libva-utils` and
/// `vulkan-tools` are how the user confirms that acceleration is really on
/// rather than silently falling back to the CPU.
pub const COMMON_PACKAGES: &[&str] = &[
    "mesa",
    "libva-utils",
    "vulkan-tools",
    "vdpauinfo",
    "mesa-utils",
    "gst-plugin-va",
    "ffmpeg",
    "gst-plugins-base",
    "gst-plugins-good",
    "gst-plugins-bad",
    "gst-plugins-ugly",
    "gst-libav",
];

/// AMD needs no separate VA-API or VDPAU package any more.
///
/// `mesa` now provides `libva-mesa-driver` itself, and Mesa 25.3 dropped VDPAU
/// support from the open drivers entirely, so `mesa-vdpau` no longer exists.
/// `xf86-video-amdgpu` is X11-only — Wayland uses the kernel's amdgpu driver.
pub const AMD_PACKAGES: &[&str] = &["vulkan-radeon", "xf86-video-amdgpu"];

/// Intel splits by generation: `intel-media-driver` covers Broadwell (Gen8)
/// and newer, `libva-intel-driver` everything older. Both are small, so
/// installing both beats detecting the generation.
///
/// `xf86-video-intel` is deliberately absent: it is deprecated, has no proper
/// support for Gen 11 and newer, and can freeze the graphics stack.
/// `linux-firmware-intel` carries the GPU firmware Skylake and newer need.
pub const INTEL_PACKAGES: &[&str] = &[
    "vulkan-intel",
    "intel-media-driver",
    "libva-intel-driver",
    "linux-firmware-intel",
];

/// Packages for a Turing-or-newer NVIDIA card, all from the official repos.
///
/// `nvidia-open-dkms` rather than the prebuilt `nvidia-open`: DKMS survives a
/// later `linux-lts` install, at the cost of one build during installation.
/// The closed `nvidia` / `nvidia-dkms` packages no longer exist — Arch
/// replaced them with the open modules in December 2025, when the 590 branch
/// dropped Pascal support.
pub const NVIDIA_OPEN_PACKAGES: &[&str] = &[
    "nvidia-open-dkms",
    "nvidia-utils",
    "nvidia-settings",
    "nvidia-prime",
    "libva-nvidia-driver",
];

/// The one NVIDIA package that is safe alongside a legacy AUR branch.
///
/// The repository's `nvidia-utils`, `nvidia-settings` and `nvidia-prime` are
/// built for the current branch and conflict with the legacy utils packages,
/// so a legacy install takes only its own AUR package.
pub const NVIDIA_LEGACY_EXTRA: &[&str] = &["libva-nvidia-driver"];

/// Builds the full package list for the detected GPUs.
pub fn codec_plan(gpus: &[Gpu]) -> CodecPlan {
    let mut plan = CodecPlan {
        packages: COMMON_PACKAGES.iter().map(|p| (*p).to_string()).collect(),
        ..CodecPlan::default()
    };

    // Virtual machines are handled first and separately.
    //
    // A guest sees a paravirtual display device and is already accelerated
    // through it. What the host has is irrelevant to it — and installing a
    // vendor driver would target hardware the guest cannot see. A guest
    // configured with GPU passthrough reports the real card, so it is
    // unaffected by this filter.
    //
    // The common stack still goes in: `mesa` provides the guest's 3D driver
    // (virgl, or llvmpipe in software), and the GStreamer plugins decode in
    // software.
    let physical: Vec<Gpu> = gpus.iter().filter(|gpu| !is_virtual(gpu)).cloned().collect();

    if physical.is_empty() {
        // Either there is no display adapter at all, or every one of them is
        // virtual. Both mean the same thing here: no vendor driver.
        plan.virtual_machine = !gpus.is_empty();
        return plan;
    }

    for vendor in vendors(&physical) {
        match vendor {
            GpuVendor::Amd => extend(&mut plan.packages, AMD_PACKAGES),
            GpuVendor::Intel => extend(&mut plan.packages, INTEL_PACKAGES),
            GpuVendor::Nvidia => {
                // A card with no readable device ID cannot be classified, and
                // guessing the branch would leave the machine with no display.
                let Some(device_id) = physical
                    .iter()
                    .filter(|g| g.vendor == GpuVendor::Nvidia)
                    .find_map(Gpu::device_id)
                else {
                    plan.nvidia = Some(NvidiaPlan::Legacy);
                    plan.needs_headers = true;
                    continue;
                };

                let branch = classify_nvidia(device_id);
                plan.nvidia = Some(branch);
                match branch {
                    NvidiaPlan::OpenModules => {
                        extend(&mut plan.packages, NVIDIA_OPEN_PACKAGES);
                    }
                    // The legacy driver itself is an AUR package the user has
                    // to choose; only the vendor-neutral extra goes in here.
                    NvidiaPlan::Legacy => {
                        extend(&mut plan.packages, NVIDIA_LEGACY_EXTRA);
                    }
                }
                plan.needs_headers = true;
            }
            GpuVendor::Unknown => {}
        }
    }

    // Deliberately does NOT push a headers package here.
    //
    // DKMS needs the headers matching the *chosen* kernel, and which kernel
    // that is is not known in this module. The caller adds
    // `Kernel::headers()` — see `install::Installer::pacstrap`.
    dedupe(&mut plan.packages);
    plan
}

/// Detects the display adapters of the machine being installed onto.
pub fn detect(runner: &Runner<'_>) -> Result<Vec<Gpu>> {
    let output = runner.capture("lspci", &["-nn"])?;
    Ok(parse_lspci(&output))
}

fn extend(target: &mut Vec<String>, source: &[&str]) {
    target.extend(source.iter().map(|p| (*p).to_string()));
}

/// Keeps the first occurrence of each package, preserving order.
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

    /// Real `lspci -nn` lines, including this machine's actual GPU.
    const AMD_LINE: &str = "03:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 48 [Radeon RX 9070/9070 XT/9070 GRE] [1002:7550] (rev c0)";
    const NVIDIA_TURING_LINE: &str = "01:00.0 VGA compatible controller [0300]: NVIDIA Corporation TU116 [GeForce GTX 1660] [10de:2184] (rev a1)";
    const NVIDIA_PASCAL_LINE: &str = "01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GP104 [GeForce GTX 1080] [10de:1b80] (rev a1)";
    const INTEL_LINE: &str = "00:02.0 VGA compatible controller [0300]: Intel Corporation Raptor Lake-P [Iris Xe Graphics] [8086:a7a0] (rev 04)";
    const NVIDIA_3D_LINE: &str = "01:00.0 3D controller [0302]: NVIDIA Corporation GA107M [GeForce MX570] [10de:25b6] (rev a1)";
    const NIC_LINE: &str = "04:00.0 Ethernet controller [0200]: Intel Corporation Ethernet Controller I225-V [8086:15f3] (rev 03)";

    #[test]
    fn detects_amd() {
        let gpus = parse_lspci(AMD_LINE);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].vendor, GpuVendor::Amd);
        assert_eq!(gpus[0].pci_id, "1002:7550");
        assert_eq!(gpus[0].device_id(), Some(0x7550));
        assert!(gpus[0].name.contains("Radeon RX 9070"), "{}", gpus[0].name);
    }

    #[test]
    fn detects_nvidia_including_3d_controllers() {
        // Discrete laptop GPUs often report as 3D controller, not VGA.
        let vga = parse_lspci(NVIDIA_TURING_LINE);
        assert_eq!(vga[0].vendor, GpuVendor::Nvidia);
        assert_eq!(vga[0].pci_id, "10de:2184");

        let controller = parse_lspci(NVIDIA_3D_LINE);
        assert_eq!(controller.len(), 1, "un 3D controller est un GPU");
        assert_eq!(controller[0].vendor, GpuVendor::Nvidia);
    }

    #[test]
    fn detects_intel() {
        let gpus = parse_lspci(INTEL_LINE);
        assert_eq!(gpus[0].vendor, GpuVendor::Intel);
        assert_eq!(gpus[0].pci_id, "8086:a7a0");
    }

    #[test]
    fn ignores_non_display_devices() {
        // A network card also carries a vendor:device pair; picking it up
        // would install a GPU driver for an Ethernet chip.
        assert!(parse_lspci(NIC_LINE).is_empty());
    }

    #[test]
    fn handles_hybrid_machines() {
        let output = format!("{INTEL_LINE}\n{NVIDIA_TURING_LINE}\n{NIC_LINE}");
        let gpus = parse_lspci(&output);
        assert_eq!(gpus.len(), 2);
        assert_eq!(vendors(&gpus), vec![GpuVendor::Intel, GpuVendor::Nvidia]);
    }

    #[test]
    fn vendors_are_deduplicated() {
        let output = format!("{AMD_LINE}\n{AMD_LINE}");
        assert_eq!(vendors(&parse_lspci(&output)), vec![GpuVendor::Amd]);
    }

    #[test]
    fn survives_malformed_lines() {
        let output = "ligne sans crochets\n\n03:00.0 VGA compatible controller [0300]: Truc [zzzz:yyyy]";
        assert!(parse_lspci(output).is_empty());
    }

    #[test]
    fn classifies_turing_as_open_modules() {
        // The GTX 16xx series is Turing, not Pascal, despite the model number.
        let gpus = parse_lspci(NVIDIA_TURING_LINE);
        let plan = codec_plan(&gpus);
        assert_eq!(plan.nvidia, Some(NvidiaPlan::OpenModules));
        assert!(plan.packages.contains(&"nvidia-open-dkms".to_string()));
        assert!(!plan.packages.iter().any(|p| p.contains("580xx")));
    }

    #[test]
    fn classifies_pascal_as_legacy() {
        let gpus = parse_lspci(NVIDIA_PASCAL_LINE);
        let plan = codec_plan(&gpus);
        assert_eq!(plan.nvidia, Some(NvidiaPlan::Legacy));
        // The legacy driver is an AUR package the user chooses, so it must not
        // be silently assumed here.
        assert!(!plan.packages.iter().any(|p| p.contains("nvidia-open")));
        assert!(!plan.packages.iter().any(|p| p.contains("580xx")));
        // Nor may the current-branch userspace packages sneak in: they
        // conflict with the legacy utils.
        assert!(!plan.packages.contains(&"nvidia-utils".to_string()));
    }

    #[test]
    fn nvidia_always_needs_kernel_headers() {
        for line in [NVIDIA_TURING_LINE, NVIDIA_PASCAL_LINE] {
            let plan = codec_plan(&parse_lspci(line));
            assert!(plan.needs_headers, "{line}");
            // The headers package itself is chosen by the caller, because it
            // has to match the selected kernel.
            assert!(
                !plan.packages.iter().any(|p| p.ends_with("-headers")),
                "le plan de codecs ne doit pas choisir de paquets d'en-têtes : {:?}",
                plan.packages
            );
        }
    }

    #[test]
    fn amd_needs_no_separate_decode_packages() {
        // mesa provides libva-mesa-driver, and mesa-vdpau is gone since Mesa
        // 25.3 dropped VDPAU from the open drivers.
        let plan = codec_plan(&parse_lspci(AMD_LINE));
        assert!(plan.packages.contains(&"vulkan-radeon".to_string()));
        assert!(!plan.packages.contains(&"libva-mesa-driver".to_string()));
        assert!(!plan.packages.contains(&"mesa-vdpau".to_string()));
        assert!(plan.nvidia.is_none());
        assert!(!plan.needs_headers);
    }

    #[test]
    fn intel_gets_both_media_drivers_and_firmware() {
        let plan = codec_plan(&parse_lspci(INTEL_LINE));
        assert!(plan.packages.contains(&"intel-media-driver".to_string()));
        assert!(plan.packages.contains(&"libva-intel-driver".to_string()));
        assert!(plan.packages.contains(&"linux-firmware-intel".to_string()));
        assert!(
            !plan.packages.contains(&"xf86-video-intel".to_string()),
            "xf86-video-intel est déprécié et peut geler la pile graphique"
        );
    }

    #[test]
    fn unknown_vendor_still_gets_the_common_stack() {
        let line = "05:00.0 VGA compatible controller [0300]: Matrox Electronics [102b:0522]";
        let plan = codec_plan(&parse_lspci(line));
        assert_eq!(plan.packages.len(), COMMON_PACKAGES.len());
        assert!(plan.packages.contains(&"ffmpeg".to_string()));
    }

    #[test]
    fn common_packages_are_never_duplicated() {
        let output = format!("{AMD_LINE}\n{INTEL_LINE}");
        let plan = codec_plan(&parse_lspci(&output));
        let mut sorted = plan.packages.clone();
        sorted.sort();
        let count = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "doublon dans {:?}", plan.packages);
    }

    #[test]
    fn hybrid_intel_nvidia_gets_both_stacks() {
        let output = format!("{INTEL_LINE}\n{NVIDIA_TURING_LINE}");
        let plan = codec_plan(&parse_lspci(&output));
        assert!(plan.packages.contains(&"vulkan-intel".to_string()));
        assert!(plan.packages.contains(&"nvidia-open-dkms".to_string()));
    }

    #[test]
    fn legacy_branches_map_to_real_aur_packages() {
        assert_eq!(NvidiaLegacy::V580.package(), "nvidia-580xx-dkms");
        assert_eq!(NvidiaLegacy::V470.package(), "nvidia-470xx-dkms");
        assert_eq!(NvidiaLegacy::V390.package(), "nvidia-390xx-dkms");
        assert_eq!(NvidiaLegacy::V340.package(), "nvidia-340xx-dkms");
        for branch in NvidiaLegacy::all() {
            assert!(branch.package().ends_with("-dkms"));
            assert!(!branch.label().is_empty());
        }
    }

    // ------------------------------------------------- virtual machines ---

    /// `lspci -nn` inside a QEMU/KVM guest, as libvirt configures it.
    const VIRTIO_LINE: &str = "00:01.0 VGA compatible controller [0300]: Red Hat, Inc. Virtio GPU [1af4:1050] (rev 01)";
    const QXL_LINE: &str = "00:02.0 VGA compatible controller [0300]: Red Hat, Inc. QXL paravirtual graphic card [1b36:0100] (rev 04)";
    const VMWARE_LINE: &str = "00:0f.0 VGA compatible controller [0300]: VMware SVGA II Adapter [15ad:0405]";
    const VBOX_LINE: &str = "00:02.0 VGA compatible controller [0300]: VirtualBox Graphics Adapter [80ee:beef]";

    #[test]
    fn recognises_virtual_display_devices() {
        for (line, name) in [
            (VIRTIO_LINE, "virtio-gpu"),
            (QXL_LINE, "qxl"),
            (VMWARE_LINE, "VMware SVGA"),
            (VBOX_LINE, "VirtualBox VGA"),
        ] {
            let gpus = parse_lspci(line);
            assert_eq!(gpus.len(), 1, "{name}");
            assert!(is_virtual(&gpus[0]), "{name} devrait être reconnu comme virtuel");
            assert_eq!(virtual_device_name(&gpus[0]), Some(name));
        }
    }

    #[test]
    fn a_virtual_machine_gets_no_vendor_driver() {
        // The failure this prevents: a guest whose virtual adapter carries a
        // vendor's name getting that vendor's driver installed for hardware it
        // cannot reach.
        for line in [VIRTIO_LINE, QXL_LINE, VBOX_LINE] {
            let plan = codec_plan(&parse_lspci(line));
            assert!(plan.virtual_machine, "devrait être signalé comme VM");
            assert!(plan.nvidia.is_none());
            assert!(!plan.needs_headers, "aucun DKMS en VM : pas d'en-têtes");

            // The vendor *drivers* must be absent. `vulkan-tools` and
            // `libva-utils` also carry a "vulkan-"/"libva-" prefix but are
            // diagnostic tools that belong on every machine, so they are
            // named explicitly rather than matched by prefix.
            for package in [
                "nvidia-open-dkms",
                "nvidia-utils",
                "vulkan-radeon",
                "vulkan-intel",
                "libva-nvidia-driver",
                "intel-media-driver",
                "libva-intel-driver",
                "xf86-video-amdgpu",
            ] {
                assert!(
                    !plan.packages.contains(&package.to_string()),
                    "pilote de constructeur {package} installé en VM : {:?}",
                    plan.packages
                );
            }
        }
    }

    #[test]
    fn a_virtual_machine_still_gets_the_common_stack() {
        // mesa drives the paravirtual 3D path (virgl) and falls back to
        // llvmpipe; the GStreamer plugins decode in software. Without these
        // the guest has no working video at all.
        let plan = codec_plan(&parse_lspci(VIRTIO_LINE));
        for package in ["mesa", "ffmpeg", "gst-plugins-base", "libva-utils"] {
            assert!(plan.packages.contains(&package.to_string()), "{package} manquant");
        }
    }

    #[test]
    fn passthrough_is_not_mistaken_for_a_virtual_machine() {
        // A guest with a real card passed through reports that card, and must
        // get its driver as usual.
        let plan = codec_plan(&parse_lspci(NVIDIA_TURING_LINE));
        assert!(!plan.virtual_machine);
        assert!(plan.packages.contains(&"nvidia-open-dkms".to_string()));
    }

    #[test]
    fn a_machine_with_no_display_adapter_is_not_a_virtual_machine() {
        // `virtual_machine` is what the UI uses to explain the absence of a
        // driver. Claiming it for a headless machine would be wrong.
        let plan = codec_plan(&[]);
        assert!(!plan.virtual_machine);
        assert_eq!(plan.packages.len(), COMMON_PACKAGES.len());
    }

    #[test]
    fn a_physical_gpu_alongside_a_virtual_one_is_still_driven() {
        // Hybrid setups exist in VMs too, e.g. a passed-through card next to
        // the paravirtual one.
        let output = format!("{VIRTIO_LINE}\n{AMD_LINE}");
        let plan = codec_plan(&parse_lspci(&output));
        assert!(!plan.virtual_machine, "il reste un GPU physique");
        assert!(plan.packages.contains(&"vulkan-radeon".to_string()));
    }
}
