#!/usr/bin/env bash
# shellcheck disable=SC2034
#
# archiso profile for noos-os.
#
# Sourced by mkarchiso. Every variable here is read by the build script, so the
# shellcheck disable above is deliberate.

iso_name="noos-os"
iso_label="NOOS_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="noos-os"
iso_application="noos-os live/install medium"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="noos"

# UEFI only.
#
# The *installed* system boots with limine on both firmware types — that is the
# installer's job. The live medium is UEFI-only because every machine this
# targets is UEFI, and dropping the syslinux mode removes a set of concatenated
# boot configs (archiso_head + archiso_sys + archiso_tail) that would otherwise
# need their own testing. To bring BIOS back: add 'bios.syslinux' here and copy
# the releng profile's syslinux/ directory into this one.
bootmodes=('uefi.systemd-boot')

arch="x86_64"
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"

# Substituted into %KERNEL_PARAMS% in the boot entry.
#
# `splash` activates the Plymouth theme defined in airootfs; `quiet` keeps the
# kernel from printing over it. The two go together — a splash with kernel
# messages scrolling across it looks broken.
#
# Recovery, if the splash ever hangs: the systemd-boot editor is enabled, so
# the command line can be amended at the menu with `plymouth.enable=0`, and
# Ctrl+Alt+F2 still reaches a shell.
kernel_params_x86_64="quiet splash"

# zstd rather than releng's xz: the build is several times faster and the ISO
# only a little larger, which matters a lot while iterating.
airootfs_image_tool_options=('-comp' 'zstd' '-Xcompression-level' '15' '-b' '1M')

bootstrap_tarball_compression=('zstd' '-c' '-T0' '--auto-threads=logical' '--long' '-19')

# /etc/shadow and /etc/gshadow are deliberately absent: they belong to the
# `filesystem` package, and airootfs is copied into place *before* pacstrap
# runs, so shipping our own copy would make pacman refuse to install the
# package that owns them. autologin on tty1 needs no password anyway.
#
# The two executables are mandatory, and not because of a missing default:
# mkarchiso copies airootfs with `cp -af --no-preserve=ownership,mode`, so a
# file shipped as 0755 arrives as 0644. For noos-installer that meant cage
# failing with "Failed to spawn client: Permission denied"; for noos-eee-off
# it would mean every invocation failing silently, since udev and the unit both
# call it by path. Files installed by packages keep their modes; only the
# profile's own files need declaring here.
file_permissions=(
  ["/root"]="0:0:750"
  ["/usr/bin/noos-installer"]="0:0:755"
  ["/usr/local/bin/noos-eee-off"]="0:0:755"
)
