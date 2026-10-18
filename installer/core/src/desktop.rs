//! Desktop environment profiles offered by the installer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Desktop {
    Gnome,
    Plasma,
    Cinnamon,
    Budgie,
}

/// Which display manager gets to own `display-manager.service`.
///
/// All three provide that unit, so exactly one may be enabled — enabling two
/// yields a systemd link-ownership warning and a nondeterministic greeter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayManager {
    Gdm,
    Sddm,
    Lightdm,
}

impl DisplayManager {
    /// The package that provides this display manager.
    pub fn package(self) -> &'static str {
        match self {
            DisplayManager::Gdm => "gdm",
            DisplayManager::Sddm => "sddm",
            DisplayManager::Lightdm => "lightdm",
        }
    }

    pub fn service(self) -> &'static str {
        match self {
            DisplayManager::Gdm => "gdm.service",
            DisplayManager::Sddm => "sddm.service",
            DisplayManager::Lightdm => "lightdm.service",
        }
    }

    /// Every display manager that must be explicitly disabled. Belt and
    /// braces: a stray enabled unit from a metapackage dependency would
    /// otherwise fight the one we want.
    pub fn others(self) -> &'static [&'static str] {
        match self {
            DisplayManager::Gdm => &["sddm.service", "lightdm.service"],
            DisplayManager::Sddm => &["gdm.service", "lightdm.service"],
            DisplayManager::Lightdm => &["gdm.service", "sddm.service"],
        }
    }
}

pub struct DesktopProfile {
    /// Extra packages for this desktop, on top of [`COMMON_PACKAGES`].
    pub packages: &'static [&'static str],
    /// Packages the profile installs and then takes back out again.
    ///
    /// Only useful where a *group* is installed: `pacstrap gnome` pulls all 58
    /// members, and no pacman-side mechanism selects a subset of a group. So the
    /// group goes in whole and these come back out in the desktop step.
    ///
    /// Every entry must be a leaf — nothing else installed may depend on it.
    /// pacman will happily remove a package something else needs and leave that
    /// something broken, and the failure surfaces much later as a panel or a
    /// button that does nothing.
    pub removals: &'static [&'static str],
    pub display_manager: DisplayManager,
    /// `greeter-session` for `/etc/lightdm/lightdm.conf`, when relevant.
    pub greeter_session: Option<&'static str>,
}

/// Installed for every desktop choice: audio, networking, fonts, portals.
///
/// Bluetooth, printing and power management are deliberately absent — they are
/// optional features with their own package sets in [`crate::services`], and
/// listing them here would install them regardless of the user's choice.
///
/// `xdg-desktop-portal` needs a backend matching the desktop or file pickers
/// and Flatpak integration misbehave — each profile adds its own below.
pub const COMMON_PACKAGES: &[&str] = &[
    "networkmanager",
    "pipewire",
    "pipewire-pulse",
    "pipewire-alsa",
    "wireplumber",
    "xdg-user-dirs",
    "xdg-desktop-portal",
    "noto-fonts",
    "ttf-dejavu",
    "ttf-liberation",
];

impl Desktop {
    pub fn all() -> [Desktop; 4] {
        [
            Desktop::Gnome,
            Desktop::Plasma,
            Desktop::Cinnamon,
            Desktop::Budgie,
        ]
    }

