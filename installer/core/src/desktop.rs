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
static GNOME: DesktopProfile = DesktopProfile {
    packages: &[
        "gnome",
        "gnome-tweaks",
        "gdm",
        "xdg-desktop-portal-gnome",
    ],
    display_manager: DisplayManager::Gdm,
    greeter_session: None,
};

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
}
