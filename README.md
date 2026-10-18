# noos-os

[![build-iso](https://github.com/Chomiam/noos-os/actions/workflows/build-iso.yml/badge.svg)](https://github.com/Chomiam/noos-os/actions/workflows/build-iso.yml)

Une distribution basée sur Arch : une ISO live construite avec `archiso` qui
démarre directement sur un **installateur graphique plein écran** — pas de
bureau — et qui installe un système Arch avec **btrfs + snapshots restaurables
depuis limine**.

## Construire l'ISO

### Avec GitHub Actions

Le workflow `build-iso` construit l'ISO dans un conteneur Arch à chaque
poussée, et l'attache à une *release* quand un tag `v*` est poussé. Rien à
installer en local :

```bash
git tag v0.1.0 && git push origin v0.1.0
```

L'ISO fait environ 1,5 Gio. GitHub plafonne un fichier de *release* à 2 Gio, et
le workflow **vérifie la taille avant de publier** : une release sans son ISO
serait pire que pas de release.

### En local

```bash
sudo pacman -S archiso          # mkarchiso, mksquashfs, xorriso, mtools
sudo ./scripts/build-iso.sh
```

Le script compile l'installateur en tant qu'utilisateur — jamais en root : un
build Rust lancé par root laisse des fichiers root dans `~/.cargo` et `~/.bun`
et casse tous les builds suivants. En conteneur il n'y a pas d'utilisateur à
protéger, donc le script y compile en root (détecté par `CI`).

Résultat dans `out/`, accompagné d'un `.sha256`. Compte plusieurs minutes au
premier build : environ 1,5 Gio de paquets, surtout `linux-firmware`.

Le script se termine par une **vérification de l'ISO produite** : 41 contrôles
qui extraient le squashfs et vérifient ce qui a déjà cassé silencieusement —
liens symboliques morts, binaire non exécutable, trousseau pacman absent,
systemd-firstboot non masqué. Un build qui passe cette étape est cohérent avec
ses sources ; `mkarchiso` seul ne le garantit pas.

## Prérequis

Sur la machine de build :

- **Rust** (rustup sans root, dans `~/.cargo` — ou le paquet `rust`)
- **bun**, pour le frontend
- `libxkbcommon` et `xorgproto` : les tables de clavier sont générées depuis
  `xkbcli` et `keysymdef.h`, donc depuis les données XKB du système
- Les dépendances système de Tauri : `webkit2gtk-4.1`, `gtk3`, `libsoup3`,
  `librsvg`

Pour tester l'ISO en machine virtuelle, voir la section *Tester* plus bas.

## Tester

### En machine virtuelle

```bash
sudo ./scripts/setup-virt.sh              # une seule fois
# puis rouvrir sa session pour que le groupe libvirt prenne effet
./scripts/run-vm.sh out/noos-os.iso
```

`run-vm.sh` crée une VM UEFI (OVMF), 6 Gio de RAM, disque qcow2 de 60 Gio,
virtio-gpu avec accélération 3D. `./scripts/run-vm.sh clean` efface tout.

### Ce qu'il faut vérifier

1. L'ISO démarre sur systemd-boot et lance l'installateur en plein écran.
2. Le parcours complet aboutit, le terminal affiche la sortie de pacstrap.
3. Après redémarrage, le système installé démarre sur limine.

Sur le système installé :

```bash
findmnt                          # les sous-volumes btrfs sont-ils montés ?
snapper -c root list             # des snapshots existent-ils ?
limine-snapper-list              # sont-ils exposés dans le menu limine ?
```

### Si l'interface ne s'affiche pas

C'est le risque principal du projet : WebKitGTK peut ne pas arriver à
construire son framebuffer sur un GPU virtuel. Bascule sur tty2 avec
`Ctrl+Alt+F2` et regarde :

```bash
journalctl -b
WLR_RENDERER=pixman cage -d -s /usr/bin/noos-installer   # rendu logiciel
```

Le binaire désactive déjà `WEBKIT_DISABLE_DMABUF_RENDERER` de lui-même.

## Ce que l'installateur produit

Disque unique, GPT, effacement complet :

| # | Partition | FS | Taille | Montage |
|---|---|---|---|---|
| 1 | ESP | **FAT32** | 8 Gio | `/boot` |
| 2 | swap | swap | 8 Gio (optionnel) | — |
| 3 | racine | btrfs | le reste | `/` + sous-volumes |

Sous-volumes : `@` → `/` (le seul snapshoté), `@home`, `@log`, `@cache`,
`@tmp`, `@root`, `@srv`.

Trois contraintes non négociables, chacune expliquée dans le code :

- **`/boot` doit être FAT32.** limine ne lit que FAT12/16/32 et ISO9660 — une
  partition `/boot` en ext4 rendrait le système non amorçable.
- **8 Gio de `/boot` ne sont pas du gaspillage.** `limine-snapper-sync` y copie
  le noyau et l'initramfs *de chaque snapshot* pour que chaque snapshot soit
  amorçable. Le minimum documenté est 4 Gio.
- **La swap est une partition, pas un fichier.** Un sous-volume contenant un
  swapfile actif ne peut pas être snapshoté : un swapfile dans `@` bloquerait
  toute la chaîne de rollback.

## Organisation

```
installer/
  core/           moteur d'installation, sans dépendance Tauri — 185 tests
    src/          layout, btrfs, limine, snapshots, kernel, keyboard,
                  network, powersave, gpu, services, validate
    src/bin/      CLI : plan | gpu | check | install
  src-tauri/      l'application Tauri v2 (interface)
  src/            frontend React (9 écrans, FR/EN)
iso/
  profiledef.sh   profil archiso (UEFI uniquement)
  packages.x86_64 paquets du média live
  airootfs/       autologin tty1 → cage, réseau, trousseau pacman, Plymouth
  efiboot/        configuration systemd-boot du live
scripts/
  build-iso.sh               construit l'ISO et la vérifie
  run-vm.sh                  crée et démarre la VM de test
  setup-virt.sh              installe et configure libvirt + virt-manager
  fix-ufw-libvirt.sh         rend le réseau libvirt compatible avec ufw
  generate-keymaps.js        tables de clavier, depuis xkbcli
  generate-map-data.js       carte du monde et fuseaux, depuis Natural Earth
  generate-nvidia-table.js   table de détection NVIDIA, depuis les sources NVIDIA
  generate-plymouth-frames.js images du splash, depuis noos-svg.svg
.github/workflows/
  build-iso.yml   construit l'ISO en CI et la publie sur tag
```

Le moteur est une bibliothèque sans dépendance Tauri : l'application graphique,
la CLI et les tests pilotent tous le même code. Tout ce qui *décrit*
l'installation (tailles de partitions, `fstab`, `limine.conf`, réglages
snapper) est une fonction pure, donc vérifiable sans root.

## La CLI

```bash
noos-install plan 500 --swap      # plan de partitions, sans rien toucher
noos-install gpu                  # matériel graphique et pile de codecs
noos-install check config.json    # valide et affiche les fichiers générés
noos-install install config.json  # exécute (nécessite root)
```

## Notes

- **L'ISO live est UEFI uniquement.** Le système *installé* démarre avec limine
  sur UEFI comme sur BIOS, mais le média live se limite à UEFI : tout le
  matériel visé l'est, et abandonner syslinux supprime un ensemble de fichiers
  de configuration concaténés à tester. Pour le rétablir : ajouter
  `bios.syslinux` à `bootmodes` dans `profiledef.sh` et copier le répertoire
  `syslinux/` du profil releng.
- **Aucun fichier appartenant à un paquet n'est livré dans `airootfs`.**
  `airootfs` est copié *avant* que `pacstrap` tourne, donc livrer par exemple
  `/etc/locale.gen` (qui appartient à `glibc`) ferait refuser l'installation du
  paquet qui le possède. Les fichiers concernés sont modifiés par des hooks
  pacman à la place.
- **`nvidia-detect` n'existe pas sur Arch.** La classification des GPU NVIDIA
  se fait hors-ligne depuis une table d'identifiants PCI embarquée, générée
  depuis les sources NVIDIA par `scripts/generate-nvidia-table.js`.
