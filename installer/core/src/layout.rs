//! Partition planning.
//!
//! Pure arithmetic on a disk size — no `lsblk`, no `sgdisk` — so the sizing
//! rules can be tested anywhere.

use crate::error::{Error, Result};

/// 1 GiB.
///
/// This used to be 8 GiB, because `limine-snapper-sync` copied a kernel *and*
/// an initramfs onto the boot partition for every snapshot it made bootable.
/// rEFInd works differently: one kernel is shared by every entry, and only the
/// root subvolume changes between them. So the partition holds the kernel,
/// its initramfs, the CPU microcode and rEFInd's own files — a few hundred
/// megabytes, with room for several kernel versions.
///
/// The trade this buys: snapshots no longer roll back the kernel. A kernel
/// upgrade that breaks the boot can no longer be escaped by booting an older
/// snapshot. See [`crate::snapshots`].
pub const ESP_SIZE_MIB: u64 = 1024;

/// 8 GiB, as requested. A partition rather than a swapfile on purpose: a
/// subvolume holding an active swapfile cannot be snapshotted, so a swapfile
/// inside `@` would break the whole snapper chain.
pub const SWAP_SIZE_MIB: u64 = 8 * 1024;

/// Refuse to install onto something that cannot hold a usable system.
pub const MIN_ROOT_MIB: u64 = 20 * 1024;

/// Room for the primary GPT header.
const FIRST_USABLE_MIB: u64 = 1;
/// Room for the backup GPT header at the end of the disk.
const GPT_TAIL_MIB: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionKind {
    /// FAT32, mounted at `/boot`. Not a choice: the firmware reads FAT
    /// natively and rEFInd inherits that, whereas a `/boot` on btrfs would
    /// need rEFInd's own driver — which the Arch wiki warns misreads a
    /// copy-on-write or deduplicated volume.
    Esp,
    /// btrfs, holding every subvolume.
    Btrfs,
    Swap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    /// 1-based GPT partition number.
    pub number: u32,
    pub name: String,
    pub start_mib: u64,
    pub size_mib: u64,
    pub kind: PartitionKind,
}

impl Partition {
    pub fn end_mib(&self) -> u64 {
        self.start_mib + self.size_mib
    }

    /// Size in bytes, for `sgdisk`/`mkfs` invocations.
    pub fn size_bytes(&self) -> u64 {
        self.size_mib * 1024 * 1024
    }

