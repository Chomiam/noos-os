//! Kernel choice.
//!
//! The kernel is not just another package: its name appears in the package
//! list, in the matching headers package, in the initramfs presets and in the
//! paths `limine.conf` boots from. Getting the names out of sync — installing
//! `linux-zen` but pointing limine at `vmlinuz-linux` — produces a system that
//! installs cleanly and then does not boot, so every derived name is computed
//! here rather than spelled out at each use site.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kernel {
    /// The rolling kernel. Newest drivers and features, and the one Arch
    /// itself targets.
    Linux,
    /// Long-term support. Older, far less churn — the safe choice on hardware
    /// that already works.
    Lts,
    /// Patched for desktop responsiveness, with a scheduler tuned for
    /// interactivity rather than throughput.
    Zen,
}

impl Kernel {
    pub fn all() -> [Kernel; 3] {
        [Kernel::Linux, Kernel::Lts, Kernel::Zen]
    }

    /// The kernel package itself.
    pub fn package(self) -> &'static str {
        match self {
            Kernel::Linux => "linux",
            Kernel::Lts => "linux-lts",
            Kernel::Zen => "linux-zen",
        }
    }

    /// The matching headers package.
    ///
    /// Must match the kernel exactly: DKMS modules are built against these,
    /// and headers from a different version produce modules the kernel
    /// refuses to load. Note they are *not* dependencies of the kernel, so
    /// they have to be requested explicitly.
    pub fn headers(self) -> &'static str {
        match self {
            Kernel::Linux => "linux-headers",
            Kernel::Lts => "linux-lts-headers",
            Kernel::Zen => "linux-zen-headers",
        }
    }

    /// Kernel image, as it appears on the boot partition.
    pub fn image(self) -> &'static str {
        match self {
            Kernel::Linux => "vmlinuz-linux",
            Kernel::Lts => "vmlinuz-linux-lts",
            Kernel::Zen => "vmlinuz-linux-zen",
        }
    }

    /// Initramfs, as it appears on the boot partition.
    ///
    /// Note the naming is not symmetrical with the image: the LTS kernel is
    /// `vmlinuz-linux-lts` but `initramfs-linux-lts.img`.
    pub fn initramfs(self) -> &'static str {
        match self {
            Kernel::Linux => "initramfs-linux.img",
            Kernel::Lts => "initramfs-linux-lts.img",
            Kernel::Zen => "initramfs-linux-zen.img",
        }
    }

    /// Name used in the limine menu entry.
    pub fn label(self) -> &'static str {
        match self {
            Kernel::Linux => "linux",
            Kernel::Lts => "linux-lts",
            Kernel::Zen => "linux-zen",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_match_the_repositories() {
        assert_eq!(Kernel::Linux.package(), "linux");
        assert_eq!(Kernel::Lts.package(), "linux-lts");
        assert_eq!(Kernel::Zen.package(), "linux-zen");
    }

    #[test]
    fn headers_are_derived_consistently() {
        // Every kernel must have a headers package that exists, and the names
        // must stay in step — a mismatch is a silent DKMS failure.
        assert_eq!(Kernel::Linux.headers(), "linux-headers");
        assert_eq!(Kernel::Lts.headers(), "linux-lts-headers");
        assert_eq!(Kernel::Zen.headers(), "linux-zen-headers");
    }

    #[test]
    fn image_and_initramfs_are_derived_from_the_kernel_name() {
        assert_eq!(Kernel::Linux.image(), "vmlinuz-linux");
        assert_eq!(Kernel::Lts.image(), "vmlinuz-linux-lts");
        assert_eq!(Kernel::Zen.image(), "vmlinuz-linux-zen");

        assert_eq!(Kernel::Linux.initramfs(), "initramfs-linux.img");
        assert_eq!(Kernel::Lts.initramfs(), "initramfs-linux-lts.img");
        assert_eq!(Kernel::Zen.initramfs(), "initramfs-linux-zen.img");
    }

    #[test]
    fn the_default_kernel_is_plain_linux() {
        // The names for the default kernel must stay unsuffixed: releng, the
        // Arch wiki and every third-party guide assume them.
        assert_eq!(Kernel::Linux.image(), "vmlinuz-linux");
        assert_eq!(Kernel::Linux.initramfs(), "initramfs-linux.img");
    }

    #[test]
    fn initramfs_name_is_the_image_name_with_a_different_prefix() {
        // Verified against a real mkinitcpio preset, which carries
        // ALL_kver="/boot/vmlinuz-linux-zen" — the two names are the same
        // string with a different prefix, and the initramfs adds `.img`.
        //
        // Pinned because the two are written out separately below, and a typo
        // in one of them produces a limine entry pointing at a file that does
        // not exist: the install succeeds and the machine does not boot.
        for kernel in Kernel::all() {
            let expected = format!(
                "{}.img",
                kernel.image().replace("vmlinuz", "initramfs")
            );
            assert_eq!(
                kernel.initramfs(),
                expected,
                "nommage incohérent pour {:?}",
                kernel
            );
        }
    }

    #[test]
    fn all_kernels_are_distinct() {
        let kernels = Kernel::all();
        for (index, kernel) in kernels.iter().enumerate() {
            for other in &kernels[index + 1..] {
                assert_ne!(kernel.package(), other.package());
                assert_ne!(kernel.image(), other.image());
                assert_ne!(kernel.initramfs(), other.initramfs());
                assert_ne!(kernel.headers(), other.headers());
            }
        }
    }
}
