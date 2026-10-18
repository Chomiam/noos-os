#!/usr/bin/env bash
#
# Builds the AUR packages the live medium carries.
#
#     ./scripts/build-aur-packages.sh <output-dir>
#
# Called by scripts/build-iso.sh. **Never run as root**: makepkg refuses to, and
# the whole reason this exists is that the build happens as a normal user on a
# real system rather than inside the installer's chroot.
#
# The package list is read out of `installer/core/src/aur.rs`. The installer and
# this script have to agree on exactly which packages ship; two lists in two
# files would drift, and the drift would show up as an ISO that installs
# everything except the one package somebody added.

set -euo pipefail

readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly AUR_RS="${ROOT}/installer/core/src/aur.rs"

die() { echo "erreur : $*" >&2; exit 1; }

[[ $# -eq 1 ]] || die "usage : $0 <répertoire de sortie>"

readonly OUT="$1"
readonly WORK="${OUT}/.build"

# makepkg refuses to run as root, and that refusal is the good kind — a build
# run as root leaves root-owned files in caches and breaks later builds.
#
# In CI the whole job runs as root inside a container, and the ISO published
# from there is the one people download: declining to build would ship an image
# whose installer compiles GraalVM native images on the user's machine, which is
# exactly what this is here to avoid. So root is allowed when `CI` says so, and
# a throwaway user is created to build as.
readonly BUILD_USER="noosbuild"
if [[ $EUID -eq 0 ]]; then
  [[ -n ${CI:-} ]] || die "lance ce script depuis ton compte, pas en root"
  id -u "$BUILD_USER" >/dev/null 2>&1 || useradd -m -s /bin/bash "$BUILD_USER"
fi

# Runs makepkg either directly or, in CI, as the throwaway user. `--nodeps`
# everywhere: the dependencies are installed separately, in one transaction,
# which is also what keeps makepkg from calling sudo on its own.
run_makepkg() {
  local dir="$1"
  if [[ $EUID -eq 0 ]]; then
    runuser -u "$BUILD_USER" -- bash -c "cd '$dir' && makepkg --nodeps --noconfirm --cleanbuild"
  else
    ( cd "$dir" && makepkg --nodeps --noconfirm --cleanbuild )
  fi
}

# Installs packages, as root or through sudo depending on how we got here.
install_packages() {
  if [[ $EUID -eq 0 ]]; then pacman -S --needed --noconfirm "$@"
  else sudo pacman -S --needed --noconfirm "$@"; fi
}

# The list, straight from the source of truth. The strings in that block are
# package names and nothing else, so a literal scrape is enough — and if the
# block ever gains a quoted comment, this finds it and the packages fail to
# build, loudly, rather than silently shipping one fewer.
mapfile -t PACKAGES < <(
  awk '/^pub const PACKAGES/{f=1} f{print} f&&/^\];/{exit}' "$AUR_RS" \
    | grep -oE '"[a-z0-9][a-z0-9.+-]*"' | tr -d '"'
)
(( ${#PACKAGES[@]} > 0 )) || die "aucun paquet trouvé dans ${AUR_RS}"

echo "  ${#PACKAGES[@]} paquets AUR à construire : ${PACKAGES[*]}"

mkdir -p "$OUT" "$WORK"

# ---------------------------------------------------------------- download ---
#
# Every PKGBUILD is fetched before anything is built, so the build dependencies
# of all of them can be installed in one transaction. `makepkg -s` would do it
# per package, by calling sudo from inside makepkg — which prompts, and hangs a
# script that is not attached to a terminal.
for package in "${PACKAGES[@]}"; do
  dir="${WORK}/${package}"
  if [[ -d $dir ]]; then
    echo "  ${package} : dépôt déjà présent"
    continue
  fi
  echo "  ${package} : clone"
  git clone --depth 1 "https://aur.archlinux.org/${package}.git" "$dir"
done

# ------------------------------------------------------------ dependencies ---
#
# `sed` and not `awk`: a PKGBUILD may write `makedepends=('a' 'b')` on one line
# or spread it over several, and only the range form reads both. An awk that
# waited for a line holding nothing but `)` would run off the end of the
# one-line form and hand back every quoted string in the file as a dependency.
#
# Version constraints (`zig>=0.16`, `pacman>6.1`) are stripped: they matter to
# makepkg's dependency check, not to the package name pacman is asked for.
collect() {
  local key="$1"
  for package in "${PACKAGES[@]}"; do
    sed -n "/^${key}=(/,/)/p" "${WORK}/${package}/PKGBUILD"
  done | grep -oE "'[^']+'" | tr -d "'" | sed 's/[<>=].*//' | sort -u
}

# `depends` too: a package built here links against the libraries its runtime
# dependencies bring, so they have to be present while it builds.
mapfile -t deps < <({ collect 'depends'; collect 'makedepends'; } | sort -u)
if (( ${#deps[@]} > 0 )); then
  echo "  dépendances de construction : ${deps[*]}"
  # One transaction for all of them, rather than one per package through
  # makepkg's own sudo.
  install_packages "${deps[@]}"
fi

# ------------------------------------------------------------------- build ---
if [[ $EUID -eq 0 ]]; then
  # The work directories were created by root; the throwaway user has to be
  # able to write in them.
  chown -R "${BUILD_USER}:${BUILD_USER}" "$WORK" "$OUT"
fi

for package in "${PACKAGES[@]}"; do
  dir="${WORK}/${package}"
  echo "  ${package} : makepkg"
  # --nocheck is deliberately absent: a failing check() is a real signal, and
  # these are the packages the distribution leans on hardest.
  run_makepkg "$dir"

  # Collect the result. `-debug` packages are left behind — hundreds of
  # megabytes of symbols nobody installs.
  found=0
  while IFS= read -r file; do
    cp "$file" "${OUT}/"
    found=1
  done < <(find "$dir" -maxdepth 1 -name '*.pkg.tar.zst' ! -name '*-debug-*')
  (( found )) || die "${package} : makepkg n'a produit aucun paquet"
done

echo "  paquets construits :"
ls -lh "${OUT}"/*.pkg.tar.zst | sed 's/^/    /'