    /// The `-n` argument for `sgdisk`.
    ///
    /// Expressed as *start plus size*, not as start and end.
    ///
    /// `sgdisk`'s end position is **inclusive**, while this module's
    /// [`end_mib`](Self::end_mib) is the end of a half-open range. Passing the
    /// latter as the end made consecutive partitions claim the same sector —
    /// partition 1 ended at sector 16779264 and partition 2 started there —
    /// and sgdisk refused the second one with "Could not create partition 2".
    ///
    /// The `+` prefix makes the second field a size rather than a position, so
    /// the half-open model is preserved and there is no off-by-one to get
    /// wrong.
    pub fn sgdisk_range(&self) -> String {
        format!("{}:{}MiB:+{}MiB", self.number, self.start_mib, self.size_mib)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionPlan {
    pub esp: Partition,
    pub swap: Option<Partition>,
    pub root: Partition,
}

impl PartitionPlan {
    /// Every partition, in on-disk order.
    pub fn all(&self) -> Vec<&Partition> {
        let mut parts = vec![&self.esp];
        if let Some(swap) = &self.swap {
            parts.push(swap);
        }
        parts.push(&self.root);
        parts
    }
}

/// Smallest disk this installer will accept, in MiB.
pub fn minimum_disk_mib(with_swap: bool) -> u64 {
    FIRST_USABLE_MIB
        + ESP_SIZE_MIB
        + if with_swap { SWAP_SIZE_MIB } else { 0 }
        + MIN_ROOT_MIB
        + GPT_TAIL_MIB
}

/// Lays out a single-disk, whole-disk-wipe install.
///
/// Order is ESP, then swap, then root last so that root absorbs whatever is
/// left — the disk size is never known exactly in advance.
pub fn plan(disk_mib: u64, with_swap: bool) -> Result<PartitionPlan> {
    let required = minimum_disk_mib(with_swap);
    if disk_mib < required {
        return Err(Error::Layout(format!(
            "disque trop petit : {} disponibles, {} requis{}",
            gib(disk_mib),
            gib(required),
            if with_swap {
                " (dont 8 Gio de swap — décoche la swap pour abaisser le minimum)"
            } else {
                ""
            }
        )));
    }

    // Leave the trailing MiB for the backup GPT header.
    let usable_end = disk_mib - GPT_TAIL_MIB;

    let esp = Partition {
        number: 1,
        name: "NOOS-ESP".into(),
        start_mib: FIRST_USABLE_MIB,
        size_mib: ESP_SIZE_MIB,
        kind: PartitionKind::Esp,
    };

    let swap = with_swap.then(|| Partition {
        number: 2,
        name: "NOOS-SWAP".into(),
        start_mib: esp.end_mib(),
        size_mib: SWAP_SIZE_MIB,
        kind: PartitionKind::Swap,
    });

    let root_start = swap.as_ref().map_or(esp.end_mib(), Partition::end_mib);
    let root = Partition {
        number: if with_swap { 3 } else { 2 },
        name: "NOOS-ROOT".into(),
        start_mib: root_start,
        size_mib: usable_end - root_start,
        kind: PartitionKind::Btrfs,
    };

    Ok(PartitionPlan { esp, swap, root })
}

/// Formats a MiB count as GiB with one decimal, for error messages.
fn gib(mib: u64) -> String {
    format!("{:.1} Gio", mib as f64 / 1024.0)
}

/// Builds the device node for a partition of a disk.
///
/// NVMe, MMC and loop devices put a `p` before the partition number
/// (`/dev/nvme0n1p2`); SATA and virtio do not (`/dev/sda2`). Getting this
/// wrong means partitioning one device and formatting another, so it is worth
/// being explicit about.
pub fn partition_path(disk: &str, number: u32) -> String {
    let name = disk.rsplit('/').next().unwrap_or(disk);
    let ends_in_digit = name.chars().next_back().is_some_and(|c| c.is_ascii_digit());
    if ends_in_digit {
        format!("{disk}p{number}")
    } else {
        format!("{disk}{number}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024;

    #[test]
    fn lays_out_esp_swap_root_in_order() {
        let plan = plan(500 * GIB, true).expect("500 Gio devrait suffire");

        assert_eq!(plan.esp.number, 1);
        assert_eq!(plan.esp.size_mib, ESP_SIZE_MIB);
        assert_eq!(plan.esp.kind, PartitionKind::Esp);

        let swap = plan.swap.as_ref().expect("swap demandée");
        assert_eq!(swap.number, 2);
        assert_eq!(swap.size_mib, SWAP_SIZE_MIB);
        assert_eq!(swap.start_mib, plan.esp.end_mib(), "les partitions ne doivent pas se chevaucher");

        assert_eq!(plan.root.number, 3);
        assert_eq!(plan.root.start_mib, swap.end_mib());
        assert_eq!(plan.root.kind, PartitionKind::Btrfs);

        // Root must absorb everything up to the backup GPT header.
        assert_eq!(plan.root.end_mib(), 500 * GIB - GPT_TAIL_MIB);
    }

    #[test]
    fn numbering_shifts_without_swap() {
        let plan = plan(500 * GIB, false).expect("500 Gio devrait suffire");
        assert!(plan.swap.is_none());
        assert_eq!(plan.root.number, 2);
        assert_eq!(plan.root.start_mib, plan.esp.end_mib());
    }

    #[test]
    fn no_gaps_and_no_overlaps() {
        let plan = plan(1000 * GIB, true).expect("plan valide");
        let parts = plan.all();
        assert_eq!(parts[0].start_mib, FIRST_USABLE_MIB);
        for pair in parts.windows(2) {
            assert_eq!(
                pair[0].end_mib(),
                pair[1].start_mib,
                "trou entre {} et {}",
                pair[0].name,
                pair[1].name
            );
        }
    }

    #[test]
    fn rejects_disks_below_the_minimum() {
        assert!(plan(8 * GIB, false).is_err());
        assert!(plan(20 * GIB, true).is_err());
    }

    #[test]
    fn accepts_exactly_the_minimum() {
        let without_swap = minimum_disk_mib(false);
        assert!(plan(without_swap, false).is_ok());
        assert!(plan(without_swap - 1, false).is_err());

        let with_swap = minimum_disk_mib(true);
        assert!(plan(with_swap, true).is_ok());
        assert!(plan(with_swap - 1, true).is_err());
    }

    #[test]
    fn swap_raises_the_minimum_by_its_own_size() {
        assert_eq!(
            minimum_disk_mib(true) - minimum_disk_mib(false),
            SWAP_SIZE_MIB
        );
    }

    #[test]
    fn error_message_names_the_shortfall() {
        let err = plan(10 * GIB, false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("trop petit"), "{msg}");
        assert!(msg.contains("requis"), "{msg}");
    }

    #[test]
    fn root_keeps_the_bulk_of_a_large_disk() {
        let plan = plan(2000 * GIB, true).expect("plan valide");
        assert!(plan.root.size_mib > 1900 * GIB);
    }

    #[test]
    fn partition_paths_match_the_device_naming_scheme() {
        // Devices ending in a digit need a separator before the number.
        assert_eq!(partition_path("/dev/nvme0n1", 1), "/dev/nvme0n1p1");
        assert_eq!(partition_path("/dev/nvme0n1", 3), "/dev/nvme0n1p3");
        assert_eq!(partition_path("/dev/mmcblk0", 2), "/dev/mmcblk0p2");
        assert_eq!(partition_path("/dev/loop0", 1), "/dev/loop0p1");
        // Others do not.
        assert_eq!(partition_path("/dev/sda", 1), "/dev/sda1");
        assert_eq!(partition_path("/dev/vda", 2), "/dev/vda2");
    }

    #[test]
    fn sgdisk_ranges_use_a_size_not_an_end_position() {
        // The bug this pins: sgdisk's end position is inclusive, while
        // `end_mib()` is the end of a half-open range. Passing the latter as
        // the end made partition 1 and partition 2 claim the same sector, and
        // sgdisk refused the second one outright — "Could not create
        // partition 2". Verified against sgdisk 1.0.10: the `+` form produces
        // contiguous partitions with no overlap.
        let plan = plan(500 * GIB, true).expect("plan valide");

        // Derived from the constants rather than written out: this assertion
        // used to spell out 8192, and changing the ESP size turned a passing
        // test into a failing one it had no business noticing.
        assert_eq!(
            plan.esp.sgdisk_range(),
            format!("1:{FIRST_USABLE_MIB}MiB:+{ESP_SIZE_MIB}MiB")
        );
        assert_eq!(
            plan.swap.as_ref().expect("swap").sgdisk_range(),
            format!(
                "2:{}MiB:+{SWAP_SIZE_MIB}MiB",
                FIRST_USABLE_MIB + ESP_SIZE_MIB
            )
        );

        // The `+` is what makes the difference; without it the value is read
        // as a position.
        for part in plan.all() {
            assert!(
                part.sgdisk_range().contains("MiB:+"),
                "{} : la taille doit être préfixée par +",
                part.name
            );
        }
    }

    #[test]
    fn consecutive_partitions_do_not_share_a_sector() {
        // The arithmetic behind the bug above, checked without sgdisk.
        let plan = plan(500 * GIB, true).expect("plan valide");
        let parts = plan.all();

        for pair in parts.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            // In the half-open model the end of one IS the start of the next,
            // which is correct — the bug was in how that was handed to sgdisk.
            assert_eq!(
                first.end_mib(),
                second.start_mib,
                "les plages doivent être contiguës"
            );
            // And the size must describe the range exactly.
            assert_eq!(first.start_mib + first.size_mib, first.end_mib());
        }
    }
}
