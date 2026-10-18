#!/usr/bin/env bash
#
# Creates and boots a noos-os test VM.
#
#     ./scripts/run-vm.sh [chemin/vers/iso]     créer et démarrer
#     ./scripts/run-vm.sh clean                 supprimer la VM de test
#
# Defaults to out/noos-os.iso, which is where scripts/build-iso.sh will put it.

set -euo pipefail

readonly VM_NAME="noos-test"
readonly POOL_DIR="/var/lib/libvirt/images"
readonly DISK="${POOL_DIR}/${VM_NAME}.qcow2"
readonly RAM_MIB=6144
readonly VCPUS=4
readonly DISK_GIB=60

# The system connection, explicitly.
#
# virt-install and virsh default to qemu:///session when run as an ordinary
# user. That connection runs VMs as the invoking user, which cannot write to
# /var/lib/libvirt/images (owned by libvirt-qemu) — the failure is a bare
# "Permission denied" while creating the disk, with no hint that the
# connection is the cause.
readonly CONNECT="qemu:///system"

die() { echo "erreur : $*" >&2; exit 1; }

command -v virsh >/dev/null 2>&1 \
  || die "libvirt n'est pas installé — lance d'abord : sudo ./scripts/setup-virt.sh"

# ---------------------------------------------------------------- clean mode

if [[ "${1:-}" == "clean" ]]; then
  virsh --connect "$CONNECT" dominfo "$VM_NAME" >/dev/null 2>&1 \
    || die "la VM $VM_NAME n'existe pas"
  virsh --connect "$CONNECT" destroy "$VM_NAME" 2>/dev/null || true
  virsh --connect "$CONNECT" undefine "$VM_NAME" --remove-all-storage
  echo "VM $VM_NAME supprimée."
  exit 0
fi

# --------------------------------------------------------------- create mode

readonly ISO="${1:-out/noos-os.iso}"

[[ -f "$ISO" ]] || die "ISO introuvable : $ISO
Construis-la d'abord, ou passe le chemin en argument."
[[ -r "$ISO" ]] || die "ISO illisible : $ISO"

virsh --connect "$CONNECT" dominfo "$VM_NAME" >/dev/null 2>&1 && die \
  "la VM $VM_NAME existe déjà. Supprime-la avec : $0 clean"

command -v virt-install >/dev/null 2>&1 \
  || die "virt-install est absent — réinstalle virt-manager"

echo "Création de la VM $VM_NAME"
echo "  connexion : $CONNECT"
echo "  ISO       : $ISO"
echo "  disque    : ${DISK_GIB} Gio (${DISK})"
echo "  RAM       : ${RAM_MIB} Mio, ${VCPUS} vCPU"
echo

virt-install \
  --connect "$CONNECT" \
  --name "$VM_NAME" \
  --memory "$RAM_MIB" \
  --vcpus "$VCPUS" \
  --cpu host-passthrough \
  --machine q35 \
  --boot uefi \
  --disk "path=${DISK},size=${DISK_GIB},format=qcow2,bus=virtio" \
  --cdrom "$ISO" \
  --network "network=default,model=virtio" \
  --graphics "spice,listen=none" \
  --video "model=virtio,accel3d=yes" \
  --sound none \
  --os-variant archlinux \
  --noautoconsole

cat <<EOF

VM $VM_NAME démarrée.

  virt-manager                                    suivre la console
  virt-viewer --connect qemu:///system $VM_NAME   console seule
  virsh console $VM_NAME                          console série (quitter : Ctrl+])

  $0 clean                                        tout supprimer et recommencer

Note : accel3d=yes demande virtio-gpu-gl. Si la console reste noire, c'est
WebKitGTK qui n'arrive pas à construire son framebuffer — édite la VM dans
virt-manager et repasse le modèle vidéo sur « Virtio » sans accélération 3D,
ou sur « QXL ». C'est précisément le risque identifié dans le plan.
EOF
