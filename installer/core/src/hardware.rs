//! Hardware inventory for the installer: block devices and connectivity.
//!
//! Parsing is separated from execution so it can be tested against captured
//! `lsblk` output without a disk present.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::exec::Runner;

/// A whole disk the installer could be pointed at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    /// Device node, e.g. `/dev/nvme0n1`.
    pub device: String,
    pub model: String,
    pub size_bytes: u64,
    pub removable: bool,
    /// True when one of its partitions is mounted.
    ///
    /// The installer refuses these: a mounted disk is either the live medium
    /// itself or a system in use, and overwriting either is a data-loss bug.
    pub in_use: bool,
}

impl Disk {
    pub fn size_mib(&self) -> u64 {
        self.size_bytes / (1024 * 1024)
    }
}

#[derive(Debug, Deserialize)]
struct LsblkOutput {
    #[serde(default)]
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Debug, Deserialize)]
struct LsblkDevice {
    name: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    model: Option<String>,
    /// `rm` in lsblk's JSON, "removable device".
    #[serde(default, rename = "rm")]
    removable: Option<bool>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    mountpoints: Option<Vec<Option<String>>>,
    #[serde(default)]
    children: Vec<LsblkDevice>,
}

impl LsblkDevice {
    /// Whether this device or anything below it is mounted.
    ///
    /// The recursion is not optional: lsblk reports mountpoints on the
    /// *partitions*, so the disk itself always shows `[null]`. Reading only
    /// the top level would report every disk as free — including one holding
    /// the running system.
    fn is_mounted(&self) -> bool {
        self.mountpoints
            .as_ref()
            .is_some_and(|mounts| mounts.iter().any(|m| m.is_some()))
            || self.children.iter().any(LsblkDevice::is_mounted)
    }
}

/// Parses `lsblk --json` output into the disks worth offering.
///
/// Only whole disks are returned — partitions and loop devices are noise here,
/// and a live medium shows up as a loop device precisely so it can be
/// filtered out.
pub fn parse_disks(json: &str) -> Result<Vec<Disk>> {
    let parsed: LsblkOutput = serde_json::from_str(json)
        .map_err(|e| Error::Layout(format!("sortie de lsblk illisible : {e}")))?;

    let disks = parsed
        .blockdevices
        .into_iter()
        .filter(|device| device.kind.as_deref() == Some("disk"))
        .map(|device| {
            let in_use = device.is_mounted();

            Disk {
                device: format!("/dev/{}", device.name),
                // lsblk pads and blanks the model on some devices.
                model: device
                    .model
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| "Disque".to_string()),
                size_bytes: device.size.unwrap_or(0),
                removable: device.removable.unwrap_or(false),
                in_use,
            }
        })
        .collect();

    Ok(disks)
}

/// Lists the disks of the machine being installed onto.
pub fn list_disks(runner: &Runner<'_>) -> Result<Vec<Disk>> {
    // -b: sizes in bytes, so no unit parsing is needed.
    let json = runner.capture("lsblk", &["--json", "-b", "-o", "NAME,SIZE,MODEL,RM,TYPE,MOUNTPOINTS"])?;
    parse_disks(&json)
}

/// Whether the machine has working internet access.
///
/// Used to gate the AUR step, which is the only part of the installation that
/// needs the network. A default route alone is not enough — a captive portal
/// or a dead uplink both look routed — so this actually fetches a mirror
/// index. `curl` ships with pacman, so it is always present.
pub fn check_network(runner: &Runner<'_>) -> bool {
    runner
        .capture(
            "curl",
            &[
                "--silent",
                "--head",
                "--max-time", "4",
                "--output", "/dev/null",
                "https://geo.mirror.pkgbuild.com/",
            ],
        )
        .is_ok()
}

/// Whether a disk is a rotating one.
///
/// Read from `/sys/block/<name>/rotational`, which is what udev and every
/// other tool use. A virtual disk reports `0` — another thing a VM lets pass
/// unnoticed.
pub fn is_rotational(disk: &str) -> bool {
    let name = disk.rsplit('/').next().unwrap_or(disk);
    std::fs::read_to_string(format!("/sys/block/{name}/rotational"))
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

/// The CPU vendor, as `/proc/cpuinfo` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CpuVendor {
    Intel,
    Amd,
    Unknown,
}

/// Reads the CPU vendor from `/proc/cpuinfo`.
///
/// Used to pick the microcode package. A virtual machine reports the vendor of
/// the host's CPU, so the right package is chosen there too — but a VM does
/// not need it, which is exactly why its absence goes unnoticed until the
/// installation lands on real hardware.
pub fn cpu_vendor() -> CpuVendor {
    let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") else {
        return CpuVendor::Unknown;
    };

    for line in cpuinfo.lines() {
        let Some(value) = line.strip_prefix("vendor_id") else {
            continue;
        };
        let value = value.trim_start_matches([':', ' ', '\t']).trim();
        return match value {
            "GenuineIntel" => CpuVendor::Intel,
            "AuthenticAMD" => CpuVendor::Amd,
            _ => CpuVendor::Unknown,
        };
    }

    CpuVendor::Unknown
}

