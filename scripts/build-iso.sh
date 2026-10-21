#!/usr/bin/env bash
#
# Builds the noos-os live ISO.
#
#     sudo ./scripts/build-iso.sh
#
# Requires the archiso package on the host:
#     sudo pacman -S archiso
#
# Everything before the mkarchiso step runs unprivileged, as the invoking user:
# a Rust build run as root leaves root-owned files in the user's cargo and bun
# caches and breaks every later build.

set -euo pipefail

readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly PROFILE="${ROOT}/iso"
readonly ASSEMBLED="${ROOT}/.build/profile"
readonly WORK_DIR="${ROOT}/work"
readonly OUT_DIR="${ROOT}/out"
readonly INSTALLER_BIN="${ROOT}/installer/target/release/noos-installer"

die() { echo "erreur : $*" >&2; exit 1; }
step() { printf '\n== %s ==\n' "$*"; }
check() { printf '  %-32s %s\n' "$1" "$2"; }

[[ $EUID -eq 0 ]] || die "mkarchiso a besoin de root : sudo $0"

# Who compiles the installer.
#
# `mkarchiso` needs root, but cargo and bun should not run as root: they write
# to `~/.cargo` and `~/.bun`, and a root-owned file in there breaks every later
# build for that user. So the script drops privileges for the compile — the
# usual reason it is launched through `sudo` rather than as root directly.
#
# In a container there is no such user to protect: the job runs as root
# throughout and the caches are thrown away with the container. Detected by
# `CI` rather than by the absence of `SUDO_USER`, so that running the script
# as root on a real machine still gets the error instead of quietly polluting
# root's home.
readonly BUILD_USER="${SUDO_USER:-}"

if [[ -n $BUILD_USER && $BUILD_USER != root ]]; then
  readonly BUILD_AS="user"
  readonly BUILD_HOME="$(getent passwd "$BUILD_USER" | cut -d: -f6)"
  # cargo lives in ~/.cargo (rustup, no root), and Cherry Studio's managed
  # toolchain directory is where bun is.
  readonly BUILD_PATH="${BUILD_HOME}/.cargo/bin:${BUILD_HOME}/.bun/bin:${BUILD_HOME}/.cherrystudio/bin:/usr/local/bin:/usr/bin"
elif [[ -n ${CI:-} ]]; then
  readonly BUILD_AS="root"
  readonly BUILD_HOME="/root"
  readonly BUILD_PATH="/usr/local/bin:/usr/bin"
  echo "  conteneur CI détecté : compilation en root"
else
  die "lance le script via sudo depuis ton compte utilisateur, pas depuis un shell root"
fi

# Runs a command either as the invoking user or directly, depending on the
# above. Defined once so the compile step does not have to branch.
as_builder() {
  if [[ $BUILD_AS == "user" ]]; then
    sudo -u "$BUILD_USER" env HOME="$BUILD_HOME" PATH="$BUILD_PATH" bash -c "$1"
  else
    env HOME="$BUILD_HOME" PATH="$BUILD_PATH" bash -c "$1"
  fi
}

step "Outils de l'hôte"
for tool in mkarchiso mksquashfs xorriso mcopy; do
  command -v "$tool" >/dev/null 2>&1 \
    || die "$tool est absent — installe le paquet archiso :  sudo pacman -S archiso"
done
echo "  archiso $(pacman -Q archiso | awk '{print $2}')"

step "Compilation de l'installateur"
as_builder "
  set -euo pipefail
  command -v cargo >/dev/null 2>&1 || { echo 'cargo introuvable' >&2; exit 1; }
  command -v bun   >/dev/null 2>&1 || { echo 'bun introuvable' >&2; exit 1; }
  command -v xkbcli >/dev/null 2>&1 || { echo 'xkbcli introuvable — installe libxkbcommon' >&2; exit 1; }
  cd '${ROOT}'
  # The keyboard tables are derived from the system's own XKB data. They are
  # regenerated on every build rather than committed, so a layout added to
  # keyboard.rs can never end up with a stale or missing table.
  bun scripts/generate-keymaps.js
  bun scripts/generate-map-data.js
  bun scripts/generate-fastfetch-logo.js
  cd '${ROOT}/installer'
  bun install --frozen-lockfile
  # --no-bundle: only the binary is wanted, not a .deb/.rpm/AppImage.
  bun run tauri build --no-bundle
"

[[ -x "$INSTALLER_BIN" ]] || die "binaire introuvable après compilation : $INSTALLER_BIN"
echo "  $(du -h "$INSTALLER_BIN" | cut -f1)  $INSTALLER_BIN"

# The Flathub remote is committed, but its signing key rotates. Regenerating it
# against the published definition keeps the image current; a failure here is
# only a warning, because the committed copy is still a working one.
step "Définition du dépôt Flathub"
if "${ROOT}/scripts/generate-flatpak-remote.sh" >/dev/null 2>&1; then
  echo "  à jour"