    pub fn profile(self) -> &'static DesktopProfile {
        match self {
            Desktop::Gnome => &GNOME,
            Desktop::Plasma => &PLASMA,
            Desktop::Cinnamon => &CINNAMON,
            Desktop::Budgie => &BUDGIE,
        }
    }

    /// Packages to hand to `pacstrap`, common set included.
    ///
    /// Groups (`gnome`, `plasma`, `budgie`) are passed by name — pacstrap
    /// resolves them. Cinnamon has no group at all on Arch, which is why its
    /// list is spelled out.
    pub fn packages(self) -> Vec<&'static str> {
        let mut packages: Vec<&'static str> = COMMON_PACKAGES.to_vec();
        packages.extend_from_slice(self.profile().packages);
        packages
    }

    pub fn display_manager(self) -> DisplayManager {
        self.profile().display_manager
    }

    /// Packages to take back out once `pacstrap` has resolved the group.
    ///
    /// Empty for every profile that lists its packages explicitly — only a
    /// group can pull in things nobody selected.
    pub fn removals(self) -> &'static [&'static str] {
        self.profile().removals
    }

    /// Package owning the desktop's session entry, for the summary screen.
    pub fn label(self) -> &'static str {
        match self {
            Desktop::Gnome => "GNOME",
            Desktop::Plasma => "KDE Plasma",
            Desktop::Cinnamon => "Cinnamon",
            Desktop::Budgie => "Budgie",
        }
    }
}

/// GNOME and Budgie are deliberately mutually exclusive: Budgie pulls in a
/// large slice of the GNOME stack, so installing both is slow and redundant.
///
/// The `gnome` group is 58 packages, and most of what it carries is a
/// full-fledged application suite nobody asked for. Everything in `removals`
/// below is a **leaf** — verified against `pacman -Sii` — so taking it out
/// cannot break the desktop. Together they are about 178 MiB.
static GNOME: DesktopProfile = DesktopProfile {
    packages: &[
        "gnome",
        // The two tools a stock GNOME is missing: the settings
        // `gnome-control-center` does not expose, and a fuller extension
        // manager than the one GNOME ships inside `gnome-shell`.
        //
        // `extension-manager` and **not** `gnome-shell-extensions`: the latter
        // is a *collection of extensions* — classic mode, the applications
        // menu, and a dozen others — installed whether or not anybody wants
        // them. The name is one word away from the tool and the two are
        // routinely confused.
        //
        // It is the same application Flathub publishes as
        // `com.mattjakeman.ExtensionManager`, at the same version — the
        // repository package is preferred here only because it is native: the
        // Flatpak would drag in the whole GNOME Platform runtime, close to a
        // gigabyte, for one small tool. Flathub is enabled anyway, so anyone
        // who wants the sandboxed one can still have it.
        //
        // GNOME also ships a manager of its own inside `gnome-shell` (the
        // Extensions app), which is why a stock GNOME can install extensions
        // without any of this. `gnome-extensions-app` is a separate, minimal
        // package for systems where that is not the case.
        "gnome-tweaks",
        "extension-manager",
        "gdm",
        "xdg-desktop-portal-gnome",
    ],
    removals: &[
        // Documentation and its viewer. The largest single item in the group,
        // and the Help buttons that open it are rarely pressed.
        "gnome-user-docs",
        "yelp",
        // The screen reader. Removing it is a real accessibility trade-off,
        // not a size one — put it back for any image meant for a wide public.
        "orca",
        // Browsers and stores: the distro ships pacman, yay and Shelly instead.
        "epiphany",
        "gnome-software",
        // Media players — all three of GNOME's, replaced by VLC.
        "showtime",
        "decibels",
        "gnome-music",
        // `grilo-plugins` is gnome-music's metadata backend and goes with it.
        "grilo-plugins",
        // Small single-purpose apps.
        "gnome-calculator",
        "gnome-maps",
        "gnome-weather",
        "gnome-clocks",
        "gnome-calendar",
        "gnome-contacts",
        "gnome-font-viewer",
        "snapshot",
        // The guided tour that runs on first login.
        "gnome-tour",
        // Remote desktop, both ends: nothing here wants to be a VNC server.
        "gnome-remote-desktop",
    ],
    display_manager: DisplayManager::Gdm,
    greeter_session: None,
};