/// The microcode package for a vendor.
///
/// Returns both when the vendor is unknown. Installing a package that is not
/// needed costs a few megabytes; *not* installing the one that is needed
/// leaves the machine running on the CPU's launch microcode, which on recent
/// silicon means missing stability and security fixes the kernel cannot apply
/// on its own — the `microcode` mkinitcpio hook has nothing to load without
/// these files.
pub fn microcode_packages(vendor: CpuVendor) -> &'static [&'static str] {
    match vendor {
        CpuVendor::Intel => &["intel-ucode"],
        CpuVendor::Amd => &["amd-ucode"],
        CpuVendor::Unknown => &["amd-ucode", "intel-ucode"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_vendors_to_microcode_packages() {
        assert_eq!(microcode_packages(CpuVendor::Intel), ["intel-ucode"]);
        assert_eq!(microcode_packages(CpuVendor::Amd), ["amd-ucode"]);
    }

    #[test]
    fn an_unknown_vendor_gets_both() {
        // A few megabytes, against a machine left on its launch microcode.
        let both = microcode_packages(CpuVendor::Unknown);
        assert_eq!(both.len(), 2);
        assert!(both.contains(&"amd-ucode"));
        assert!(both.contains(&"intel-ucode"));
    }

    #[test]
    fn detects_the_vendor_of_this_machine() {
        // The test host is x86_64, so it must report one of the two.
        let vendor = cpu_vendor();
        assert!(
            matches!(vendor, CpuVendor::Intel | CpuVendor::Amd),
            "vendeur inattendu : {vendor:?}"
        );
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;

    /// Trimmed `lsblk --json -b` output, shaped exactly as lsblk emits it.
    const SAMPLE: &str = r#"{
       "blockdevices": [
          {"name":"loop0","size":812345678,"model":null,"rm":false,"type":"loop","mountpoints":[null]},
          {"name":"sda","size":500107862016,"model":"Samsung SSD 860","rm":false,"type":"disk",
           "mountpoints":[null],
           "children":[
              {"name":"sda1","size":536870912,"model":null,"rm":false,"type":"part","mountpoints":["/boot"]},
              {"name":"sda2","size":499570917376,"model":null,"rm":false,"type":"part","mountpoints":["/"]}
           ]},
          {"name":"nvme0n1","size":2000398934016,"model":"WD Black SN850X ","rm":false,"type":"disk",
           "mountpoints":[null]},
          {"name":"sdb","size":16000000000,"model":"USB DISK 3.0","rm":true,"type":"disk",
           "mountpoints":[null]}
       ]
    }"#;

    #[test]
    fn keeps_only_whole_disks() {
        let disks = parse_disks(SAMPLE).expect("échantillon valide");
        let devices: Vec<&str> = disks.iter().map(|d| d.device.as_str()).collect();
        assert_eq!(devices, vec!["/dev/sda", "/dev/nvme0n1", "/dev/sdb"]);
    }

    #[test]
    fn filters_out_loop_devices() {
        // The live medium itself appears as a loop device; offering it would
        // let the user install over the medium they are installing from.
        let disks = parse_disks(SAMPLE).unwrap();
        assert!(!disks.iter().any(|d| d.device.contains("loop")));
    }

    #[test]
    fn detects_a_disk_in_use() {
        let disks = parse_disks(SAMPLE).unwrap();
        let sda = disks.iter().find(|d| d.device == "/dev/sda").unwrap();
        assert!(sda.in_use, "sda porte des partitions montées");

        let nvme = disks.iter().find(|d| d.device == "/dev/nvme0n1").unwrap();
        assert!(!nvme.in_use);
    }

    #[test]
    fn detects_removable_media() {
        let disks = parse_disks(SAMPLE).unwrap();
        assert!(disks.iter().find(|d| d.device == "/dev/sdb").unwrap().removable);
        assert!(!disks.iter().find(|d| d.device == "/dev/nvme0n1").unwrap().removable);
    }

    #[test]
    fn trims_model_names_and_falls_back() {
        let disks = parse_disks(SAMPLE).unwrap();
        // The trailing space in the lsblk output must not survive.
        let nvme = disks.iter().find(|d| d.device == "/dev/nvme0n1").unwrap();
        assert_eq!(nvme.model, "WD Black SN850X");
    }

    #[test]
    fn reports_sizes_in_mib() {
        let disks = parse_disks(SAMPLE).unwrap();
        let nvme = disks.iter().find(|d| d.device == "/dev/nvme0n1").unwrap();
        // 2 000 398 934 016 bytes.
        assert_eq!(nvme.size_mib(), 1_907_729);
    }

    #[test]
    fn detects_a_mount_nested_several_levels_down() {
        // LVM and LUKS both nest: disk -> partition -> container -> volume.
        // Stopping one level early would report the disk as free.
        let json = r#"{
          "blockdevices": [
            {"name":"sda","type":"disk","mountpoints":[null],
             "children":[
               {"name":"sda3","type":"part","mountpoints":[null],
                "children":[
                  {"name":"cryptroot","type":"crypt","mountpoints":[null],
                   "children":[
                     {"name":"root","type":"lvm","mountpoints":["/"]}
                   ]}
                ]}
             ]}
          ]
        }"#;
        let disks = parse_disks(json).unwrap();
        assert!(disks[0].in_use, "un montage imbriqué doit être détecté");
    }

    #[test]
    fn an_unmounted_disk_is_not_in_use() {
        // The inverse matters just as much: a false positive would make the
        // installer refuse to install anywhere.
        let json = r#"{
          "blockdevices": [
            {"name":"sdb","type":"disk","mountpoints":[null],
             "children":[{"name":"sdb1","type":"part","mountpoints":[null]}]}
          ]
        }"#;
        let disks = parse_disks(json).unwrap();
        assert!(!disks[0].in_use);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        // A device with no model, no size and no mountpoints must not panic.
        let json = r#"{"blockdevices":[{"name":"vda","type":"disk"}]}"#;
        let disks = parse_disks(json).unwrap();
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].model, "Disque");
        assert_eq!(disks[0].size_bytes, 0);
        assert!(!disks[0].in_use);
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_disks("pas du json").is_err());
    }

    #[test]
    fn empty_output_is_not_an_error() {
        let disks = parse_disks(r#"{"blockdevices":[]}"#).unwrap();
        assert!(disks.is_empty());
    }
}
