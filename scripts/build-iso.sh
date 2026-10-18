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
  readonly BUILD_PATH="${BUILD_HOME}/.cargo/bin:${BUILD_HOME}/.cherrystudio/bin:/usr/local/bin:/usr/bin"
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
if pacman-key --list-keys "$CHAOTIC_KEY" >/dev/null 2>&1; then
  echo "  clé $CHAOTIC_KEY déjà présente"
else
  echo "  import de la clé $CHAOTIC_KEY"
  pacman-key --recv-key "$CHAOTIC_KEY" --keyserver keyserver.ubuntu.com
  # `--lsign-key` is the trust decision: it marks the key as locally signed, so
  # packages from Chaotic verify without a prompt nobody can answer during a
  # build.
  pacman-key --lsign-key "$CHAOTIC_KEY"
fi

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
  rm -rf "$WORK_DIR"
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

# rEFInd into the EFI image, in a second mkarchiso pass.
#
# This is a workaround for a limit of mkarchiso, and it is worth being precise
# about why it takes this shape.
#
# mkarchiso has no rEFInd bootmode, and it reads only `loader.conf` and
# `entries/*.conf` out of a profile — so there is no supported way to hand it an
# extra EFI *binary*. What it does do is copy `${WORK_DIR}/iso/EFI` wholesale
# into the FAT image it builds, and that directory is assembled with `install`,
# not purged, so files placed in it survive a rebuild.
#
# Hence: build once, drop rEFInd into the assembled tree, invalidate the markers
# that cover the boot image and the final packing, build again. The squashfs and
# the pacstrap are cached, so the second pass costs seconds.
#
# The markers are mkarchiso's `_run_once` files. Fewer than the ones removed
# here and the image keeps its old contents — which is the stale-ISO trap this
# script fights everywhere else, now used deliberately.
step "rEFInd dans l'image EFI"

readonly ISO_TREE="${WORK_DIR}/iso"
readonly REFIND_SRC="${WORK_DIR}/refind-src"

[[ -d $ISO_TREE ]] || die "mkarchiso n'a pas laissé d'arbre ISO dans ${WORK_DIR}"

# The rEFInd binaries come from the package on the build host, never from a copy
# in git: a committed `.efi` would be a copy of somebody else's binary that no
# review can check, and it would drift from the packaged version.
if ! pacman -Qq refind >/dev/null 2>&1; then
  echo "  installation de refind sur l'hôte"
  pacman -S --needed --noconfirm refind
fi

rm -rf "$REFIND_SRC"
install -d -m 0755 "${ISO_TREE}/EFI/refind" \
  "${ISO_TREE}/EFI/refind/drivers_x64" \
  "${ISO_TREE}/EFI/refind/themes/noos"

install -m 0644 /usr/share/refind/refind_x64.efi "${ISO_TREE}/EFI/refind/"

# The ISO9660 driver, so rEFInd can read the kernel off the medium whatever the
# firmware can manage on its own. rEFInd inherits the firmware's filesystems,
# and this makes the medium readable on a machine whose UEFI stops at FAT.
install -m 0644 /usr/share/refind/drivers_x64/iso9660_x64.efi \
  "${ISO_TREE}/EFI/refind/drivers_x64/"

# The theme, from the one committed copy.
install -m 0644 "${PROFILE}/airootfs/usr/share/noos/refind-theme/"*.png \
  "${ISO_TREE}/EFI/refind/themes/noos/"

# The live medium's configuration.
#
# No `archisosearchuuid`: mkarchiso substitutes that UUID into the systemd-boot
# entry, but it is generated during the build and a static file here cannot know
# it. `archisolabel` is equally good and known in advance — the archiso hook
# falls back to `/dev/disk/by-label/${archisolabel}`, which is why it is the one
# passed. See ISO_LABEL below.
# `declare -A` first, and it is not decoration: `file_permissions` in a profile
# is an *associative* array, written the way the releng profile writes it —
# `["/root"]="0:0:750"` — and relies on its consumer having declared it.
# mkarchiso does exactly this before sourcing. Reading the profile without it
# makes bash treat `["/root"]` as an arithmetic expression in an indexed array
# and abort with "/root : erreur de syntaxe arithmétique".
ISO_LABEL="$( ( declare -A file_permissions; source "${PROFILE}/profiledef.sh"; printf '%s' "$iso_label" ) )"
[[ -n $ISO_LABEL ]] || die "iso_label introuvable dans profiledef.sh"

cat > "${ISO_TREE}/EFI/refind/refind.conf" <<EOF
# Generated by scripts/build-iso.sh for the live medium.

timeout 10
scanfor internal,external,optical,netboot,cd,manual

include themes/noos/theme.conf

