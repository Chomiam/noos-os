#!/usr/bin/env bash
#
# Installs and configures libvirt + virt-manager so the noos-os ISO can be
# tested in a virtual machine.
#
# Run once:
#     sudo ./scripts/setup-virt.sh
#
# The script is idempotent: running it again only re-checks the configuration.

set -euo pipefail

readonly PACKAGES=(
  virt-manager
  virt-install   # not pulled in by virt-manager on all versions; used by run-vm.sh
  libvirt
  qemu-desktop   # qemu-full is only needed for other architectures
  edk2-ovmf      # UEFI firmware — the ISO has to be tested on UEFI, not BIOS
  dnsmasq        # implements libvirt's default NAT network
  swtpm          # software TPM; not required today, but cheap to have ready
  iptables       # nft backend by default since April 2026; iptables-nft is gone
  virt-viewer    # a viewer that does not depend on the virt-manager GUI
)

# The modular daemons, one per driver.
#
# ArchWiki's virt-manager page recommends these over the monolithic
# libvirtd.service, which upstream intends to remove. The two sets cannot run
# at the same time. `virtlogd` is included because the QEMU driver delegates
# its logging to it.
readonly DAEMONS=(
  virtqemud.socket      # the QEMU driver — the one that actually runs VMs
  virtnetworkd.socket   # virtual networks, including `default`
  virtnodedevd.socket   # host device enumeration
  virtstoraged.socket   # storage pools
  virtsecretd.socket    # secrets
  virtnwfilterd.socket  # network filters
  virtinterfaced.socket # host interfaces
  virtlogd.socket       # domain logging
)

die() { echo "erreur : $*" >&2; exit 1; }

check() { printf '  %-32s %s\n' "$1" "$2"; }

[[ $EUID -eq 0 ]] || die "à lancer en root : sudo $0"

# Resolve the human behind sudo: root is not in the desktop session and would
# end up in the libvirt group for nothing.
readonly TARGET_USER="${SUDO_USER:-}"
[[ -n $TARGET_USER && $TARGET_USER != root ]] \
  || die "lance le script via sudo depuis ton compte utilisateur, pas depuis un shell root"

echo "== Paquets =="
pacman -S --needed --noconfirm "${PACKAGES[@]}"

echo
echo "== Daemons libvirt (modulaires) =="
# The monolithic libvirtd.service must not also be running: the two sets
# conflict. Disable it first in case an earlier attempt enabled it.
systemctl disable --now libvirtd.service 2>/dev/null || true
systemctl disable --now libvirtd.socket 2>/dev/null || true

systemctl enable --now "${DAEMONS[@]}"

echo
echo "== Réseau NAT par défaut =="
# libvirt ships the network definition but leaves it stopped and without
# autostart, so a fresh install has no outbound network until this is done.
virsh net-info default >/dev/null 2>&1 || die "le réseau « default » est introuvable"
virsh net-autostart default
virsh net-start default 2>/dev/null || true

echo
echo "== Groupes =="
# libvirt: password-less access to the daemon socket. kvm: reach /dev/kvm
# directly, which is what lets virt-manager report acceleration as available.
usermod -aG libvirt,kvm "$TARGET_USER"

echo
echo "== Vérifications =="

if [[ -e /dev/kvm ]]; then
  check "/dev/kvm" "présent"
else
  check "/dev/kvm" "ABSENT — pas d'accélération matérielle"
fi

if grep -qE 'vmx|svm' /proc/cpuinfo; then
  check "virtualisation CPU" "disponible"
else
  check "virtualisation CPU" "ABSENTE — active VT-x/AMD-V dans le BIOS"
fi

if virsh -c qemu:///system version >/dev/null 2>&1; then
  check "connexion qemu:///system" "ok"
else
  check "connexion qemu:///system" "ÉCHEC"
fi

if virsh net-list --name 2>/dev/null | grep -qx default; then
  check "réseau default" "actif"
else
  check "réseau default" "inactif"
fi

# Arch installs OVMF under a versioned filename; take whichever variant exists.
readonly ovmf="$(find /usr/share/edk2 -name 'OVMF_CODE*.fd' 2>/dev/null | head -1)"
check "firmware UEFI (OVMF)" "${ovmf:-INTROUVABLE}"

if id -nG "$TARGET_USER" | tr ' ' '\n' | grep -qx libvirt; then
  check "groupe libvirt ($TARGET_USER)" "ok"
else
  check "groupe libvirt ($TARGET_USER)" "ÉCHEC"
fi

# The authoritative end-to-end check, shipped by the libvirt package itself.
echo
echo "== virt-host-validate =="
virt-host-validate qemu || true

cat <<EOF

Terminé.

  $TARGET_USER a été ajouté aux groupes libvirt et kvm.
  Un groupe n'est pris en compte qu'à la prochaine ouverture de session :
  déconnecte-toi puis reconnecte-toi (ou redémarre) avant d'utiliser
  virt-manager.

Ensuite :

  ./scripts/run-vm.sh out/noos-os.iso   # créer et démarrer la VM de test
  virt-manager                          # ou l'interface graphique

Deux pièges connus à garder en tête :

  * edk2-ovmf 202505-1 et suivants font geler le démarrage UEFI des invités
    amorcés par GRUB. Notre ISO live passe par systemd-boot puis rEFInd, et ni
    l'un ni l'autre n'est GRUB — mais si un invité se fige au démarrage, c'est
    la première piste.

  * L'accélération OpenGL 3D avec egl-headless est cassée sur les hôtes
    NVIDIA. Cette machine est en AMD, elle n'est pas concernée.

EOF
