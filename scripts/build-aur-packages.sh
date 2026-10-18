#!/usr/bin/env bash
#
# Builds the AUR packages the live medium carries.
#
#     sudo ./scripts/build-aur-packages.sh <output-dir>
#
# Called by scripts/build-iso.sh, which is already root.
#
# **Runs as root and drops to a normal user for makepkg.** The other way round —
# running as the user and letting the script call sudo — asks for a password
# from inside `sudo -u`, where there is no terminal to type it into, and fails
# hard rather than prompting. It is the same split build-iso.sh makes for
# compiling the installer, and for the same reason: makepkg refuses to run as
# root, and a build run as root leaves root-owned files in caches that break
# every later build.
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

# Created before anything else can fail.
#
# Not cosmetic: the caller's only signal that this ran is whether the directory
# holds packages, and a script that dies before its first `mkdir` leaves exactly
# the same trace as one that never started.
mkdir -p "$OUT" "$WORK"

[[ $EUID -eq 0 ]] || die "lance ce script via sudo — build-iso.sh le fait pour toi"

# Who builds. The invoking user when there is one, so the caches it warms are
# theirs and survive to the next build; a throwaway account in a container,
# where there is no user to protect.
builder="${SUDO_USER:-}"
[[ -n $builder && $builder != root ]] || builder="noosbuild"
readonly BUILD_USER="$builder"
id -u "$BUILD_USER" >/dev/null 2>&1 || useradd -m -s /bin/bash "$BUILD_USER"
readonly BUILD_HOME="$(getent passwd "$BUILD_USER" | cut -d: -f6)"

echo "  construction en tant que ${BUILD_USER} (${BUILD_HOME})"

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

# ---------------------------------------------------------------- download ---
#
# Every PKGBUILD is fetched before anything is built, so the build dependencies
# of all of them are known at once and can be installed in one transaction.
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
collect_makedepends() {
  local package
  for package in "${PACKAGES[@]}"; do
    sed -n '/^makedepends=(/,/)/p' "${WORK}/${package}/PKGBUILD"
  done | grep -oE "'[^']+'" | tr -d "'" | sed 's/[<>=].*//' | sort -u
}

# `makedepends` only, not `depends`.
#
# What has to be present is what the *build* uses: compilers, headers, and the
# libraries a C or Zig package links against — and upstream lists those in
# `makedepends` precisely because they are needed before the package exists.
# `depends` is the *runtime* set, and installing it here would drag the target's
# packages onto the build host: `limine`, `snapper` and `btrfs-progs` to build a
# Java native image that links against none of them.
mapfile -t deps < <(collect_makedepends)
if (( ${#deps[@]} > 0 )); then
  echo "  dépendances de construction : ${deps[*]}"
  # Already root, so no sudo and nobody to ask for a password.
  #
  # No `-y`: syncing without upgrading is how a rolling release ends up with
  # libraries newer than the rest of the system. The database is whatever
  # build-iso.sh has been using, which is what pacstrap just resolved against.
  pacman -S --needed --noconfirm "${deps[@]}"
fi

# ------------------------------------------------------------------- build ---
#
# makepkg, as somebody other than root. `--nodeps` throughout: the dependencies
# were installed just above, and letting makepkg check them again would fail the
# build over a list it is not allowed to install itself.
build_one() {
  local dir="$1"
  # The clones were made by root; the builder has to be able to write in them,
  # and makepkg needs a home it can write caches into.
  chown -R "${BUILD_USER}:${BUILD_USER}" "$dir"
  runuser -u "$BUILD_USER" -- env HOME="$BUILD_HOME" \
    bash -c "cd '$dir' && makepkg --nodeps --noconfirm --cleanbuild"
}

for package in "${PACKAGES[@]}"; do
  dir="${WORK}/${package}"
  echo "  ${package} : makepkg"
  # --nocheck is deliberately absent: a failing check() is a real signal, and
  # these are the packages the distribution leans on hardest.
  build_one "$dir"

  # Collect the result. `-debug` packages are left behind: makepkg emits one
  # beside every build, and they are hundreds of megabytes of symbols nobody
  # installs.
  found=0
  while IFS= read -r file; do
    cp "$file" "${OUT}/"
    found=1
  done < <(find "$dir" -maxdepth 1 -name '*.pkg.tar.zst' ! -name '*-debug-*')
  (( found )) || die "${package} : makepkg n'a produit aucun paquet"
done

echo "  paquets construits :"
ls -lh "${OUT}"/*.pkg.tar.zst | sed 's/^/    /'
