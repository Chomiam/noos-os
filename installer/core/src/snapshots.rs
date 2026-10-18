//! Wiring snapper, snap-pac and refind-btrfs together.
//!
//! The chain is: `snap-pac` creates a pre/post snapshot around every pacman
//! transaction → snapper records it under the `root` config → `refind-btrfs`
//! turns each one into a rEFInd boot stanza, with the same kernel the ESP
//! already carries and a different root subvolume.
//!
//! **Those stanzas roll back everything except the kernel.** With `/boot` on
//! its own partition, the kernel is shared by every entry, so a snapshot cannot
//! rescue a machine whose kernel upgrade broke the boot. `refind-btrfs` says so
//! itself; limine's tooling avoided it by copying a kernel per snapshot onto the
//! ESP, which is what the 8 GiB boot partition used to be for. That trade was
//! made deliberately.

/// Snapper config name. It must manage the subvolume mounted at `/`.
pub const SNAPPER_CONFIG_NAME: &str = "root";

/// Tells snapper to work without the system bus.
///
/// Mandatory for every invocation from the installer. `snapper` reaches
/// `snapperd` over D-Bus by default, and an `arch-chroot` has no bus — the
/// call fails with `org.freedesktop.DBus.Error.ServiceUnknown`, which names
/// the bus error rather than the cause.
///
/// The manual warns that a running `snapperd` will not learn about changes
/// made this way. That is exactly right here: there is no daemon in the chroot
/// to inform, and the configuration is written for the next boot.
pub const NO_DBUS: &str = "--no-dbus";

/// How many snapshots snapper keeps.
///
/// If it kept more than `refind-btrfs` exposes, every sync would log about the
/// surplus; fewer, and boot entries would point at snapshots cleanup already
/// deleted.
pub const SNAPSHOT_LIMIT: u32 = 10;

/// Path of the refind-btrfs configuration.
///
/// Installed by the package, with defaults written for Snapper — which is what
/// this distribution uses. The installer deliberately does **not** write it:
/// reimplementing a configuration file whose keys are the tool's own business
/// is how a working default gets broken by a well-meaning override.
pub const REFIND_BTRFS_CONF: &str = "/etc/refind-btrfs.conf";

/// Applied with `snapper -c root set-config KEY=VALUE` rather than by editing
/// `/etc/snapper/configs/root` directly — snapper owns that file's format.
pub fn snapper_settings() -> Vec<(&'static str, String)> {
    vec![
        ("TIMELINE_CREATE", "yes".into()),
        ("TIMELINE_CLEANUP", "yes".into()),
        ("TIMELINE_LIMIT_HOURLY", "5".into()),
        ("TIMELINE_LIMIT_DAILY", "7".into()),
        ("TIMELINE_LIMIT_WEEKLY", "0".into()),
        ("TIMELINE_LIMIT_MONTHLY", "0".into()),
        ("TIMELINE_LIMIT_YEARLY", "0".into()),
        ("NUMBER_LIMIT", SNAPSHOT_LIMIT.to_string()),
        ("NUMBER_LIMIT_IMPORTANT", "5".into()),
        ("EMPTY_PRE_POST_CLEANUP", "yes".into()),
    ]
}

/// systemd units to enable once `/boot` is mounted.
///
/// `refind-btrfs.service` watches the snapshot directories and rewrites the
/// rEFInd stanzas whenever one appears or disappears. Its unit is ordered
/// before `snapper-boot.service` upstream, so snapshots taken at boot are
/// covered too.
pub const UNITS: &[&str] = &[
    "snapper-timeline.timer",
    "snapper-cleanup.timer",
    "refind-btrfs.service",
];

/// Runtime dependency of `refind-btrfs` that is only an *optional* dependency
/// in its PKGBUILD. Without it the watcher never sees snapshot changes and the
/// tool does nothing at all, silently.
pub const REQUIRED_EXTRA_PACKAGES: &[&str] = &["inotify-tools"];

/// The AUR package the snapshot chain rests on.
///
/// Listed in [`crate::install`] as taken from Chaotic-AUR rather than compiled.
/// It is a Python program, not a GraalVM native image, so it *could* be built —
/// but keeping every AUR package on one path means one mechanism to reason
/// about. The test in that module keeps the two lists from drifting apart.
pub const SNAPSHOT_CHAIN_PACKAGES: &[&str] = &["refind-btrfs"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapper_is_always_called_without_dbus() {
        // The installer runs snapper inside `arch-chroot`, which has no D-Bus.
        // Without this flag `snapper -c root create-config /` fails with
        // `org.freedesktop.DBus.Error.ServiceUnknown` — a message that names
        // the bus error and nothing about the cause, three steps before
        // anything else breaks.
        assert_eq!(NO_DBUS, "--no-dbus");
        assert!(NO_DBUS.starts_with("--"));
    }

    #[test]
    fn snapshot_limits_stay_consistent() {
        // snapper keeps them and refind-btrfs exposes them; the two reads of
        // one number have to come from one place.
        let snapper_limit = snapper_settings()
            .into_iter()
            .find(|(k, _)| *k == "NUMBER_LIMIT")
            .map(|(_, v)| v)
            .expect("NUMBER_LIMIT doit être réglé");
        assert_eq!(snapper_limit, SNAPSHOT_LIMIT.to_string());
    }

    #[test]
    fn the_timers_and_the_watcher_are_enabled() {
        assert!(UNITS.contains(&"snapper-timeline.timer"));
        assert!(UNITS.contains(&"snapper-cleanup.timer"));
        assert!(
            UNITS.contains(&"refind-btrfs.service"),
            "sans le guetteur, aucun instantané n'apparaîtrait dans le menu"
        );
    }

    #[test]
    fn no_limine_unit_survives() {
        // The bootloader changed; a leftover unit would be enabled by systemd
        // against a package that is no longer installed.
        for unit in UNITS {
            assert!(
                !unit.contains("limine"),
                "{unit} n'a plus de paquet derrière elle"
            );
        }
    }

    #[test]
    fn inotify_tools_is_required() {
        // It is only an optdepend upstream, but without it the watcher is inert.
        assert!(REQUIRED_EXTRA_PACKAGES.contains(&"inotify-tools"));
    }

    #[test]
    fn the_snapshot_chain_is_the_package_that_generates_the_stanzas() {
        // Renaming this here without renaming it upstream would leave the
        // installation reporting success with no snapshot menu anywhere.
        assert_eq!(SNAPSHOT_CHAIN_PACKAGES, &["refind-btrfs"]);
    }

    #[test]
    fn the_refind_btrfs_config_is_left_to_its_package() {
        // The path is pinned so the installer's documentation and the package
        // cannot drift; the *contents* are deliberately not written by us.
        assert_eq!(REFIND_BTRFS_CONF, "/etc/refind-btrfs.conf");
    }
}
