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

# The AUR packages, built here and carried on the medium.
#
# This is what keeps the installation from compiling them: two of the four
# build a GraalVM native image, and that build fails under makepkg inside the
# installer's chroot — see installer/core/src/aur.rs. Building them now, as a
# normal user on a real system, is the whole point.
#
# A failure is a warning and not an error: an ISO without them still installs a
# complete system, falling back to building from the AUR at install time. It is
# slower and needs the network, but it is not broken.
step "Paquets AUR préconstruits"
readonly AUR_STAGE="${ROOT}/.build/aur-packages"
if as_builder "'${ROOT}/scripts/build-aur-packages.sh' '${AUR_STAGE}'"; then
  echo "  $(ls -1 "${AUR_STAGE}"/*.pkg.tar.zst 2>/dev/null | wc -l) paquets prêts"
else
  echo "  avertissement : construction impossible — l'installation les compilera depuis l'AUR"
  echo "                 (plus lent, et il faut le réseau ; voir installer/core/src/aur.rs)"
fi

step "Assemblage du profil"
# The profile is copied rather than written to in place, so the compiled binary
# never lands in the source tree.
rm -rf "$ASSEMBLED"
mkdir -p "$ASSEMBLED"
cp -a "${PROFILE}/." "$ASSEMBLED/"
install -Dm755 "$INSTALLER_BIN" "${ASSEMBLED}/airootfs/usr/bin/noos-installer"
echo "  installateur -> airootfs/usr/bin/noos-installer"

# The prebuilt AUR packages, into the medium's own filesystem.
#
# The path is not free: it is `aur::DIR` in the installer, which copies from
# there into the target. The two have to agree, and nothing but this comment
# says so.
if compgen -G "${AUR_STAGE}/*.pkg.tar.zst" > /dev/null; then
  mkdir -p "${ASSEMBLED}/airootfs/var/cache/noos-aur"
  cp -a "${AUR_STAGE}"/*.pkg.tar.zst "${ASSEMBLED}/airootfs/var/cache/noos-aur/"
  echo "  $(ls -1 "${ASSEMBLED}/airootfs/var/cache/noos-aur" | wc -l) paquets AUR -> airootfs/var/cache/noos-aur"
fi

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
rm -f "$OUT_DIR"/*.iso

mkarchiso -v -w "$WORK_DIR" -o "$OUT_DIR" "$ASSEMBLED"

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

# Checking the built boot entry rather than profiledef.sh: this verifies the
# whole chain at once — that the variable was set, that mkarchiso substituted
# it, and that the substitution reached the file limine and systemd-boot
# actually read.
if xorriso -osirrox on -indev "$ISO" \
     -extract /loader/entries/01-noos-linux.conf "$VERIFY_DIR/entry.conf" 2>/dev/null \
   && grep -q 'splash' "$VERIFY_DIR/entry.conf"; then
  check "paramètre noyau splash" "oui"
else
  die "l'entrée de démarrage ne contient pas 'splash' : pas d'écran de démarrage"
fi

# The prebuilt AUR packages, when this build produced any.
#
# Checked on the finished squashfs rather than on the staging directory: the
# installer looks for them at one exact path, and packages that reached the
# profile but not the image would stay invisible until an installation tried to
# use them — the failure mode this whole project keeps meeting.
if [[ -d "${ASSEMBLED}/airootfs/var/cache/noos-aur" ]]; then
  STAGED="$(ls -1 "${ASSEMBLED}/airootfs/var/cache/noos-aur" 2>/dev/null | grep -c '\.pkg\.tar\.zst' || true)"
  IN_IMAGE="$(
    unsquashfs -l "$VERIFY_DIR/airootfs.sfs" var/cache/noos-aur 2>/dev/null \
      | grep -c '\.pkg\.tar\.zst' || true
  )"
  if [[ "$STAGED" -gt 0 && "$IN_IMAGE" -ge "$STAGED" ]]; then
    check "paquets AUR embarqués" "$IN_IMAGE"
  else
    die "$STAGED paquets préparés, $IN_IMAGE dans l'ISO : l'installateur ne les trouvera pas"
  fi
fi

cat <<EOF

  Tester en VM :

    ./scripts/run-vm.sh $ISO

  Si l'interface ne s'affiche pas dans la VM, bascule sur tty2
  (Ctrl+Alt+F2) et regarde :

    journalctl -b -u systemd-logind
    WLR_RENDERER=pixman cage -d -s /usr/bin/noos-installer

EOF
