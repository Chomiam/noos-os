//! The Chaotic-AUR binary repository.
//!
//! The AUR is source-only: anything taken from it has to be compiled on the
//! machine that wants it. Three of the packages this distribution needs build
//! a **GraalVM native image** or a Zig binary, and the GraalVM build fails under
//! `makepkg` with `native-image` exiting 30 and no message at all — a wall the
//! project hit repeatedly and never got past. Chaotic-AUR builds those packages
//! and serves signed binaries, which removes the compiler from the picture
//! entirely.
//!
//! It is enabled on the target, not only at build time: the installer then
//! installs the snapshot chain and the AUR helper from it like any other
//! repository, and the user keeps a large source of prebuilt AUR packages
//! afterwards.
//!
//! **This is a third-party repository the distribution trusts by default.**
//! Every package from it is built by Chaotic's infrastructure, not by us and not
//! by the user. That is the trade: the alternative is shipping a compiler
//! toolchain in an installer and asking the machine to build AOT Java.

/// Path of the mirror list, owned by the `chaotic-mirrorlist` package.
///
/// `pacman.conf` names this file, so it has to be in place *before* the first
/// `pacman -Sy` that uses the repository — hence the copy from the live medium
/// in [`crate::install`] rather than relying on the package to install itself.
pub const MIRRORLIST_PATH: &str = "/etc/pacman.d/chaotic-mirrorlist";

/// The pacman configuration the section is appended to.
pub const PACMAN_CONF: &str = "/etc/pacman.conf";

/// The repository section, exactly as Chaotic documents it.
pub const SECTION: &str = "chaotic-aur";

/// The 32-bit repository section, enabled alongside Chaotic-AUR.
pub const MULTILIB_SECTION: &str = "multilib";
pub const MULTILIB_INCLUDE: &str = "/etc/pacman.d/mirrorlist";

/// The signing key, as Chaotic publishes it.
///
/// Checked against the key itself: `3056513887B78AEB`, a 2048-bit RSA key
/// created in 2018 under `Pedro Henrique Lara Campos <root@pedrohlc.com>`,
/// fingerprint `EF925EA60F33D0CB85C44AD13056513887B78AEB`. It is the key the
/// `chaotic-keyring` package carries; fetching it by short id from a keyserver
/// is what bootstraps the trust, and it happens on the **build host** — never on
/// the target, which inherits the live keyring through `pacstrap`.
pub const KEY_ID: &str = "3056513887B78AEB";

/// Where the key is fetched from. The pool Arch uses is not reachable for this
/// key, which is why the project's own documentation names this one.
pub const KEYSERVER: &str = "keyserver.ubuntu.com";

/// Packages that must be installed for the repository to be usable at all.
///
/// The keyring is what keeps signature verification working after a rotation,
/// and the mirror list is what `[chaotic-aur]` includes.
pub const REPO_PACKAGES: &[&str] = &["chaotic-keyring", "chaotic-mirrorlist"];

/// The `[chaotic-aur]` section, ready to append to a `pacman.conf`.
pub fn section() -> String {
    format!(
        "\n# Binary AUR repository. See installer/core/src/chaotic.rs for why the\n\
         # distribution relies on it rather than compiling these packages itself.\n\
         [{SECTION}]\n\
         Include = {MIRRORLIST_PATH}\n"
    )
}

/// Enables `[multilib]` and adds `[chaotic-aur]` to a `pacman.conf`, leaving
/// everything else untouched.
///
/// Idempotent, and written as a pure function for the same reason
/// [`crate::locale::enable`] is: this is a file the `pacman` package owns, and
/// the only safe way to change it is to take the file's own contents, add a
/// line, and give them back.
pub fn enable(pacman_conf: &str) -> String {
    let mut out = enable_multilib(pacman_conf);
    if is_enabled(&out) {
        return out;
    }

    // A file that does not end in a newline would otherwise have the section
    // glued onto its last line, and pacman would read that line as garbage.
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&section());
    out
}

