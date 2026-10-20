//! UEFI Secure Boot support through `shim-signed` and a local Machine Owner Key.
//!
//! A PC shipped with Secure Boot enabled trusts Microsoft's third-party UEFI CA
//! (`Microsoft Corporation UEFI CA 2011`) and nothing this distribution builds.
//! Replacing the platform's own keys (`PK`/`KEK`/`db`) via `sbctl` requires the
//! user to reboot into firmware setup, clear the factory keys into Setup Mode,
//! and enroll custom ones — a workflow that varies across every motherboard and
//! can brick Option ROMs (discrete GPUs, network cards) signed by Microsoft's CA
//! if Microsoft's certificates are omitted.
//!
//! `shim-signed` avoids touching firmware setup at all:
//!
//! 1. The firmware loads `shimx64.efi` (at `\EFI\refind\shimx64.efi` from the
//!    NVRAM entry, or `\EFI\BOOT\BOOTX64.EFI` from the removable-media fallback),
//!    which carries Microsoft's signature and passes the firmware's check out of
//!    the box.
//! 2. `shimx64.efi` verifies and launches `grubx64.efi` in the **same directory**
//!    — that filename is compiled into Fedora's shim binary, so rEFInd's signed
//!    binary sits there under that name as well as under `refind_x64.efi`.
//! 3. `rEFInd` hooks Shim's verification protocol (`SHIM_LOCK_GUID`) into the
//!    UEFI `Security2` architectural protocol, and verifies the kernel image
//!    (`/boot/vmlinuz-*`) against the same Machine Owner Key (MOK).
//!
//! Every machine generates its own RSA-2048 MOK keypair during installation in
//! [`KEYS_DIR`] (mode `0700`): no private signing key is ever shared across
//! installations or shipped on the live medium. The public DER certificate is
//! placed at `/boot/noos-mok.cer` on the FAT32 ESP so that on the first Secure
//! Boot startup, `mmx64.efi` (MokManager) lets the user enroll it with
//! *Enroll key from disk* using only the arrow and Enter keys — avoiding the
//! one-time password prompt that `mokutil --import` would force onto a US QWERTY
//! firmware keyboard.
//!
//! Finally, a pacman hook (`95-noos-secureboot.hook`) runs after `mkinitcpio`
//! (`90-mkinitcpio-install.hook`) on every kernel, `refind` or `shim-signed`
//! upgrade so that a `pacman -Syu` never leaves an unsigned kernel in `/boot`.

/// Directory holding the machine's own Secure Boot signing keypair (`0700`).
///
/// Matches the path `refind` itself creates (`/etc/refind.d/keys`), kept private
/// to root so an unprivileged account cannot sign arbitrary EFI binaries.
pub const KEYS_DIR: &str = "/etc/refind.d/keys";

/// Private RSA-2048 key (`0600`), used by `sbsign` and never copied to the ESP.
pub const MOK_KEY_PATH: &str = "/etc/refind.d/keys/noos-mok.key";

/// PEM certificate (`0644`), passed to `sbsign --cert` and `sbverify --cert`.
pub const MOK_CRT_PATH: &str = "/etc/refind.d/keys/noos-mok.crt";

/// DER certificate (`0644`), the format `mmx64.efi` (*Enroll key from disk*)
/// expects on the FAT32 ESP.
pub const MOK_CER_PATH: &str = "/etc/refind.d/keys/noos-mok.cer";

/// Filename of the DER certificate copied onto the root of the ESP (`/boot`)
/// and into `/boot/EFI/refind/keys/`.
pub const ESP_MOK_CER_NAME: &str = "noos-mok.cer";

/// Subject (Distinguished Name) of the locally-generated MOK certificate.
pub const MOK_SUBJECT: &str = "/CN=noos-os Secure Boot MOK/";

/// Where the `shim-signed` package installs its Microsoft-signed EFI binaries.
pub const SHIM_SOURCE_DIR: &str = "/usr/share/shim-signed";

/// First-stage bootloader signed by Microsoft's UEFI CA.
pub const SHIM_EFI: &str = "shimx64.efi";

/// MokManager binary signed by Microsoft's UEFI CA, launched automatically by
/// `shimx64.efi` when the second-stage loader's certificate is not yet in
/// `MokList`.
pub const MOK_MANAGER_EFI: &str = "mmx64.efi";

/// Filename `shimx64.efi` is compiled to load from its own directory.
///
/// This is hardcoded inside `shimx64.efi` (`\\grubx64.efi`), so our MOK-signed
/// copy of `refind_x64.efi` has to sit at this path beside `shimx64.efi`.
pub const SHIM_STAGE2_EFI: &str = "grubx64.efi";