menuentry "noos-os live" {
    icon /EFI/refind/themes/noos/os_noos.png
    volume "$ISO_LABEL"
    loader /noos/boot/x86_64/vmlinuz-linux
    initrd /noos/boot/x86_64/initramfs-linux.img
    options archisobasedir=noos archisolabel=$ISO_LABEL quiet splash
}
EOF

echo "  $(basename "$ISO_LABEL") : noyau lu sur le volume ISO9660"

# Invalidate the markers covering the boot image and the packing. Everything
# else — pacstrap, the squashfs — is reused.
for marker in \
  "$WORK_DIR"/base._make_boot_on_fat \
  "$WORK_DIR"/base._make_boot_on_iso9660 \
  "$WORK_DIR"/base._make_bootmode_uefi.systemd-boot \
  "$WORK_DIR"/iso._build_iso_image \
  "$WORK_DIR"/build._build_buildmode_iso ; do
  [[ -e $marker ]] && rm -f "$marker"
done
rm -f "$WORK_DIR"/efiboot.img

echo "  seconde passe mkarchiso (squashfs en cache)"
rm -f "$OUT_DIR"/*.iso "$OUT_DIR"/*.iso.sha256
mkarchiso -v -w "$WORK_DIR" -o "$OUT_DIR" "$ASSEMBLED"

# The image was really rebuilt, not reused.
#
# Without this the second pass would silently keep the first image — which
# contains systemd-boot and no rEFInd — and the only symptom would be a live
# medium that boots into a menu nobody recognises.
EFIBOOT_IMG="${WORK_DIR}/efiboot.img"
[[ -e $EFIBOOT_IMG ]] || die "mkarchiso n'a pas reconstruit l'image EFI"
if ! mdir -i "$EFIBOOT_IMG" ::/EFI/refind 2>/dev/null | grep -q refind_x64.efi; then
  die "l'image EFI ne contient pas rEFInd : le média live ne chaînerait pas vers lui"
fi
echo "  rEFInd présent dans l'image EFI"

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
#
# Both files are read out of the ISO rather than out of the profile: mkarchiso
# copies `EFI/` and `loader/` onto the ISO9660 filesystem as well as into the
# EFI image, so what is read here is what a machine will actually be booted
# from — including that the substitutions below reached it.
#
# The kernel parameters moved. They used to sit on the systemd-boot entry; they
# are now in rEFInd's own configuration, because rEFInd is what loads the kernel.
if xorriso -osirrox on -indev "$ISO" \
     -extract /EFI/refind/refind.conf "$VERIFY_DIR/refind.conf" 2>/dev/null; then
  check "configuration rEFInd sur le média" "oui"
else
  die "l'ISO ne contient pas /EFI/refind/refind.conf : le live ne démarrerait pas"
fi

for expected in 'quiet splash' 'archisobasedir=noos' 'archisolabel=' 'menuentry "noos-os live"'; do
  grep -qF "$expected" "$VERIFY_DIR/refind.conf" \
    || die "la configuration rEFInd du live ne contient pas « $expected »"
done
check "paramètres du noyau live" "quiet splash + archiso"

# The hand-off from systemd-boot. Without it the live medium would show
# systemd-boot's own menu — a screen nobody recognises as noos-os.
if xorriso -osirrox on -indev "$ISO" \
     -extract /loader/entries/01-refind.conf "$VERIFY_DIR/entry.conf" 2>/dev/null \
   && grep -qE '^efi[[:space:]]+/EFI/refind/refind_x64\.efi' "$VERIFY_DIR/entry.conf"; then
  check "chaînage systemd-boot vers rEFInd" "oui"
else
  die "l'entrée systemd-boot ne chaîne pas vers rEFInd"
fi

# The timeout, which is what makes the hand-off invisible.
if xorriso -osirrox on -indev "$ISO" \
     -extract /loader/loader.conf "$VERIFY_DIR/loader.conf" 2>/dev/null \
   && grep -qE '^timeout +0$' "$VERIFY_DIR/loader.conf"; then
  check "menu systemd-boot masqué" "oui"
else
  die "loader.conf n'est pas en timeout 0 : le menu systemd-boot apparaîtrait"
fi

# The theme, on the medium the installer reads it from. `install_refind` copies
# it onto the target's ESP from /usr/share/noos/refind-theme.
for image in background.png os_noos.png selection_big.png selection_small.png; do
  unsquashfs -l "$VERIFY_DIR/airootfs.sfs" "usr/share/noos/refind-theme/$image" 2>/dev/null \
    | grep -q "$image" || die "le thème rEFInd n'a pas $image dans l'ISO"
done
check "thème rEFInd sur le média" "4 images"

cat <<EOF

  Tester en VM :

    ./scripts/run-vm.sh $ISO

  Si l'interface ne s'affiche pas dans la VM, bascule sur tty2
  (Ctrl+Alt+F2) et regarde :

    journalctl -b -u systemd-logind
    WLR_RENDERER=pixman cage -d -s /usr/bin/noos-installer

EOF