/// Enables `[multilib]` in a `pacman.conf` by uncommenting the stock block if
/// present, or appending the section if absent.
pub fn enable_multilib(pacman_conf: &str) -> String {
    if is_multilib_enabled(pacman_conf) {
        return pacman_conf.to_string();
    }

    const COMMENTED: &str = "#[multilib]\n#Include = /etc/pacman.d/mirrorlist";
    const UNCOMMENTED: &str = "[multilib]\nInclude = /etc/pacman.d/mirrorlist";

    if pacman_conf.contains(COMMENTED) {
        return pacman_conf.replacen(COMMENTED, UNCOMMENTED, 1);
    }

    let mut out = pacman_conf.to_string();
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "\n[{MULTILIB_SECTION}]\nInclude = {MULTILIB_INCLUDE}\n"
    ));
    out
}

/// Whether `[multilib]` is already enabled.
pub fn is_multilib_enabled(pacman_conf: &str) -> bool {
    pacman_conf
        .lines()
        .any(|line| line.trim() == format!("[{MULTILIB_SECTION}]"))
}

/// Whether the section is already present.
///
/// Matched as a whole line: `#[chaotic-aur]` in a comment, or a longer section
/// name that merely contains this one, must not count as enabled.
pub fn is_enabled(pacman_conf: &str) -> bool {
    pacman_conf
        .lines()
        .any(|line| line.trim() == format!("[{SECTION}]"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real `pacman.conf`, comments and sections included.
    const SAMPLE: &str = "\
#
# /etc/pacman.conf
#
[options]
HoldPkg = pacman glibc
SigLevel = Required DatabaseOptional

[core]
Include = /etc/pacman.d/mirrorlist

[extra]
Include = /etc/pacman.d/mirrorlist
";

    #[test]
    fn the_section_is_added_once() {
        let once = enable(SAMPLE);
        assert_eq!(enable(&once), once, "enable doit être idempotent");
        assert_eq!(
            once.matches("[chaotic-aur]").count(),
            1,
            "la section apparaît plusieurs fois :\n{once}"
        );
    }

    #[test]
    fn everything_else_survives() {
        let out = enable(SAMPLE);
        for line in SAMPLE.lines() {
            assert!(out.contains(line), "ligne perdue : {line:?}");
        }
    }

    #[test]
    fn the_include_points_at_the_mirrorlist() {
        let out = enable(SAMPLE);
        assert!(out.contains(&format!("Include = {MIRRORLIST_PATH}")));
        assert!(out.ends_with('\n'), "pacman.conf doit finir par un saut de ligne");
    }

    #[test]
    fn a_file_without_a_trailing_newline_is_still_readable() {
        // Appending to the last line would produce `Include = …[chaotic-aur]`.
        let out = enable("[options]\nSigLevel = Required");
        assert!(out.contains("SigLevel = Required\n"), "ligne collée :\n{out}");
        assert!(is_enabled(&out));
    }

    #[test]
    fn a_comment_does_not_count_as_enabled() {
        assert!(!is_enabled("# [chaotic-aur]\n"));
        assert!(is_enabled("[chaotic-aur]\n"));
    }

    #[test]
    fn the_key_is_the_one_chaotic_publishes() {
        // A wrong key here means every download is rejected, and the failure
        // surfaces as "invalid or corrupted package" rather than as a wrong key.
        assert_eq!(KEY_ID, "3056513887B78AEB");
        assert_eq!(KEY_ID.len(), 16, "identifiant de clé de 16 caractères attendu");
    }

    #[test]
    fn the_repository_carries_its_own_keyring() {
        // Without `chaotic-keyring` installed, signature verification stops
        // working the moment Chaotic rotates the key.
        assert!(REPO_PACKAGES.contains(&"chaotic-keyring"));
        assert!(REPO_PACKAGES.contains(&"chaotic-mirrorlist"));
    }

    #[test]
    fn multilib_is_enabled_in_place_without_touching_multilib_testing() {
        let stock = format!(
            "{SAMPLE}\n#[multilib-testing]\n#Include = /etc/pacman.d/mirrorlist\n\n\
             #[multilib]\n#Include = /etc/pacman.d/mirrorlist\n"
        );
        let out = enable(&stock);
        assert!(is_multilib_enabled(&out), "multilib devrait être activé :\n{out}");
        assert!(
            out.contains("#[multilib-testing]\n#Include = /etc/pacman.d/mirrorlist"),
            "multilib-testing doit rester commenté :\n{out}"
        );
        assert_eq!(out.matches("[multilib]").count(), 1);
    }
}