else
  echo "  avertissement : Flathub injoignable, la copie versionnée est conservée"
fi

# The Chaotic-AUR repository, on the build host.
#
# Two things are needed before mkarchiso can put `chaotic-keyring` into the live
# environment, and both are idempotent:
#
#   - the host's pacman keyring has to trust Chaotic's signing key. Without it
#     the packages below fail signature verification, which surfaces as "invalid
#     or corrupted package" and names nothing about a key.
#   - the host needs `chaotic-mirrorlist`, because the `[chaotic-aur]` section
#     in iso/pacman.conf includes a path that mkarchiso resolves on the **host**,
#     not inside the profile.
#
# The key is fetched by its full identifier from a keyserver — that is the
# official bootstrap, and the only step that trusts anything on the strength of
# an identifier rather than a signature. Everything after it is verified against
# the key. See installer/core/src/chaotic.rs for what the repository is and what
# relying on it costs.
step "Dépôt Chaotic-AUR (hôte)"

readonly CHAOTIC_KEY="3056513887B78AEB"

# `pacman-key --list-keys` on the local keyring, not on the fetched one: what
# matters is whether the *host* has already accepted the key.
# Chaotic's key is fetched by its full identifier — the official bootstrap, and
# the only step that trusts anything on the strength of an identifier rather
# than a signature. Everything after it is verified against the key.
if pacman-key --list-keys "$CHAOTIC_KEY" >/dev/null 2>&1; then
  echo "  clé $CHAOTIC_KEY déjà présente"
else
  echo "  import de la clé $CHAOTIC_KEY"
  pacman-key --recv-key "$CHAOTIC_KEY" --keyserver keyserver.ubuntu.com
fi

# Locally signing needs a **secret** key, which `pacman-key --init` creates.
#
# Doing it the obvious way — test whether the keyring exists, initialise if not
# — does not work: an Arch container ships `/etc/pacman.d/gnupg` with public
# keys already imported and no secret key, so the directory is present and the
# signing still fails. Every CI run stopped here:
#
#   ERROR: There is no secret key available to sign with.
#
# So the attempt comes first and the initialisation only on failure, which is
# what the error message itself advises. A second failure aborts under `set -e`
# rather than being swallowed.
if ! pacman-key --lsign-key "$CHAOTIC_KEY" 2>/dev/null; then
  echo "  trousseau sans clé secrète : pacman-key --init"
  pacman-key --init
  pacman-key --lsign-key "$CHAOTIC_KEY"
fi
echo "  clé $CHAOTIC_KEY signée localement"

if pacman -Qq chaotic-mirrorlist >/dev/null 2>&1; then
  echo "  chaotic-mirrorlist déjà installé"
else
  echo "  installation de chaotic-keyring et chaotic-mirrorlist"
  # Fetched by URL rather than by repository name: the repository cannot be used
  # until these two are installed, which is the chicken-and-egg the official
  # instructions work around exactly this way.
  pacman -U --noconfirm --needed \
    'https://cdn-mirror.chaotic.cx/chaotic-aur/chaotic-keyring.pkg.tar.zst' \
    'https://cdn-mirror.chaotic.cx/chaotic-aur/chaotic-mirrorlist.pkg.tar.zst'
fi

step "Assemblage du profil"
# The profile is copied rather than written to in place, so the compiled binary
# never lands in the source tree.
rm -rf "$ASSEMBLED"
mkdir -p "$ASSEMBLED"
cp -a "${PROFILE}/." "$ASSEMBLED/"
install -Dm755 "$INSTALLER_BIN" "${ASSEMBLED}/airootfs/usr/bin/noos-installer"
echo "  installateur -> airootfs/usr/bin/noos-installer"

# Copy the WhiteSur GRUB2 theme into the profile's `grub/themes/whitesur` tree
# so `mkarchiso`'s `uefi.grub` bootmode embeds it into `/boot/grub/themes/whitesur`
# on both the ISO9660 filesystem and the FAT `efiboot.img`.
install -d -m 0755 "${ASSEMBLED}/grub/themes/whitesur"
cp -a "${PROFILE}/airootfs/usr/share/grub/themes/whitesur/." "${ASSEMBLED}/grub/themes/whitesur/"
echo "  thème GRUB WhiteSur -> grub/themes/whitesur"

step "Construction de l'ISO"
mkdir -p "$OUT_DIR"

