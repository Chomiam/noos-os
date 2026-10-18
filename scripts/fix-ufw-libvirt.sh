#!/usr/bin/env bash
#
# Makes libvirt's default NAT network work alongside ufw.
#
# Run once:
#     sudo ./scripts/fix-ufw-libvirt.sh
#
# The problem
# -----------
# ufw and libvirt both write netfilter rules, and ufw's win. Two of its
# defaults break a libvirt guest:
#
#   * `DEFAULT_FORWARD_POLICY="DROP"` blocks the forwarding that NAT needs.
#   * ufw's `after.rules` sends DHCP (udp/67-68) to its input *policy*, which
#     is also DROP — so the guest never even gets an address. It fails before
#     NAT is reached, which is why the symptom is "no network at all" rather
#     than "no internet".
#
# The fix is three rules and one setting. None of them weaken the firewall for
# the outside world: they only open the virtual bridge.

set -euo pipefail

die() { echo "erreur : $*" >&2; exit 1; }
step() { printf '\n== %s ==\n' "$*"; }

[[ $EUID -eq 0 ]] || die "à lancer en root : sudo $0"

command -v ufw >/dev/null 2>&1 || die "ufw n'est pas installé"

if ! systemctl is-active --quiet ufw; then
  die "ufw n'est pas actif : rien à corriger"
fi

# The interface carrying the default route — what libvirt's NAT masquerades
# through.
readonly WAN="$(ip route show default | awk '/^default/ {print $5; exit}')"
[[ -n $WAN ]] || die "impossible de déterminer l'interface sortante"
echo "  interface sortante : $WAN"

readonly NET="192.168.122.0/24"

step "1. Autoriser l'entrée sur le pont virtuel"
# This is the one that fixes DHCP and DNS. Without it the guest gets no IPv4
# address at all and nothing downstream can work.
ufw allow in on virbr0

step "2. Autoriser le routage du pont vers l'extérieur"
ufw route allow in on virbr0 out on "$WAN" from "$NET"

step "3. Autoriser le forwarding dans la politique par défaut"
# libvirt installs its own forward rules, but ufw's default policy drops
# packets before they are consulted.
readonly DEFAULT_UFW="/etc/default/ufw"
if grep -q '^DEFAULT_FORWARD_POLICY="ACCEPT"' "$DEFAULT_UFW"; then
  echo "  déjà réglé"
else
  cp -a "$DEFAULT_UFW" "${DEFAULT_UFW}.bak.$(date +%s)"
  sed -i 's/^DEFAULT_FORWARD_POLICY=.*/DEFAULT_FORWARD_POLICY="ACCEPT"/' "$DEFAULT_UFW"
  echo "  DEFAULT_FORWARD_POLICY -> ACCEPT (sauvegarde conservée)"
fi

step "4. Masquerade explicite"
# libvirt adds this itself for a nat-mode network, but only when it manages to
# install its rules — which it cannot while the forward policy drops them. The
# duplicate is harmless: the first matching rule wins.
readonly BEFORE="/etc/ufw/before.rules"
if grep -q 'POSTROUTING -s 192.168.122.0/24' "$BEFORE"; then
  echo "  déjà présent"
else
  cp -a "$BEFORE" "${BEFORE}.bak.$(date +%s)"

  # The nat section must come before the *filter section: ufw reads the file
  # top to bottom and a table cannot be declared after rules that use it.
  tmp="$(mktemp)"
  cat > "$tmp" <<EOF
# Added by noos-os: libvirt NAT.
*nat
:PREROUTING ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
-A POSTROUTING -s ${NET} -o ${WAN} -j MASQUERADE
COMMIT

EOF
  cat "$BEFORE" >> "$tmp"
  mv "$tmp" "$BEFORE"
  echo "  section nat ajoutée (sauvegarde conservée)"
fi

step "5. Rechargement"
ufw reload

step "Vérifications"
printf '  %-34s %s\n' "ufw actif" "$(systemctl is-active ufw)"
printf '  %-34s %s\n' "interface virbr0" "$(ip -br addr show virbr0 2>/dev/null | awk '{print $1, $3}')"
printf '  %-34s %s\n' "politique de forwarding" "$(grep -oP '(?<=^DEFAULT_FORWARD_POLICY=")[^"]*' /etc/default/ufw)"
printf '  %-34s %s\n' "masquerade dans before.rules" "$(grep -c 'POSTROUTING -s 192.168.122.0/24' "$BEFORE")"

cat <<EOF

Terminé.

  Redémarre la VM pour qu'elle redemande une adresse :

    virsh --connect qemu:///system destroy archlinux
    virsh --connect qemu:///system start archlinux

  Puis vérifie, au bout d'une minute :

    virsh --connect qemu:///system net-dhcp-leases default

  Un bail doit apparaître. S'il n'y en a toujours pas :

    sudo ufw status verbose        règles actives
    sudo nft list ruleset | grep -A5 'chain forward'
    sudo journalctl -u libvirtd -n 50

  Pour tout annuler, les sauvegardes sont à côté des fichiers modifiés :

    sudo cp /etc/ufw/before.rules.bak.* /etc/ufw/before.rules
    sudo cp /etc/default/ufw.bak.* /etc/default/ufw
    sudo ufw reload

EOF