// What the GNOME trim deliberately leaves alone, so a later reader does not
// "finish the job" and break something:
//
//   gnome-console        the only terminal in the group, and vim, nano and
//                        man-db are installed
//   papers, loupe        the only PDF and image viewers; VLC does neither
//   gnome-text-editor    the only graphical editor
//   gnome-disk-utility   how a user inspects the btrfs subvolumes
//   gnome-color-manager  \
//   tecla                 > hard dependencies of gnome-control-center —
//   malcontent           /  removing them takes the Settings app with them
//   gnome-backgrounds    the wallpaper set; replace it only once the image
//                        ships a wallpaper of its own

/// `plasma` (68 packages), never `plasma` + `kde-applications` (193).
static PLASMA: DesktopProfile = DesktopProfile {
    packages: &[
        "plasma",
        "konsole",
        "dolphin",
        "kate",
        "sddm",
        "xdg-desktop-portal-kde",
    ],
    removals: &[],
    display_manager: DisplayManager::Sddm,
    greeter_session: None,
};

static CINNAMON: DesktopProfile = DesktopProfile {
    packages: &[
        "cinnamon",
        "muffin",
        "nemo",
        "nemo-fileroller",
        "gnome-terminal",
        "gnome-keyring",
        "xdg-desktop-portal-gtk",
        "lightdm",
        "lightdm-slick-greeter",
    ],
    removals: &[],
    display_manager: DisplayManager::Lightdm,
    greeter_session: Some("lightdm-slick-greeter"),
};

static BUDGIE: DesktopProfile = DesktopProfile {
    packages: &[
        "budgie",
        "gnome-terminal",
        "gnome-keyring",
        "xdg-desktop-portal-gtk",
        "lightdm",
        "lightdm-slick-greeter",
    ],
    removals: &[],
    display_manager: DisplayManager::Lightdm,
    greeter_session: Some("lightdm-slick-greeter"),
};