# mkarchiso caches every build step in the work directory, using empty marker
# files named after the function that ran — `base._make_custom_airootfs` and
# friends. On a second run it sees those markers and SKIPS the steps, so the
# freshly built installer never gets copied into the image and the ISO is
# assembled from stale contents. The failure is completely silent: the build
# reports success and produces a valid ISO that simply does not contain the
# changes.
#
# `mkarchiso -r` deletes the work directory at the *end*, which is not enough:
# an interrupted build leaves it behind and the next run is stale again.
# Removing it up front costs a few minutes of pacstrap and is the only way to
# be sure the ISO matches the source.
if [[ -d $WORK_DIR ]]; then
  echo "  suppression du cache de build (work/) pour garantir un résultat à jour"
  for mnt in "$WORK_DIR/x86_64/airootfs/dev/pts" \
             "$WORK_DIR/x86_64/airootfs/dev/shm" \
             "$WORK_DIR/x86_64/airootfs/dev" \
             "$WORK_DIR/x86_64/airootfs/proc" \
             "$WORK_DIR/x86_64/airootfs/sys" \
             "$WORK_DIR/x86_64/airootfs/run" \
             "$WORK_DIR/x86_64/airootfs"; do
    umount -Rl "$mnt" 2>/dev/null || true
  done
  if findmnt -R -n "$WORK_DIR" >/dev/null 2>&1; then
    die "des points de montage système sont encore actifs sous $WORK_DIR — refus de supprimer pour protéger /dev de l'hôte"
  fi
  rm -rf --one-file-system "$WORK_DIR"
fi

