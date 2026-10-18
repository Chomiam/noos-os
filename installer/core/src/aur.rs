//! AUR packages built when the ISO is built, and shipped on the live medium.
//!
//! Installing them by building from the AUR during installation was the wrong
//! place for it. All three are heavy compilations, and each needs a different
//! toolchain the target has no other use for:
//!
//! - `limine-snapper-sync` and `limine-mkinitcpio-hook` build a **GraalVM
//!   native image**: a 300 MB JDK download, Gradle, and an ahead-of-time
//!   compile. Under `makepkg` that build fails with `native-image` exiting 30
//!   and no message at all, while the identical Gradle command run by hand
//!   succeeds — a difference that survived every environment comparison.
//! - `shelly` builds with Zig 0.16, clang and GTK4.
//!
//! The installer therefore installs packaged files from the medium first and
//! only falls back to building from the AUR for whatever is missing. An ISO
//! built without the packages still produces a working system — a slower
//! installation, and the snapshot menu absent — rather than a failed one.
//!
//! `inotify-tools` is *not* in this list: it is a repository package. It used
//! to be installed by the AUR step, which meant a failed AUR step left the
//! snapshot watcher inert with nothing to say so.

/// Where the live medium carries the packages, and where they are staged
/// inside the target before `pacman -U` runs on them.
///
/// One constant because it is one directory: the installer copies the tree
/// across and installs from the copy, so the paths must agree.
pub const DIR: &str = "/var/cache/noos-aur";

/// Suffix of a pacman package file.
pub const PACKAGE_SUFFIX: &str = ".pkg.tar.zst";

/// What `makepkg` appends to the debug package it produces beside every build.
///
/// Those are hundreds of megabytes of debug symbols that no user asked for,
/// and — being named `<package>-debug-…` — they would also make
/// [`is_shipped`] believe the real package was built.
pub const DEBUG_MARKER: &str = "-debug-";

/// The AUR packages this image ships prebuilt.
pub const PACKAGES: &[&str] = &[
    // The AUR helper itself. A prebuilt binary package, but still an AUR
    // package, so it is built once with everything else.
    "yay-bin",
    // The snapshot chain — the point of the distribution.
    "limine-mkinitcpio-hook",
    "limine-snapper-sync",
    // The graphical package manager the desktop otherwise lacks.
    "shelly",
];

/// Filters a directory listing down to the package files worth installing.
///
/// Split out from the directory read so the selection is testable without a
/// filesystem: what arrives is a list of file names, what leaves is the subset
/// that is a pacman package and not a debug one.
pub fn package_files(names: &[String]) -> Vec<String> {
    let mut files: Vec<String> = names
        .iter()
        .filter(|name| name.ends_with(PACKAGE_SUFFIX))
        .filter(|name| !name.contains(DEBUG_MARKER))
        .cloned()
        .collect();
    files.sort();
    files
}

/// The package name out of a pacman package file name.
///
/// A pacman file is `<name>-<pkgver>-<pkgrel>-<arch>.pkg.tar.zst`, and only the
/// **end** of it is predictable. `pkgver` may not contain a dash — makepkg
/// replaces one with an underscore — but the package name may, and does:
/// `limine-snapper-sync` is exactly that case. So the fields are taken from the
/// right, three of them, and what is left over has its own trailing version
/// trimmed.
///
/// Prefix matching instead would let `limine-snapper-sync-git-1.32.1-1-x86_64`
/// satisfy a request for `limine-snapper-sync`, and the installer would skip
/// the package it was asked for.
pub fn package_name(file: &str) -> Option<&str> {
    let stem = file.strip_suffix(PACKAGE_SUFFIX)?;
    // arch, then pkgrel, then the remainder — from the right.
    let mut fields = stem.rsplitn(3, '-');
    fields.next()?;
    fields.next()?;
    let name_and_version = fields.next()?;
    name_and_version.rsplit_once('-').map(|(name, _version)| name)
}

/// Whether `package` is among the files the medium carries.
pub fn is_shipped(package: &str, files: &[String]) -> bool {
    files
        .iter()
        .filter_map(|file| package_name(file))
        .any(|name| name == package)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn only_package_files_are_selected() {
        let listing = names(&[
            "yay-bin-13.0.1-1-x86_64.pkg.tar.zst",
            "shelly-3.1.7-1-x86_64.pkg.tar.zst",
            "shelly-3.1.7-1-x86_64.pkg.tar.zst.sig",
            "SHA256SUMS",
            ".gitignore",
        ]);
        assert_eq!(
            package_files(&listing),
            names(&[
                "shelly-3.1.7-1-x86_64.pkg.tar.zst",
                "yay-bin-13.0.1-1-x86_64.pkg.tar.zst",
            ])
        );
    }

    #[test]
    fn debug_packages_are_left_out() {
        // makepkg emits one of these beside every build. Installing them costs
        // hundreds of megabytes and installs nothing anybody uses.
        let listing = names(&[
            "shelly-3.1.7-1-x86_64.pkg.tar.zst",
            "shelly-debug-3.1.7-1-x86_64.pkg.tar.zst",
        ]);
        assert_eq!(
            package_files(&listing),
            names(&["shelly-3.1.7-1-x86_64.pkg.tar.zst"])
        );
    }

    #[test]
    fn a_debug_package_alone_does_not_count_as_the_package() {
        // The subtle half of the point above: were the debug file kept in the
        // listing, this would report `shelly` as shipped and the installer
        // would skip it entirely.
        let listing = package_files(&names(&["shelly-debug-3.1.7-1-x86_64.pkg.tar.zst"]));
        assert!(!is_shipped("shelly", &listing));
    }

    #[test]
    fn a_name_that_contains_a_dash_survives() {
        assert_eq!(
            package_name("limine-snapper-sync-1.32.1-1-x86_64.pkg.tar.zst"),
            Some("limine-snapper-sync")
        );
        assert_eq!(
            package_name("yay-bin-13.0.1-1-x86_64.pkg.tar.zst"),
            Some("yay-bin")
        );
        assert_eq!(package_name("shelly-3.1.7-1-any.pkg.tar.zst"), Some("shelly"));
    }

    #[test]
    fn a_file_that_is_not_a_package_has_no_name() {
        assert_eq!(package_name("README.md"), None);
        assert_eq!(package_name("shelly.pkg.tar.zst"), None);
    }

    #[test]
    fn a_prefix_is_not_a_match() {
        // `limine-snapper-sync-git` must not satisfy `limine-snapper-sync`.
        let listing = package_files(&names(&["limine-snapper-sync-git-1.32.1-1-x86_64.pkg.tar.zst"]));
        assert!(!is_shipped("limine-snapper-sync", &listing));
    }

    #[test]
    fn every_shipped_package_is_recognised() {
        let listing = package_files(&names(&[
            "yay-bin-13.0.1-1-x86_64.pkg.tar.zst",
            "limine-mkinitcpio-hook-1.40.0-1-x86_64.pkg.tar.zst",
            "limine-snapper-sync-1.32.1-1-x86_64.pkg.tar.zst",
            "shelly-3.1.7-1-x86_64.pkg.tar.zst",
        ]));
        for package in PACKAGES {
            assert!(
                is_shipped(package, &listing),
                "{package} devrait être reconnu dans {listing:?}"
            );
        }
    }

    #[test]
    fn inotify_tools_is_not_an_aur_package() {
        // It lives in `extra`. Routing it through the AUR step is what left the
        // snapshot watcher inert whenever that step failed.
        assert!(!PACKAGES.contains(&"inotify-tools"));
    }
}
