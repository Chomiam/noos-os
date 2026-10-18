# noos-os

[![build-iso](https://github.com/Chomiam/noos-os/actions/workflows/build-iso.yml/badge.svg)](https://github.com/Chomiam/noos-os/actions/workflows/build-iso.yml)

Une distribution basée sur Arch : une ISO live construite avec `archiso` qui
démarre directement sur un **installateur graphique plein écran** — pas de
bureau — et qui installe un système Arch avec **btrfs + snapshots restaurables
depuis rEFInd**.

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

Le script se termine par une **vérification de l'ISO produite** : 20 contrôles
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
3. Après redémarrage, le système installé démarre sur rEFInd.

Sur le système installé :

```bash
findmnt                          # les sous-volumes btrfs sont-ils montés ?
snapper -c root list             # des snapshots existent-ils ?
cat /boot/EFI/refind/noos-snapshots/*.conf   # les entrées générées par refind-btrfs
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

- **`/boot` doit être FAT32.** Le firmware lit le FAT nativement et rEFInd en
  hérite ; un `/boot` en btrfs obligerait rEFInd à charger son propre pilote,
  dont le wiki Arch dit qu'il se trompe sur un volume dédupliqué ou en
  copy-on-write.
- **1 Gio de `/boot` suffisent.** Il porte le noyau, son initramfs, le microcode
  et les fichiers de rEFInd. Les 8 Gio d'avant servaient à `limine-snapper-sync`,
  qui y copiait un noyau par instantané ; rEFInd partage un seul noyau et ne
  change que le sous-volume racine. **Contrepartie assumée : les instantanés ne
  restaurent plus le noyau** — une mise à jour qui casse le démarrage ne se
  rattrape plus en démarrant un instantané.
- **La swap est une partition, pas un fichier.** Un sous-volume contenant un
  swapfile actif ne peut pas être snapshoté : un swapfile dans `@` bloquerait
  toute la chaîne de rollback.

## Le dépôt Chaotic-AUR

Trois paquets dont la distribution a besoin viennent de l'AUR, qui ne distribue
que des sources. `refind-btrfs` est un programme Python — il *pourrait* être
construit — mais il vient de **Chaotic-AUR** comme `yay`, pour qu'il n'y ait
qu'un seul chemin pour les paquets AUR.

L'historique vaut d'être connu : la chaîne de snapshots reposait sur
`limine-snapper-sync` et `limine-mkinitcpio-hook`, qui se compilent en **image
native GraalVM** — un JDK de 300 Mo, Gradle, et une compilation ahead-of-time
qui échoue sous `makepkg` en sortant 30 sans le moindre message.

Le dépôt est activé **sur la machine de build et sur la cible** :

- au build, pour que le média live embarque `chaotic-keyring` et
  `chaotic-mirrorlist` ;
- sur la cible, pour que l'installateur y prenne la chaîne de snapshots et `yay`
  comme dans n'importe quel dépôt — et pour que l'utilisateur garde ensuite une
  vaste source de paquets AUR précompilés.

Le maillon est le trousseau : `pacman-key --populate` du média live importe la
clé de Chaotic, et `pacstrap` copie ce trousseau dans la cible. C'est ce qui
permet au premier `pacman -Sy` de la cible de vérifier ce que Chaotic sert.

**C'est un dépôt tiers auquel la distribution fait confiance par défaut.** Tous
ses paquets sont construits par l'infrastructure de Chaotic, ni par nous ni par
l'utilisateur. C'est le prix ; l'alternative était d'embarquer une chaîne de
compilation dans un installateur et de demander à la machine de compiler du Java
AOT. Voir `installer/core/src/chaotic.rs`.

## Apparence

Sur GNOME, le thème d'icônes **Papirus-Dark** est appliqué avec le schéma de
couleurs sombre, cohérent avec l'installateur, l'écran Plymouth et le thème
rEFInd.

Il ne l'est pas par un réglage utilisateur : au moment de l'installation
personne ne s'est jamais connecté, et `dconf` comme `gsettings set` veulent le
bus système — celui-là même dont l'absence faisait échouer `snapper`. Un
**override de schéma** à la place : `glib-compile-schemas` l'intègre à
`gschemas.compiled`, et une clé que personne n'a jamais écrite se lit comme la
valeur par défaut surchargée. Sans bus, sans utilisateur, et valable pour tout
compte créé plus tard — ce qu'un réglage par utilisateur ne ferait pas.

Quatre familles d'applications dessinent leurs propres widgets, et **aucune ne
lit le réglage d'une autre** :

| Boîte à outils | Levier |
|---|---|
| GTK 4 / libadwaita | `color-scheme='prefer-dark'` dans l'override |
| GTK 3 | `gtk-theme='Adwaita-dark'` **et** `/etc/gtk-3.0/settings.ini` |
| Qt 5 et 6 | `qt5ct` + `qt6ct`, leur configuration et `QT_QPA_PLATFORMTHEME=qt5ct:qt6ct` |
| Electron | aucun : il interroge GTK, donc il suit les deux premiers |

Qt 5 compte vraiment ici : **VLC est bâti contre Qt 5** sur Arch, c'est le
lecteur de la distribution. Les deux versions sont couvertes par une seule
variable, parce qu'elle accepte une liste séparée par des deux-points.

**Les applications Flatpak** : celles en GTK 4 suivent le portail de réglages,
qui lit l'hôte — elles sont donc sombres sans rien faire. Celles en GTK 3 ne
voient pas `/usr/share/themes` et ont besoin d'une extension installée dans le
bac à sable. L'installateur ne peut pas la poser : `flatpak` a besoin du bus
système, absent du chroot. À faire une fois après la première connexion :

```bash
flatpak install flathub org.gtk.Gtk3theme.Adwaita-dark
```

## Organisation

```
installer/
  core/           moteur d'installation, sans dépendance Tauri — 215 tests
    src/          layout, btrfs, refind, snapshots, chaotic, flatpak, kernel,
                  keyboard, network, powersave, gpu, services, validate
    src/bin/      CLI : plan | gpu | check | install
  src-tauri/      l'application Tauri v2 (interface)
  src/            frontend React (9 écrans, FR/EN)
iso/
  profiledef.sh   profil archiso (UEFI uniquement)
  pacman.conf     miroirs Arch + section [chaotic-aur]
  packages.x86_64 paquets du média live
  airootfs/       autologin tty1 → cage, réseau, trousseau pacman, Plymouth
  efiboot/        configuration systemd-boot du live
scripts/
  build-iso.sh                  construit l'ISO et la vérifie
  run-vm.sh                     crée et démarre la VM de test
  setup-virt.sh                 installe et configure libvirt + virt-manager
  fix-ufw-libvirt.sh            rend le réseau libvirt compatible avec ufw
  generate-keymaps.js           tables de clavier, depuis xkbcli
  generate-map-data.js          carte du monde et fuseaux, depuis Natural Earth
  generate-nvidia-table.js      table de détection NVIDIA, depuis les sources NVIDIA
  generate-plymouth-frames.js   images du splash, depuis noos-svg.svg
  generate-refind-theme.js      thème du menu rEFInd, depuis noos-svg.svg
  generate-flatpak-remote.sh    définition du dépôt Flathub, depuis Flathub
.github/workflows/
  build-iso.yml   construit l'ISO en CI et la publie sur tag
```

Le moteur est une bibliothèque sans dépendance Tauri : l'application graphique,
la CLI et les tests pilotent tous le même code. Tout ce qui *décrit*
l'installation (tailles de partitions, `fstab`, `refind.conf`, réglages
snapper) est une fonction pure, donc vérifiable sans root.

## La CLI

```bash
noos-install plan 500 --swap      # plan de partitions, sans rien toucher
noos-install gpu                  # matériel graphique et pile de codecs
noos-install check config.json    # valide et affiche les fichiers générés
noos-install install config.json  # exécute (nécessite root)
```

## Notes

- **UEFI uniquement, pour le live comme pour la cible.** rEFInd n'a pas
  d'équivalent à l'écriture d'un chargeur dans le MBR d'un disque : sur une
  machine BIOS, l'installateur échoue en le disant, plutôt que de laisser un
  disque non amorçable passer pour une installation réussie.
- **Le média live passe par systemd-boot avant rEFInd.** mkarchiso n'a pas de
  mode rEFInd et ne lit que `loader.conf` et `entries/` d'un profil : il n'existe
  aucun moyen supporté de lui confier un binaire EFI supplémentaire. systemd-boot
  est donc construit normalement — c'est lui qui produit une image UEFI valide —
  et son unique entrée, en `timeout 0`, passe la main à rEFInd sans jamais
  afficher de menu. Le détail est dans `scripts/build-iso.sh`.
- **Aucun fichier appartenant à un paquet n'est livré dans `airootfs`.**
  `airootfs` est copié *avant* que `pacstrap` tourne, donc livrer par exemple
  `/etc/locale.gen` (qui appartient à `glibc`) ferait refuser l'installation du
  paquet qui le possède. Les fichiers concernés sont modifiés par des hooks
  pacman à la place.
- **`nvidia-detect` n'existe pas sur Arch.** La classification des GPU NVIDIA
  se fait hors-ligne depuis une table d'identifiants PCI embarquée, générée
  depuis les sources NVIDIA par `scripts/generate-nvidia-table.js`.