# Removing the previous ISO makes the check below mean something: without it,
# a build that produced nothing would still find the old file and report it as
# the result.
#
# The checksum goes too. Left behind, it would sit beside a *different* image —
# a failed build writes no new one — and `sha256sum -c` would then fail on an
# ISO that is in fact fine, or worse, pass on one that is not.
rm -f "$OUT_DIR"/*.iso "$OUT_DIR"/*.iso.sha256

mkarchiso -v -w "$WORK_DIR" -o "$OUT_DIR" "$ASSEMBLED"

step "GRUB et Secure Boot dans l'image EFI"

readonly ISO_TREE="${WORK_DIR}/iso"
readonly PACSTRAP_ROOT="${WORK_DIR}/x86_64/airootfs"
readonly BOOT_STAGE_DIR="${WORK_DIR}/x86_64/boot"
readonly SHIM_DIR="${PACSTRAP_ROOT}/usr/share/shim-signed"
readonly SB_DIR="${WORK_DIR}/sb-keys"
readonly EFIBOOT_IMG="${WORK_DIR}/efiboot.img"

[[ -d $ISO_TREE ]] || die "mkarchiso n'a pas laissé d'arbre ISO dans ${WORK_DIR}"
[[ -e $EFIBOOT_IMG ]] || die "mkarchiso n'a pas généré ${EFIBOOT_IMG}"

for host_pkg in grub sbsigntools openssl mtools; do
  if ! pacman -Qq "$host_pkg" >/dev/null 2>&1; then
    echo "  installation de $host_pkg sur l'hôte"
    pacman -S --needed --noconfirm "$host_pkg"
  fi
done

[[ -f "${SHIM_DIR}/shimx64.efi" && -f "${SHIM_DIR}/mmx64.efi" ]] \
  || die "shim-signed introuvable dans ${SHIM_DIR} — vérifie iso/packages.x86_64"

# Ephemeral Machine Owner Key (MOK) for this ISO build.
#
# The private key stays inside ${WORK_DIR}/sb-keys and never enters the ISO;
# only the public DER certificate (`noos-mok.cer`) is shipped on the EFI image
# and at the ISO root so MokManager (`mmx64.efi`) can enroll it via "Enroll key
# from disk" on a machine with Secure Boot enabled.
rm -rf "$SB_DIR"
install -d -m 0700 "$SB_DIR"
openssl req -new -x509 -newkey rsa:2048 -nodes -days 3650 \
  -subj "/CN=noos-os Live Secure Boot MOK/" \
  -keyout "${SB_DIR}/mok.key" \
  -out "${SB_DIR}/mok.crt" 2>/dev/null
openssl x509 -in "${SB_DIR}/mok.crt" -outform DER -out "${SB_DIR}/noos-mok.cer"

sb_sign_file() {
  local src="$1"
  local dst="$2"
  local tmp="${dst}.tmp"
  sbsign --key "${SB_DIR}/mok.key" --cert "${SB_DIR}/mok.crt" \
    --output "$tmp" "$src" 2>&1 \
    | grep -v "gaps between PE/COFF sections" || true
  [[ -s "$tmp" ]] || die "échec de la signature Secure Boot de $src"
  mv -f "$tmp" "$dst"
  sbverify --cert "${SB_DIR}/mok.crt" "$dst" >/dev/null 2>&1 \
    || die "vérification sbverify échouée sur $dst"
}

# Stage the Secure Boot hand-off in /EFI/BOOT:
#
#   BOOTx64.EFI (shimx64.efi, Microsoft-signed)
#     -> grubx64.efi (standalone GRUB with WhiteSur theme & sbat.csv, MOK-signed)
#       -> /noos/boot/x86_64/vmlinuz-linux (MOK-signed)
readonly GRUB_STANDALONE_SRC="${ISO_TREE}/EFI/BOOT/BOOTx64.EFI"
[[ -f "$GRUB_STANDALONE_SRC" ]] || die "BOOTx64.EFI (GRUB standalone) introuvable dans ${ISO_TREE}/EFI/BOOT"
sb_sign_file "$GRUB_STANDALONE_SRC" "${ISO_TREE}/EFI/BOOT/grubx64.efi"
install -m 0644 "${SHIM_DIR}/mmx64.efi" "${ISO_TREE}/EFI/BOOT/mmx64.efi"
install -m 0644 "${SHIM_DIR}/shimx64.efi" "${ISO_TREE}/EFI/BOOT/BOOTx64.EFI"

# Sign the UEFI shell if present.
if [[ -f "${ISO_TREE}/shellx64.efi" ]]; then
  sb_sign_file "${ISO_TREE}/shellx64.efi" "${ISO_TREE}/shellx64.efi"
fi

# Sign the live kernel in both the staging directory and the ISO tree.
for kernel in "${BOOT_STAGE_DIR}"/vmlinuz-* "${ISO_TREE}/noos/boot/x86_64"/vmlinuz-*; do
  [[ -f "$kernel" ]] || continue
  sb_sign_file "$kernel" "$kernel"
done

# Public MOK certificate, placed where MokManager's "Enroll key from disk"
# finds it immediately without a password prompt.
install -m 0644 "${SB_DIR}/noos-mok.cer" "${ISO_TREE}/noos-mok.cer"
install -m 0644 "${SB_DIR}/noos-mok.cer" "${ISO_TREE}/EFI/noos-mok.cer"
install -m 0644 "${SB_DIR}/noos-mok.cer" "${ISO_TREE}/EFI/BOOT/noos-mok.cer"

# Rebuild `efiboot.img` (32 MiB FAT) and inject the signed GRUB binary,
# Microsoft-signed shim, MokManager, MOK certificate, GRUB config + WhiteSur
# theme, and signed UEFI shell.
rm -f "$EFIBOOT_IMG"
mkfs.fat -C -n ARCHISO_EFI "$EFIBOOT_IMG" 32768 >/dev/null
mcopy -s -i "$EFIBOOT_IMG" "${ISO_TREE}/EFI" "${ISO_TREE}/boot" "::/"
mcopy -o -i "$EFIBOOT_IMG" "${ISO_TREE}/noos-mok.cer" "::/noos-mok.cer"
if [[ -f "${ISO_TREE}/shellx64.efi" ]]; then
  mcopy -o -i "$EFIBOOT_IMG" "${ISO_TREE}/shellx64.efi" "::/shellx64.efi"
fi

# Invalidate only the final ISO image packing markers so xorriso repacks the
# updated ISO_TREE and EFIBOOT_IMG in seconds.
for marker in \
  "$WORK_DIR"/iso._build_iso_image \
  "$WORK_DIR"/build._build_buildmode_iso ; do
  [[ -e $marker ]] && rm -f "$marker"
done

echo "  seconde passe mkarchiso (repaquetage ISO avec GRUB signé + Secure Boot)"
rm -f "$OUT_DIR"/*.iso "$OUT_DIR"/*.iso.sha256
mkarchiso -v -w "$WORK_DIR" -o "$OUT_DIR" "$ASSEMBLED"

readonly EFI_CHECK_DIR="${WORK_DIR}/efi-check"
mkdir -p "$EFI_CHECK_DIR"

readonly EFI_FILES=(
  /EFI/BOOT/BOOTx64.EFI
  /EFI/BOOT/grubx64.efi
  /EFI/BOOT/mmx64.efi
  /EFI/BOOT/noos-mok.cer
  /EFI/noos-mok.cer
  /boot/grub/grub.cfg
  /boot/grub/themes/whitesur/theme.txt
  /boot/grub/themes/whitesur/background.jpg
  /boot/grub/themes/whitesur/icons/noos-os.png
  /boot/grub/themes/whitesur/icons/noos_os.png
)

for file in "${EFI_FILES[@]}"; do
  target="${EFI_CHECK_DIR}/$(basename "$file")"
  rm -f "$target"
  mcopy -o -i "$EFIBOOT_IMG" "::${file}" "$target" </dev/null 2>/dev/null \
    || die "l'image EFI n'a pas ${file} : le média live ne s'amorcerait pas comme prévu"
  if [[ $(stat -c%s "$target") -eq 0 ]]; then
    die "${file} est vide dans l'image EFI"
  fi
done
echo "  média live dans l'image EFI (GRUB, thème WhiteSur et Secure Boot)"

step "Terminé"
readonly ISO="$(ls "$OUT_DIR"/*.iso 2>/dev/null | head -1)"
[[ -n $ISO ]] || die "mkarchiso n'a produit aucune ISO dans $OUT_DIR"
ls -lh "$ISO" | sed 's/^/  /'

# A checksum beside the image. An ISO is written to removable media on machines
# with no network, so this is the only way to tell a bad burn from a bad image.
( cd "$OUT_DIR" && sha256sum "$(basename "$ISO")" > "$(basename "$ISO").sha256" )
echo "  $(cat "${ISO}.sha256" | cut -c1-16)…  $(basename "$ISO").sha256"

step "Vérification de l'ISO"
# Everything checked below has already gone wrong once, silently. mkarchiso's
# failures are quiet: it reports success and produces a valid ISO that simply
# does not do what the source says. Inspecting the built artifact is the only
# way to catch that before booting a VM and staring at the wrong screen.

readonly VERIFY_DIR="${ROOT}/.verify"
# Deliberately not `mktemp -d`: the default is /tmp, which on this system is a
# tmpfs — so extracting a 1.1 GiB squashfs there would spend a gigabyte of RAM
# and can fill it outright. The verification scratch belongs on disk.
rm -rf "$VERIFY_DIR"
mkdir -p "$VERIFY_DIR"
trap 'rm -rf "$VERIFY_DIR"' EXIT

xorriso -osirrox on -indev "$ISO" \
  -extract "/noos/x86_64/airootfs.sfs" "$VERIFY_DIR/airootfs.sfs" 2>/dev/null \
  || die "airootfs.sfs introuvable dans l'ISO"

# Dangling symlinks, checked against the extracted root filesystem.
#
# A broken symlink is the quietest failure there is: the file is present, the
# build succeeds, and the unit or rule it was meant to enable simply never
# runs. `pacman-init.service` was shipped that way — the symlink existed, the
# unit it pointed at did not, because that unit comes from the archiso profile
# rather than from a package — and the live medium spent the whole session
# with an empty keyring. pacstrap reported it three steps later as "keyring is
# not writable", which points nowhere near the cause.
#
# This has to run against the squashfs, not the profile: the targets of most
# of these links are package files, which only exist once pacstrap has run.
readonly SYMLINK_ROOT="$VERIFY_DIR/root"
mkdir -p "$SYMLINK_ROOT"
unsquashfs -d "$SYMLINK_ROOT" -no-progress "$VERIFY_DIR/airootfs.sfs" >/dev/null 2>&1

DANGLING=()
while IFS= read -r link; do
  target="$(readlink "$link")"

  # Two paths are needed and they must not be confused:
  #
  #   rooted    where the target lives inside the extracted tree, to test
  #             whether it exists at all
  #   relative  the same location expressed from the tree's root, to recognise
  #             the runtime paths
  #
  # An absolute target is rooted directly under $SYMLINK_ROOT; a relative one
  # is resolved against the directory holding the link. Deriving one from the
  # other by string surgery is how this check first went wrong — it prefixed
  # the root twice and reported every symlink in the tree as broken.
  case "$target" in
    /*)
      rooted="${SYMLINK_ROOT}${target}"
      relative="${target}"
      ;;
    *)
      rooted="$(dirname "$link")/${target}"
      relative="$(realpath -m --relative-to="$SYMLINK_ROOT" "$rooted" 2>/dev/null || echo "$rooted")"
      ;;
  esac

  # Runtime paths are created when the system boots, and a device node has no
  # reason to be in a squashfs.
  case "/${relative#/}" in
    /dev/*|/proc/*|/sys/*|/run/*|/tmp/*) continue ;;
  esac

  [[ -e "$rooted" ]] || DANGLING+=("${link#"${SYMLINK_ROOT}/"} -> ${target}")
done < <(find "$SYMLINK_ROOT" -type l)

if (( ${#DANGLING[@]} > 0 )); then
  printf '  lien(s) symbolique(s) cassé(s) :\n'
  printf '    %s\n' "${DANGLING[@]}"
  die "un lien cassé ne casse rien mais ne fait rien non plus"
fi
check "liens symboliques" "tous résolus"

# mkarchiso copies airootfs without preserving modes, so an executable shipped
# as 0755 lands as 0644 and cage dies with "Failed to spawn client: Permission
# denied". Declaring it in file_permissions is what fixes it — this check is
# what proves the declaration took effect.
readonly BINARY_MODE="$(
  unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" usr/bin/noos-installer 2>/dev/null \
    | awk '/noos-installer/ {print $1}'
)"

case "$BINARY_MODE" in
  -rwx*) check "installateur exécutable" "oui ($BINARY_MODE)" ;;
  "")    die "l'installateur est ABSENT de l'ISO" ;;
  *)     die "l'installateur n'est pas exécutable dans l'ISO ($BINARY_MODE) — déclare-le dans file_permissions (profiledef.sh)" ;;
esac

# systemd-firstboot seizes the console when /etc/machine-id is
# "uninitialized", which is exactly what mkarchiso writes.
if unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" etc/systemd/system/systemd-firstboot.service 2>/dev/null \
     | grep -q '^l'; then
  check "systemd-firstboot masqué" "oui"
else
  die "systemd-firstboot n'est pas masqué : il prendra la console au démarrage"
fi

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" usr/bin/cage 2>/dev/null | grep -q cage; then
  check "compositeur cage présent" "oui"
else
  die "cage est absent : aucun compositeur pour l'installateur"
fi

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" root/.bash_profile 2>/dev/null | grep -q bash_profile; then
  check "démarrage automatique tty1" "oui"
else
  die "root/.bash_profile est absent : l'installateur ne sera pas lancé"
fi

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" usr/bin/fish 2>/dev/null | grep -q fish \
   && unsquashfs -cat "$VERIFY_DIR/airootfs.sfs" etc/fish/conf.d/noos.fish 2>/dev/null | grep -q 'set -g fish_greeting'; then
  check "shell fish (sans message d'accueil)" "oui"
else
  die "fish ou /etc/fish/conf.d/noos.fish est absent de l'ISO"
fi

if unsquashfs -cat "$VERIFY_DIR/airootfs.sfs" etc/pacman.conf 2>/dev/null | grep -q '^\[multilib\]$'; then
  check "dépôt multilib activé (live)" "oui"
else
  die "le dépôt [multilib] n'est pas activé dans /etc/pacman.conf du live"
fi

# Networking. Two separate things can be missing and both look like "no
# internet": the interface is never configured, or it is configured but names
# do not resolve. The second is the easier one to overlook.
if unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" etc/resolv.conf 2>/dev/null | grep -q '^l'; then
  check "resolv.conf (résolution DNS)" "oui"
else
  die "etc/resolv.conf manque : l'invité aura une adresse mais ne résoudra aucun nom"
fi

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" \
     etc/systemd/system/multi-user.target.wants/systemd-networkd.service 2>/dev/null | grep -q networkd; then
  check "systemd-networkd activé" "oui"
else
  die "systemd-networkd n'est pas activé : aucune interface ne sera configurée"
fi

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" \
     etc/systemd/system/multi-user.target.wants/qemu-guest-agent.service 2>/dev/null | grep -q guest-agent; then
  check "qemu-guest-agent activé" "oui"
else
  # Not fatal: it only affects testing, not the installed system.
  echo "  avertissement : qemu-guest-agent n'est pas activé, la VM de test ne sera pas interrogeable"
fi

# Network power management. The script is called by path from both a udev rule
# and a unit, so a missing executable bit would make every invocation fail
# silently — the same trap that broke cage.
POWERSAVE_MODE="$(
  unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" usr/local/bin/noos-eee-off 2>/dev/null \
    | awk '/noos-eee-off/ {print $1}'
)"
case "$POWERSAVE_MODE" in
  -rwx*) check "script anti-économie d'énergie" "exécutable" ;;
  "")    die "usr/local/bin/noos-eee-off est absent de l'ISO" ;;
  *)     die "noos-eee-off n'est pas exécutable dans l'ISO ($POWERSAVE_MODE) — déclare-le dans file_permissions" ;;
esac

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" usr/bin/ethtool 2>/dev/null | grep -q ethtool; then
  check "ethtool présent" "oui"
else
  die "ethtool est absent : le script anti-EEE échouerait sur chaque interface"
fi

# The pacman keyring. Its absence is not visible until pacstrap runs, and then
# it aborts the installation with "keyring is not writable" and "required key
# missing" — errors that say nothing about the missing unit. Three things have
# to be in place: the tmpfs the keyring lives on, the unit that initialises it,
# and a working clock for gpg.
if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" \
     etc/systemd/system/etc-pacman.d-gnupg.mount 2>/dev/null | grep -q gnupg.mount; then
  check "tmpfs du trousseau pacman" "oui"
else
  die "etc-pacman.d-gnupg.mount manque : pacstrap échouera sur le trousseau"
fi

if unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" \
     etc/systemd/system/multi-user.target.wants/pacman-init.service 2>/dev/null | grep -q '^l'; then
  check "pacman-init.service activé" "oui"
else
  die "pacman-init.service n'est pas activé : l'ISO n'aura aucun trousseau"
fi

if unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" \
     etc/systemd/sysinit.target.wants/systemd-timesyncd.service 2>/dev/null | grep -q '^l'; then
  check "synchronisation de l'horloge" "oui"
else
  # Not fatal on its own, but gpg refuses to validate signatures against a
  # clock that is wrong, and a live medium often boots on a stale one.
  echo "  avertissement : systemd-timesyncd n'est pas activé, le trousseau peut refuser de se peupler"
fi

# systemd would otherwise mount the target's partitions by their GPT type as
# soon as the installer creates them.
if unsquashfs -ll "$VERIFY_DIR/airootfs.sfs" \
     etc/systemd/system-generators/systemd-gpt-auto-generator 2>/dev/null | grep -q '^l'; then
  check "gpt-auto-generator désactivé" "oui"
else
  echo "  avertissement : systemd-gpt-auto-generator est actif, systemd peut monter la cible en cours d'installation"
fi

# The splash needs three things to agree: the package, the initramfs hook and
# the `splash` kernel parameter. Missing any one of them produces a silent
# fallback to a plain text boot, which is easy to mistake for "it works".
if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" usr/share/plymouth/themes/noos/noos.script 2>/dev/null \
     | grep -q noos.script; then
  check "thème Plymouth installé" "oui"
else
  die "le thème Plymouth est absent de l'ISO"
fi

# The animation is baked into frames, so a missing sequence means a static
# splash rather than a broken boot — worth catching, easy to miss by eye.
FRAME_COUNT="$(
  unsquashfs -l "$VERIFY_DIR/airootfs.sfs" 2>/dev/null \
    | grep -c 'themes/noos/frame-.*\.png'
)"
if [[ "$FRAME_COUNT" -ge 12 ]]; then
  check "images d'animation Plymouth" "$FRAME_COUNT"
else
  die "seulement $FRAME_COUNT images d'animation : lance scripts/generate-plymouth-frames.js"
fi

if unsquashfs -cat "$VERIFY_DIR/airootfs.sfs" etc/mkinitcpio.conf.d/archiso.conf 2>/dev/null \
     | grep -q 'plymouth'; then
  check "hook plymouth dans l'initramfs" "oui"
else
  die "le hook plymouth manque : le splash ne démarrera pas"
fi

# The live boot chain, checked on the built artifact.
if xorriso -osirrox on -indev "$ISO" \
     -extract /boot/grub/grub.cfg "$VERIFY_DIR/grub.cfg" 2>/dev/null; then
  check "configuration GRUB sur le média" "oui"
else
  die "l'ISO ne contient pas /boot/grub/grub.cfg : le live ne démarrerait pas"
fi

for expected in 'quiet splash' 'archisobasedir=noos' 'themes/whitesur/theme.txt' 'menuentry "noos-os live"'; do
  grep -qF "$expected" "$VERIFY_DIR/grub.cfg" \
    || die "la configuration GRUB du live ne contient pas « $expected »"
done
check "paramètres du noyau live" "quiet splash + archiso + WhiteSur"

# The WhiteSur GRUB2 theme, both on the ISO boot tree and inside airootfs for
# `install_grub` to copy onto the target.
for asset in theme.txt background.jpg icons/noos-os.png icons/noos_os.png icons/snapshots.png; do
  unsquashfs -l "$VERIFY_DIR/airootfs.sfs" "usr/share/grub/themes/whitesur/$asset" 2>/dev/null \
    | grep -q "$(basename "$asset")" || die "le thème GRUB WhiteSur n'a pas $asset dans l'ISO"
done
check "thème GRUB WhiteSur sur le média" "présent (live + cible)"

# The KDE WhiteSur Liquid archive, extracted onto the target when the user
# selects KDE Plasma.
if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" "usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst" 2>/dev/null \
     | grep -q "whitesur-liquid.tar.zst" \
   && tar --zstd -tf "$SYMLINK_ROOT/usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst" \
        usr/share/wallpapers/noos/contents/images/2752x1536.jpg >/dev/null 2>&1 \
   && tar --zstd -xOf "$SYMLINK_ROOT/usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst" \
        usr/share/plasma/look-and-feel/com.github.vinceliuice.WhiteSur-dark/contents/layouts/org.kde.plasma.desktop-layout.js 2>/dev/null \
      | grep -q 'org.kde.plasma.kicker' \
   && tar --zstd -xOf "$SYMLINK_ROOT/usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst" \
        usr/share/plasma/look-and-feel/com.github.vinceliuice.WhiteSur-dark/contents/layouts/org.kde.plasma.desktop-layout.js 2>/dev/null \
      | grep -q 'com.mitchellh.ghostty.desktop' \
   && tar --zstd -xOf "$SYMLINK_ROOT/usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst" \
        usr/share/plasma/look-and-feel/com.github.vinceliuice.WhiteSur-dark/contents/layouts/org.kde.plasma.desktop-layout.js 2>/dev/null \
      | grep -q 'noos-package-manager.desktop' \
   && tar --zstd -xOf "$SYMLINK_ROOT/usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst" \
        usr/share/plasma/look-and-feel/com.github.vinceliuice.WhiteSur-dark/contents/layouts/org.kde.plasma.desktop-layout.js 2>/dev/null \
      | grep -q '/usr/share/wallpapers/noos/contents/images/2752x1536.jpg'; then
  check "thème KDE WhiteSur Liquid (Menu des applications + Ghostty + noos-package-manager + fond d'écran noos-os)" "présent"
else
  die "whitesur-liquid.tar.zst est absent ou n'utilise pas org.kde.plasma.kicker / Ghostty / noos-package-manager / fond d'écran noos-os"
fi

# Fastfetch profile (sample 13) and 45°-tilted infinity logo.
if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" "etc/fastfetch/config.jsonc" 2>/dev/null \
     | grep -q "config.jsonc" \
   && unsquashfs -l "$VERIFY_DIR/airootfs.sfs" "usr/share/noos/fastfetch/noos-infinity.png" 2>/dev/null \
     | grep -q "noos-infinity.png"; then
  check "profil Fastfetch et logo infini 45°" "présents"
else
  die "le profil Fastfetch ou noos-infinity.png manque dans l'ISO"
fi

# Secure Boot verification on the built artifact:
#   1. BOOTx64.EFI carries Microsoft's UEFI CA signature (shim-signed), and
#      mmx64.efi carries the vendor signature embedded in shimx64.efi's
#      .vendor_cert section (Fedora / Red Hat Secure Boot CA in shim-signed)
#   2. grubx64.efi (standalone GRUB) and the live kernel vmlinuz-linux pass
#      `sbverify` against the MOK certificate
#   3. The DER certificate `noos-mok.cer` is reachable at the ISO root and on
#      the EFI image for MokManager's "Enroll key from disk"
#   4. The live squashfs carries `shim-signed` and `sbsign` for the target
if sbverify --list "${EFI_CHECK_DIR}/BOOTx64.EFI" 2>/dev/null | grep -qi "Microsoft"; then
  check "shim signé par Microsoft (BOOTx64.EFI)" "oui"
else
  die "BOOTx64.EFI ne porte pas la signature UEFI Microsoft (shim-signed)"
fi

if sbverify --list "${EFI_CHECK_DIR}/mmx64.efi" 2>/dev/null | grep -Eqi "Fedora|Red Hat|Microsoft"; then
  check "MokManager signé (mmx64.efi)" "oui"
else
  die "mmx64.efi ne porte pas la signature vendeur attendue par shimx64.efi"
fi

sbverify --cert "${SB_DIR}/mok.crt" "${EFI_CHECK_DIR}/grubx64.efi" >/dev/null 2>&1 \
  || die "grubx64.efi dans l'image EFI n'est pas signé avec la clé MOK du build"
check "binaire EFI signé MOK" "grubx64.efi (GRUB standalone)"

xorriso -osirrox on -indev "$ISO" \
  -extract "/noos/boot/x86_64/vmlinuz-linux" "$VERIFY_DIR/vmlinuz-linux" 2>/dev/null \
  || die "vmlinuz-linux introuvable dans l'ISO"
if sbverify --cert "${SB_DIR}/mok.crt" "$VERIFY_DIR/vmlinuz-linux" >/dev/null 2>&1; then
  check "noyau live signé MOK (vmlinuz-linux)" "oui"
else
  die "le noyau live vmlinuz-linux n'est pas signé pour Secure Boot"
fi

xorriso -osirrox on -indev "$ISO" \
  -extract "/noos-mok.cer" "$VERIFY_DIR/noos-mok.cer" 2>/dev/null \
  && [[ -s "$VERIFY_DIR/noos-mok.cer" ]] \
  || die "certificat MOK /noos-mok.cer absent de l'ISO"
check "certificat MOK sur le média" "/noos-mok.cer + EFI"

if unsquashfs -l "$VERIFY_DIR/airootfs.sfs" usr/share/shim-signed/shimx64.efi 2>/dev/null | grep -q shimx64.efi \
   && unsquashfs -l "$VERIFY_DIR/airootfs.sfs" usr/bin/sbsign 2>/dev/null | grep -q sbsign; then
  check "outils Secure Boot pour la cible" "shim-signed + sbsigntools"
else
  die "shim-signed ou sbsigntools manque dans le squashfs live"
fi

cat <<EOF

  Tester en VM :

    ./scripts/run-vm.sh $ISO

  Si l'interface ne s'affiche pas dans la VM, bascule sur tty2
  (Ctrl+Alt+F2) et regarde :

    journalctl -b -u systemd-logind
    WLR_RENDERER=pixman cage -d -s /usr/bin/noos-installer

EOF
