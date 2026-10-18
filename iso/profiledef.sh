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

# UEFI only, and systemd-boot only as a hand-off.
#
# The *installed* system boots with rEFInd, which is UEFI-only — it has no
# equivalent of writing a bootloader into a disk's MBR, and the installer says
# so rather than leaving the disk unbootable.
#
# This mode is not what the live medium shows. mkarchiso has no rEFInd bootmode,
# so systemd-boot is built here — it is what produces a valid UEFI image — and
# its single entry chainloads rEFInd, which draws the menu. See the entry in
# efiboot/ and the staging step in scripts/build-iso.sh.
#
# Dropping syslinux also removes a set of concatenated boot configs
# (archiso_head + archiso_sys + archiso_tail) that would need their own testing.
bootmodes=('uefi.systemd-boot')

arch="x86_64"
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"

# Kernel parameters for the live medium.
#
# Kept for the profile's own record, but note that nothing substitutes it any
# more: `%KERNEL_PARAMS%` has no user now that rEFInd, not systemd-boot, is what
# loads the live kernel. The command line actually used is written by
# scripts/build-iso.sh into rEFInd's configuration, and it matches this line.
# Editing one without the other has no effect on the boot.
#
# `splash` activates the Plymouth theme defined in airootfs; `quiet` keeps the
# kernel from printing over it. The two go together — a splash with kernel
# messages scrolling across it looks broken.
#
# Recovery, if the splash ever hangs: rEFInd has an options editor (press F2 on
# an entry), so `plymouth.enable=0` can be added at the menu, and Ctrl+Alt+F2
# still reaches a shell.
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