/// `/etc/lightdm/lightdm.conf` for the LightDM-based desktops.
pub fn lightdm_conf(greeter_session: &str) -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [Seat:*]\n\
         greeter-session={greeter_session}\n\
         user-session={}\n",
        // slick-greeter is only the login screen; the session itself is the
        // desktop the user picked.
        greeter_session.replace("lightdm-slick-", "")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_desktop_has_a_display_manager_and_a_session() {
        for desktop in Desktop::all() {
            let profile = desktop.profile();
            assert!(
                !profile.packages.is_empty(),
                "{} n'a aucun paquet",
                desktop.label()
            );
            assert!(
                profile.packages.contains(&profile.display_manager.package()),
                "{} n'installe pas son gestionnaire de connexion {}",
                desktop.label(),
                profile.display_manager.service()
            );
        }
    }

    #[test]
    fn no_display_manager_conflict_between_profiles() {
        // gdm, sddm and lightdm all provide display-manager.service; each
        // profile must name exactly one as its own and disable the rest.
        for desktop in Desktop::all() {
            let dm = desktop.display_manager();
            assert_eq!(dm.others().len(), 2);
            for other in dm.others() {
                assert_ne!(*other, dm.service());
                assert!(
                    !desktop.profile().packages.contains(&dm_package(other)),
                    "{} installerait deux gestionnaires de connexion",
                    desktop.label()
                );
            }
        }
    }

    #[test]
    fn common_packages_are_included_and_deduplicated() {
        for desktop in Desktop::all() {
            let packages = desktop.packages();
            for common in COMMON_PACKAGES {
                assert!(packages.contains(common), "{common} manquant");
            }
            let unique: HashSet<_> = packages.iter().collect();
            assert_eq!(
                unique.len(),
                packages.len(),
                "doublon dans les paquets de {}",
                desktop.label()
            );
        }
    }

    #[test]
    fn cinnamon_never_pulls_the_full_kde_suite() {
        for desktop in Desktop::all() {
            let packages = desktop.packages();
            assert!(!packages.contains(&"kde-applications"));
        }
    }

    #[test]
    fn lightdm_desktops_configure_a_greeter() {
        for desktop in [Desktop::Cinnamon, Desktop::Budgie] {
            let greeter = desktop
                .profile()
                .greeter_session
                .expect("lightdm a besoin d'un greeter");
            let conf = lightdm_conf(greeter);
            assert!(conf.contains("greeter-session=lightdm-slick-greeter"));
        }
        assert!(Desktop::Gnome.profile().greeter_session.is_none());
    }

    /// Maps a `*.service` name back to its package, for the conflict test.
    fn dm_package(service: &str) -> &'static str {
        match service {
            "gdm.service" => "gdm",
            "sddm.service" => "sddm",
            "lightdm.service" => "lightdm",
            other => panic!("service inconnu : {other}"),
        }
    }

    #[test]
    fn only_a_group_install_needs_removals() {
        // Every other profile spells its packages out, so there is nothing to
        // take back out; a non-empty list there would mean a package installed
        // and removed in the same run.
        for desktop in Desktop::all() {
            if desktop == Desktop::Gnome {
                assert!(
                    !desktop.removals().is_empty(),
                    "le groupe gnome a besoin d'être trié"
                );
            } else {
                assert!(
                    desktop.removals().is_empty(),
                    "{} n'installe pas de groupe",
                    desktop.label()
                );
            }
        }
    }

    #[test]
    fn nothing_worth_keeping_is_removed() {
        // What the trim leaves the desktop with. The first group is the only
        // provider of something a user needs — a terminal, the PDF and image
        // viewers, an editor, the disks tool, the wallpapers. The second group
        // is what `gnome-control-center` depends on directly: removing one of
        // those takes the Settings app with it, which reads as a broken desktop
        // rather than as a missing accessory.
        const KEPT: &[&str] = &[
            "gnome-console",
            "papers",
            "loupe",
            "gnome-text-editor",
            "gnome-disk-utility",
            "gnome-backgrounds",
            "gnome-color-manager",
            "tecla",
            "malcontent",
            "gnome-shell",
            "gnome-session",
            "gnome-settings-daemon",
            "gnome-control-center",
            "gdm",
            "nautilus",
            "gnome-keyring",
            "gvfs",
        ];
        for name in KEPT {
            assert!(
                !Desktop::Gnome.removals().contains(name),
                "{name} ne doit pas figurer dans les retraits"
            );
        }
    }

    #[test]
    fn the_gnome_media_players_go_because_vlc_replaces_them() {
        // The group carries three of them; the image ships VLC instead.
        for player in ["showtime", "decibels", "gnome-music"] {
            assert!(
                Desktop::Gnome.removals().contains(&player),
                "{player} devrait être retiré au profit de VLC"
            );
        }
    }

    #[test]
    fn gnome_ships_tweaks_and_an_extension_manager() {
        let packages = Desktop::Gnome.packages();
        assert!(packages.contains(&"gnome-tweaks"), "gnome-tweaks manque");
        assert!(
            packages.contains(&"extension-manager"),
            "aucun gestionnaire d'extensions : impossible d'en installer une"
        );
    }

    #[test]
    fn the_extension_collection_is_not_the_extension_manager() {
        // `gnome-shell-extensions` is a *set of extensions* — classic mode, the
        // applications menu and a dozen more — installed whether or not anyone
        // wants them. `extension-manager` is the tool that installs extensions.
        // One word apart, routinely confused, and the wrong one turns up in a
        // desktop as a pile of unwanted features.
        for desktop in Desktop::all() {
            assert!(
                !desktop.packages().contains(&"gnome-shell-extensions"),
                "{} : gnome-shell-extensions est une collection d'extensions, \
                 pas un gestionnaire",
                desktop.label()
            );
        }
        assert!(Desktop::Gnome.packages().contains(&"extension-manager"));
    }

    #[test]
    fn nothing_is_installed_and_removed_in_the_same_run() {
        for desktop in Desktop::all() {
            for name in desktop.removals() {
                assert!(
                    !desktop.packages().contains(name),
                    "{} : {name} est installé puis retiré",
                    desktop.label()
                );
            }
        }
    }
}