/// Path of the signing script installed on the target (`0755`).
pub const SIGN_SCRIPT_PATH: &str = "/usr/local/bin/noos-sb-sign";

/// Directory where pacman reads local alpm hooks.
pub const PACMAN_HOOK_DIR: &str = "/etc/pacman.d/hooks";

/// Path of the pacman hook that re-signs kernels and rEFInd after upgrades.
///
/// Numbered `95-` deliberately: `90-mkinitcpio-install.hook` is what copies the
/// new kernel from `/usr/lib/modules/<ver>/vmlinuz` into `/boot/vmlinuz-*`. A
/// hook ordered before `90-` would sign the *previous* kernel on `/boot`, and
/// `mkinitcpio` would overwrite it with the unsigned new one moments later.
pub const PACMAN_HOOK_PATH: &str = "/etc/pacman.d/hooks/95-noos-secureboot.hook";

/// Official repository packages required on the target for key generation and
/// PE/COFF Authenticode signing.
pub const REQUIRED_PACKAGES: &[&str] = &["sbsigntools", "openssl"];

/// Chaotic-AUR package carrying the Microsoft-signed `shimx64.efi` and
/// `mmx64.efi` binaries.
pub const CHAOTIC_PACKAGES: &[&str] = &["shim-signed"];

/// Script that deploys `shim-signed` and signs rEFInd and every kernel in `/boot`.
///
/// Invoked once during installation (from `install_refind`) and thereafter by
/// [`PACMAN_HOOK`] on every kernel, `refind` or `shim-signed` upgrade.
pub const SIGN_SCRIPT: &str = "\
#!/bin/bash
#
# Generated by the noos-os installer.
#
# Signs rEFInd and every kernel image in /boot with the machine's local Machine
# Owner Key (MOK), and keeps the Microsoft-signed shim + MokManager binaries in
# sync on the ESP.

set -euo pipefail

readonly KEY=\"/etc/refind.d/keys/noos-mok.key\"
readonly CRT=\"/etc/refind.d/keys/noos-mok.crt\"
readonly CER=\"/etc/refind.d/keys/noos-mok.cer\"
readonly SHIM_SRC=\"/usr/share/shim-signed\"
readonly REFIND_BIN=\"/usr/share/refind/refind_x64.efi\"
readonly ESP=\"/boot\"

if [[ ! -f \"$KEY\" || ! -f \"$CRT\" ]]; then
    echo \"noos-sb-sign : clés MOK absentes dans /etc/refind.d/keys, signature ignorée\" >&2
    exit 0
fi

if [[ ! -d \"$ESP/EFI\" ]]; then
    echo \"noos-sb-sign : $ESP/EFI absent (/boot non monté ?), signature ignorée\" >&2
    exit 0
fi

install -d -m 0755 \"$ESP/EFI/refind\" \"$ESP/EFI/refind/keys\" \"$ESP/EFI/BOOT\"

# Sign a PE/COFF binary into a temporary file first, so an in-place signature on
# a kernel never truncates the input and a partial write never replaces a good
# binary.
sign_efi() {
    local src=\"$1\"
    local dst=\"$2\"
    local tmp=\"${dst}.tmp\"

    /usr/bin/sbsign --key \"$KEY\" --cert \"$CRT\" --output \"$tmp\" \"$src\" 2>&1 \\
        | grep -v \"gaps between PE/COFF sections\" || true
    if [[ ! -s \"$tmp\" ]]; then
        rm -f \"$tmp\"
        echo \"noos-sb-sign : échec de sbsign sur $src\" >&2
        return 1
    fi
    mv -f \"$tmp\" \"$dst\"
}

# 1. Microsoft-signed first stage (shimx64.efi) and MokManager (mmx64.efi).
if [[ -f \"$SHIM_SRC/shimx64.efi\" && -f \"$SHIM_SRC/mmx64.efi\" ]]; then
    install -m 0644 \"$SHIM_SRC/shimx64.efi\" \"$ESP/EFI/refind/shimx64.efi\"
    install -m 0644 \"$SHIM_SRC/mmx64.efi\"   \"$ESP/EFI/refind/mmx64.efi\"
    install -m 0644 \"$SHIM_SRC/shimx64.efi\" \"$ESP/EFI/BOOT/BOOTX64.EFI\"
    install -m 0644 \"$SHIM_SRC/mmx64.efi\"   \"$ESP/EFI/BOOT/mmx64.efi\"
fi

# 2. Public DER certificate at the root of the ESP and under rEFInd's keys/, so
#    MokManager's 'Enroll key from disk' finds it immediately without a password.
if [[ -f \"$CER\" ]]; then
    install -m 0644 \"$CER\" \"$ESP/noos-mok.cer\"
    install -m 0644 \"$CER\" \"$ESP/EFI/refind/keys/noos-mok.cer\"
fi

# 3. rEFInd, signed under both its own name and grubx64.efi (the second-stage
#    name compiled into shimx64.efi).
if [[ -f \"$REFIND_BIN\" ]]; then
    sign_efi \"$REFIND_BIN\" \"$ESP/EFI/refind/refind_x64.efi\"
    sign_efi \"$REFIND_BIN\" \"$ESP/EFI/refind/grubx64.efi\"
    sign_efi \"$REFIND_BIN\" \"$ESP/EFI/BOOT/grubx64.efi\"
fi

# 4. Every kernel image in /boot, skipped if already signed with our certificate.
shopt -s nullglob
for kernel in \"$ESP\"/vmlinuz-*; do
    [[ -f \"$kernel\" ]] || continue
    if /usr/bin/sbverify --cert \"$CRT\" \"$kernel\" >/dev/null 2>&1; then
        continue
    fi
    sign_efi \"$kernel\" \"$kernel\"
done

exit 0
";

/// Pacman alpm hook that keeps `/boot` signed across upgrades.
pub const PACMAN_HOOK: &str = "\
# Generated by the noos-os installer.
#
# Re-signs rEFInd and /boot/vmlinuz-* with the machine's MOK after any kernel,
# refind or shim-signed upgrade. Ordered at 95- so it runs after
# 90-mkinitcpio-install.hook has written the new kernel image to /boot.

[Trigger]
Operation = Install
Operation = Upgrade
Type = Path
Target = usr/lib/modules/*/vmlinuz

[Trigger]
Operation = Install
Operation = Upgrade
Type = Package
Target = refind
Target = shim-signed
Target = sbsigntools

[Action]
Description = Signature Secure Boot (MOK) du chargeur et du noyau...
When = PostTransaction
Exec = /usr/local/bin/noos-sb-sign
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hook_runs_after_mkinitcpio_writes_the_kernel() {
        // mkinitcpio's install hook is `90-mkinitcpio-install.hook`. Anything
        // lexicographically earlier signs the old kernel in /boot before
        // mkinitcpio overwrites it with the unsigned new one.
        let filename = PACMAN_HOOK_PATH
            .rsplit('/')
            .next()
            .expect("nom de fichier de hook");
        assert!(
            filename > "90-mkinitcpio-install.hook",
            "le hook ({filename}) tournerait avant 90-mkinitcpio-install.hook"
        );
        assert!(filename.ends_with(".hook"));
    }

    #[test]
    fn the_hook_calls_the_installed_script_by_absolute_path() {
        assert!(PACMAN_HOOK.contains(&format!("Exec = {SIGN_SCRIPT_PATH}")));
        assert!(PACMAN_HOOK.contains("When = PostTransaction"));
        assert!(PACMAN_HOOK.contains("Target = usr/lib/modules/*/vmlinuz"));
        assert!(PACMAN_HOOK.contains("Target = refind"));
        assert!(PACMAN_HOOK.contains("Target = shim-signed"));
    }

    #[test]
    fn the_script_signs_refind_as_grubx64_for_shim() {
        // shimx64.efi is compiled to load `grubx64.efi` from its own directory;
        // without that copy shim cannot chainload rEFInd.
        assert!(SIGN_SCRIPT.starts_with("#!/bin/bash"));
        assert!(SIGN_SCRIPT.contains("$ESP/EFI/refind/grubx64.efi"));
        assert!(SIGN_SCRIPT.contains("$ESP/EFI/BOOT/grubx64.efi"));
        assert!(SIGN_SCRIPT.contains("$ESP/EFI/refind/shimx64.efi"));
        assert!(SIGN_SCRIPT.contains("$ESP/EFI/BOOT/BOOTX64.EFI"));
        assert!(SIGN_SCRIPT.contains("$ESP/noos-mok.cer"));
    }

    #[test]
    fn the_script_uses_absolute_paths_to_sbsigntools() {
        assert!(SIGN_SCRIPT.contains("/usr/bin/sbsign"));
        assert!(SIGN_SCRIPT.contains("/usr/bin/sbverify"));
    }

    #[test]
    fn required_packages_cover_signing_and_shim() {
        assert!(REQUIRED_PACKAGES.contains(&"sbsigntools"));
        assert!(REQUIRED_PACKAGES.contains(&"openssl"));
        assert!(CHAOTIC_PACKAGES.contains(&"shim-signed"));
    }
}
