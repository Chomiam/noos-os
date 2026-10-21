//! The desktop's appearance: dark, across every toolkit.
//!
//! Four families of application draw their own widgets, and each has its own
//! lever. None of them reads another's, so a dark desktop means setting all
//! four — and each one fails silently on its own.
//!
//! | Toolkit | Lever |
//! |---|---|
//! | GTK 4 / libadwaita | `color-scheme` in the schema override |
//! | GTK 3 | `gtk-theme` in the override, and `/etc/gtk-3.0/settings.ini` |
//! | Qt 5 and 6 | `qt5ct` + `qt6ct`, and their config files |
//! | Electron | none of its own: it reads GTK, so it follows the two above |
//! | Flatpak | the settings portal for GTK 4; a Flathub extension for GTK 3 |
//!
//! Applied through a **schema override** rather than per-user settings, because
//! at installation time no user has ever logged in and their `dconf` database
//! does not exist — and both `dconf` and `gsettings set` want the system bus,
//! the same absence that made `snapper` fail three steps from anything naming
//! it. `glib-compile-schemas` folds the override into `gschemas.compiled`, and a
//! key nobody has written reads back as the overridden default. It holds for
//! every account created later, which a per-user setting would not.

/// Where glib keeps the compiled schema database and its overrides.
///
/// Two things about compiling this directory, both learned by testing it:
///
/// - **Never pass `--strict`.** A stock Arch schemas directory contains files
///   that fail strict compilation for reasons that have nothing to do with us
///   (`org.gnome.GWeather4` declares an enum before defining it), and `--strict`
///   turns that into a failed installation.
/// - The directory has to be **complete**, `*.enums.xml` included. Compiling a
///   hand-copied subset silently discards every schema whose enums live
///   elsewhere — `org.gnome.desktop.interface` among them — and the override
///   then applies to nothing. pacstrap installs the whole thing, so this is only
///   a trap when reproducing the mechanism by hand.
pub const SCHEMAS_DIR: &str = "/usr/share/glib-2.0/schemas";

/// Our override file.
///
/// The `99-` prefix is the convention for "apply last": GNOME ships overrides of
/// its own, and a name that sorted before them could be overwritten by one.
pub const OVERRIDE_FILE: &str = "99-noos.gschema.override";

/// The icon theme, in the package that carries it.
pub const ICON_THEME_PACKAGE: &str = "papirus-icon-theme";

/// The icon theme's name, exactly as its own `index.theme` declares it.
///
/// The `-Dark` variant, not plain `Papirus`: the installer, the Plymouth splash
/// and the rEFInd theme are all built dark, and Papirus draws its light and dark
/// sets differently enough that the wrong one reads as a mistake rather than as
/// a preference. A wrong name here is silent — GTK falls back to `hicolor` and
/// every icon quietly changes shape.
pub const ICON_THEME: &str = "Papirus-Dark";

/// The GTK widget theme.
///
/// Still needed even though GTK 4 dropped the separate dark theme in favour of
/// `color-scheme`: **GTK 3** applications — and every Electron application,
/// which is GTK underneath — look at `gtk-theme` and nothing else.
pub const GTK_THEME: &str = "Adwaita-dark";

/// The package that carries [`GTK_THEME`].
pub const GTK_THEME_PACKAGE: &str = "gnome-themes-extra";

/// The colour scheme hint. GTK 4 and libadwaita read this one only.
pub const COLOR_SCHEME: &str = "prefer-dark";

/// The GTK settings files, which cover what the schema override cannot.
///
/// Worth having as well as the override: GTK reads these itself, before and
/// independently of `gsettings`, so they still apply to an application that
/// never talks to the settings daemon. `gtk-application-prefer-dark-theme` in
/// particular is what Electron asks for.
pub const GTK_SETTINGS_DIRS: &[&str] = &["/etc/gtk-3.0", "/etc/gtk-4.0"];

/// Qt configuration packages.
pub const QT_PACKAGES: &[&str] = &["qt5ct", "qt6ct"];

/// The Qt platform theme, for Qt 5 and Qt 6 at once.
///
/// A colon-separated list, and that is the whole point: one environment variable
/// covers both, where a single name would leave the other family unthemed. It
/// matters here — **VLC is built against Qt 5** on Arch, so Qt 5 is not a
/// historical footnote on this distribution, it is the media player.
pub const QT_PLATFORM_THEME: &str = "qt5ct:qt6ct";

/// Qt's own configuration files, paired with the directory each one lives in.
///
/// The names differ — `qt5ct.conf` and `qt6ct.conf` — and each tool reads only
/// its own. Writing the same file name into both directories, which is the
/// obvious way to loop, leaves one of the two reading nothing at all.
pub const QT_CONF_FILES: &[(&str, &str)] = &[
    ("/etc/xdg/qt5ct", "qt5ct.conf"),
    ("/etc/xdg/qt6ct", "qt6ct.conf"),
];

/// The dark colour scheme, shipped by `qt6ct`.
///
/// Reused rather than written: Qt expresses a scheme as three comma-separated
/// lists of twenty-one `#aarrggbb` values in the order of Qt's `QPalette` roles,
/// and a hand-written one that got the order wrong would produce a plausible,
/// wrongly-coloured application. This file is the package's own.
pub const QT_COLOR_SCHEME: &str = "/usr/share/qt6ct/colors/darker.conf";

/// Directory holding the environment file that Qt reads.
pub const QT_ENVIRONMENT_DIR: &str = "/etc/environment.d";

/// Where the environment variable that Qt reads is written.
///
/// `/etc/environment.d` rather than `/etc/environment`: it is the systemd-user
/// convention, it is parsed per line rather than by a shell, and a file dropped
/// there can be removed on its own.
pub const QT_ENVIRONMENT_FILE: &str = "/etc/environment.d/10-noos-qt.conf";

/// The Flathub extension that brings a GTK 3 theme into the sandbox.
///
/// Flatpak applications cannot see the host's `/usr/share/themes`, so the theme
/// has to exist *inside* the runtime. GTK 4 applications need none of this — they
/// ask the settings portal, which reads the host — but GTK 3 ones do.
///
/// Verified on Flathub rather than assumed: `org.gtk.Gtk3theme.Adwaita-dark`
/// resolves and describes itself as "Dark variant of the Adwaita GTK theme".
///
/// There is **no** equivalent for the icon theme — the Flathub id
/// `org.freedesktop.Platform.Icontheme.Papirus` does not resolve — so sandboxed
/// applications keep the runtime's own icons. Worth knowing before wondering why
/// Papirus shows in one window and not another.
pub const FLATPAK_GTK3_THEME: &str = "org.gtk.Gtk3theme.Adwaita-dark";

/// Renders the arguments that install [`FLATPAK_GTK3_THEME`].
///
/// **`flatpak install --system` does work from a chroot**, contrary to what this
/// module claimed at first. The bus is not the obstacle: the system helper exists
/// for *unprivileged* clients, and the installer runs as root. What fails, when
/// it fails, is the `apply_extra` step, which needs a working bubblewrap sandbox
/// — a different problem with its own answer, and not one to solve by handing out
/// a setuid binary.
pub fn flatpak_install_args() -> Vec<String> {
    vec![
        "install".to_string(),
        "--system".to_string(),
        "--noninteractive".to_string(),
        // The remote's name comes from the file name of the `.flatpakrepo` the
        // installer ships in /etc/flatpak/remotes.d — so `flathub` resolves with
        // no `remote-add` and no network round-trip to set it up.
        "flathub".to_string(),
        FLATPAK_GTK3_THEME.to_string(),
    ]
}


/// Renders the schema override.
pub fn override_file() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         #\n\
         # Read by glib-compile-schemas, which folds it into gschemas.compiled.\n\
         # Editing this file has no effect until that command runs again.\n\
         #\n\
         # `icon-theme` and `gtk-theme` name a theme by the Name= field of its own\n\
         # index.theme.\n\
         \n\
         [org.gnome.desktop.interface]\n\
         icon-theme='{icon}'\n\
         gtk-theme='{gtk}'\n\
         color-scheme='{scheme}'\n",
        icon = ICON_THEME,
        gtk = GTK_THEME,
        scheme = COLOR_SCHEME,
    )
}

/// Renders `/etc/gtk-3.0/settings.ini` and its GTK 4 twin.
pub fn gtk_settings_ini() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         #\n\
         # Read by GTK itself, without going through gsettings — which is what\n\
         # makes it apply to applications that never talk to the settings daemon,\n\
         # Electron among them.\n\
         \n\
         [Settings]\n\
         gtk-theme-name={gtk}\n\
         gtk-icon-theme-name={icon}\n\
         gtk-application-prefer-dark-theme=1\n",
        gtk = GTK_THEME,
        icon = ICON_THEME,
    )
}

/// Renders the `qt5ct` / `qt6ct` configuration.
///
/// The section and key names are not guesswork: `[Appearance]`, `custom_palette`,
/// `color_scheme_path` and `icon_theme` are the strings `qt6ct`'s own binary
/// looks for, read out of it rather than assumed. A wrong section name is
/// ignored in silence.
pub fn qt_conf() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         #\n\
         # Read by qt5ct and qt6ct, which both need the platform theme named in\n\
         # {env}. Both read this same shape: they are the same project.\n\
         \n\
         [Appearance]\n\
         custom_palette=true\n\
         color_scheme_path={scheme}\n\
         icon_theme={icon}\n",
        env = QT_ENVIRONMENT_FILE,
        scheme = QT_COLOR_SCHEME,
        icon = ICON_THEME,
    )
}

/// Renders the environment file that makes Qt read the configuration above.
pub fn qt_environment() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         #\n\
         # Without this, qt5ct and qt6ct are installed and configured and nothing\n\
         # ever asks them for anything: Qt only loads a platform theme when told\n\
         # to.\n\
         \n\
         QT_QPA_PLATFORMTHEME={theme}\n",
        theme = QT_PLATFORM_THEME,
    )
}

// -------------------------------------------- KDE Plasma (WhiteSur Solid) ---

/// Pre-built archive carried on the live medium containing the complete
/// WhiteSur-dark solid theme suite:
///
/// - Plasma Global Theme (`com.github.vinceliuice.WhiteSur-dark`)
/// - Plasma Desktop Theme (`WhiteSur-dark`)
/// - Color Scheme (`WhiteSurDark`)
/// - Aurorae window decorations (`WhiteSur-dark-opaque`)
/// - Kvantum theme (`WhiteSur-opaqueDark`)
/// - SDDM login theme (`WhiteSur-dark`, Plasma 6.8+ QML)
/// - Icon & Cursor themes (`WhiteSur-dark`, `WhiteSur`, `WhiteSur-cursors`)
/// - GTK 3 / GTK 4 themes (`WhiteSur-Dark-solid`) and libadwaita overrides in
///   `/etc/skel/.config/gtk-4.0`
pub const WHITESUR_ARCHIVE_PATH: &str =
    "/usr/share/noos/whitesur-kde/whitesur-liquid.tar.zst";

pub const PLASMA_LOOK_AND_FEEL: &str = "com.github.vinceliuice.WhiteSur-dark";
pub const PLASMA_DESKTOP_THEME: &str = "WhiteSur-dark";
pub const PLASMA_COLOR_SCHEME: &str = "WhiteSurDark";
pub const PLASMA_AURORAE_THEME: &str = "__aurorae__svg__WhiteSur-dark-opaque";
pub const PLASMA_KVANTUM_THEME: &str = "WhiteSur-opaqueDark";
pub const PLASMA_ICON_THEME: &str = "WhiteSur-dark";
pub const PLASMA_CURSOR_THEME: &str = "WhiteSur-cursors";
pub const PLASMA_GTK_THEME: &str = "WhiteSur-Dark-solid";
pub const PLASMA_SDDM_THEME: &str = "WhiteSur-dark";
pub const PLASMA_DEFAULT_WALLPAPER: &str =
    "/usr/share/wallpapers/noos/contents/images/2752x1536.jpg";

/// Packages required on the target to render the WhiteSur Liquid theme on KDE.
pub const PLASMA_THEME_PACKAGES: &[&str] = &["kvantum", "kvantum-qt5"];

/// System-wide and skeleton configuration files written for KDE Plasma so the
/// WhiteSur Liquid Dark theme is active on first login without running a live
/// D-Bus session during installation.
pub fn plasma_kdeglobals() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [ColorEffects:Disabled]\n\
         Color=56,56,56\n\
         ColorAmount=0\n\
         ColorEffect=0\n\
         ContrastAmount=0.65\n\
         ContrastEffect=1\n\
         IntensityAmount=0.1\n\
         IntensityEffect=2\n\
         \n\
         [ColorEffects:Inactive]\n\
         ChangeSelectionColor=true\n\
         Color=112,111,110\n\
         ColorAmount=0.025\n\
         ColorEffect=2\n\
         ContrastAmount=0.1\n\
         ContrastEffect=2\n\
         Enable=false\n\
         IntensityAmount=0\n\
         IntensityEffect=0\n\
         \n\
         [Colors:Button]\n\
         BackgroundAlternate=70,70,70\n\
         BackgroundNormal=90,90,90\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=61,174,233\n\
         ForegroundInactive=170,170,170\n\
         ForegroundLink=41,128,185\n\
         ForegroundNegative=218,68,83\n\
         ForegroundNeutral=246,116,0\n\
         ForegroundNormal=255,255,255\n\
         ForegroundPositive=39,174,96\n\
         ForegroundVisited=127,140,141\n\
         \n\
         [Colors:Complementary]\n\
         BackgroundAlternate=66,66,66\n\
         BackgroundNormal=51,51,51\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=246,116,0\n\
         ForegroundInactive=160,160,160\n\
         ForegroundLink=49,91,239\n\
         ForegroundNegative=237,21,21\n\
         ForegroundNeutral=201,206,59\n\
         ForegroundNormal=252,252,252\n\
         ForegroundPositive=17,209,22\n\
         ForegroundVisited=49,91,239\n\
         \n\
         [Colors:Header]\n\
         BackgroundAlternate=66,66,66\n\
         BackgroundNormal=54,54,54\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=0,122,255\n\
         ForegroundInactive=150,150,150\n\
         ForegroundLink=49,91,239\n\
         ForegroundNegative=237,21,21\n\
         ForegroundNeutral=201,206,59\n\
         ForegroundNormal=252,252,252\n\
         ForegroundPositive=17,209,22\n\
         ForegroundVisited=49,91,239\n\
         \n\
         [Colors:Header][Inactive]\n\
         BackgroundAlternate=66,66,66\n\
         BackgroundNormal=45,45,45\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=0,122,255,150\n\
         ForegroundInactive=150,150,150,150\n\
         ForegroundLink=49,91,239\n\
         ForegroundNegative=237,21,21\n\
         ForegroundNeutral=201,206,59\n\
         ForegroundNormal=255,255,255,150\n\
         ForegroundPositive=17,209,22\n\
         ForegroundVisited=49,91,239\n\
         \n\
         [Colors:Selection]\n\
         BackgroundAlternate=30,144,255\n\
         BackgroundNormal=0,122,255\n\
         DecorationFocus=255,255,255\n\
         DecorationHover=255,255,255\n\
         ForegroundActive=255,255,255\n\
         ForegroundInactive=130,156,239\n\
         ForegroundLink=253,188,75\n\
         ForegroundNegative=218,68,83\n\
         ForegroundNeutral=246,116,0\n\
         ForegroundNormal=255,255,255\n\
         ForegroundPositive=39,174,96\n\
         ForegroundVisited=128,152,239\n\
         \n\
         [Colors:Tooltip]\n\
         BackgroundAlternate=70,70,70\n\
         BackgroundNormal=45,45,45\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=49,91,239\n\
         ForegroundInactive=170,170,170\n\
         ForegroundLink=49,91,239\n\
         ForegroundNegative=218,68,83\n\
         ForegroundNeutral=246,116,0\n\
         ForegroundNormal=255,255,255\n\
         ForegroundPositive=39,174,96\n\
         ForegroundVisited=127,140,141\n\
         \n\
         [Colors:View]\n\
         BackgroundAlternate=40,40,40\n\
         BackgroundNormal=30,30,30\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=49,91,239\n\
         ForegroundInactive=150,150,150\n\
         ForegroundLink=49,91,239\n\
         ForegroundNegative=218,68,83\n\
         ForegroundNeutral=246,116,0\n\
         ForegroundNormal=255,255,255\n\
         ForegroundPositive=39,174,96\n\
         ForegroundVisited=120,120,120\n\
         \n\
         [Colors:Window]\n\
         BackgroundAlternate=60,60,60\n\
         BackgroundNormal=50,50,50\n\
         DecorationFocus=49,91,239\n\
         DecorationHover=49,91,239\n\
         ForegroundActive=49,91,239\n\
         ForegroundInactive=150,150,150\n\
         ForegroundLink=49,91,239\n\
         ForegroundNegative=218,68,83\n\
         ForegroundNeutral=246,116,0\n\
         ForegroundNormal=255,255,255\n\
         ForegroundPositive=39,174,96\n\
         ForegroundVisited=120,120,120\n\
         \n\
         [General]\n\
         ColorScheme={color}\n\
         Name={color}\n\
         TerminalApplication=ghostty\n\
         TerminalService=com.mitchellh.ghostty.desktop\n\
         shadeSortColumn=true\n\
         \n\
         [Icons]\n\
         Theme={icon}\n\
         \n\
         [KDE]\n\
         LookAndFeelPackage={lnf}\n\
         contrast=4\n\
         widgetStyle=kvantum-dark\n\
         \n\
         [WM]\n\
         activeBackground=51,51,51\n\
         activeBlend=171,171,171\n\
         activeForeground=252,252,252\n\
         inactiveBackground=66,66,66\n\
         inactiveBlend=85,85,85\n\
         inactiveForeground=170,170,170\n",
        color = PLASMA_COLOR_SCHEME,
        icon = PLASMA_ICON_THEME,
        lnf = PLASMA_LOOK_AND_FEEL,
    )
}

pub fn plasma_plasmarc() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [Theme]\n\
         name={theme}\n",
        theme = PLASMA_DESKTOP_THEME,
    )
}

pub fn plasma_kcminputrc() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [Keyboard]\n\
         NumLock=0\n\
         \n\
         [Mouse]\n\
         cursorTheme={cursor}\n",
        cursor = PLASMA_CURSOR_THEME,
    )
}

pub fn plasma_ksplashrc() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [KSplash]\n\
         Theme={lnf}\n",
        lnf = PLASMA_LOOK_AND_FEEL,
    )
}

/// KRunner configuration so the `Alt+Space` launcher opens centered on screen
/// (`FreeFloating=true`, Spotlight style) instead of docked to the top edge.
pub fn plasma_krunnerrc() -> String {
    "# Generated by the noos-os installer.\n\
     [General]\n\
     FreeFloating=true\n"
        .to_string()
}

/// KWin configuration enabling the WhiteSurLiquid-dark Aurorae window
/// decoration, macOS left-hand traffic-light buttons (`XIA`), and the KWin
/// blur + translucency compositor effects required by the Liquid Glass style.
pub fn plasma_kwinrc() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [Effect-blur]\n\
         BlurStrength=12\n\
         NoiseStrength=2\n\
         \n\
         [Plugins]\n\
         blurEnabled=true\n\
         translucencyEnabled=true\n\
         \n\
         [org.kde.kdecoration2]\n\
         ButtonsOnLeft=XIA\n\
         ButtonsOnRight=\n\
         library=org.kde.kwin.aurorae\n\
         theme={aurorae}\n",
        aurorae = PLASMA_AURORAE_THEME,
    )
}

pub fn plasma_kvantum_kvconfig() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [General]\n\
         theme={kvantum}\n",
        kvantum = PLASMA_KVANTUM_THEME,
    )
}

/// Renders `kscreenlockerrc` so the KDE lock screen uses the default noos-os
/// wallpaper (`/usr/share/wallpapers/noos/contents/images/2752x1536.jpg`).
pub fn plasma_kscreenlockerrc() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [Greeter][Wallpaper][org.kde.image][General]\n\
         Image=file://{wallpaper}\n\
         PreviewImage=file://{wallpaper}\n",
        wallpaper = PLASMA_DEFAULT_WALLPAPER,
    )
}

/// Renders `/etc/sddm.conf.d/kde_settings.conf` so the SDDM login screen uses
/// the WhiteSur-dark theme, WhiteSur-cursors, and activates NumLock by default.
pub fn plasma_sddm_conf() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [General]\n\
         Numlock=on\n\
         \n\
         [Theme]\n\
         Current={sddm}\n\
         CursorTheme={cursor}\n",
        sddm = PLASMA_SDDM_THEME,
        cursor = PLASMA_CURSOR_THEME,
    )
}

/// Renders the GSettings schema override for KDE Plasma so GTK 3, GTK 4 and
/// Electron applications match the WhiteSur-Dark theme and icons.
pub fn plasma_override_file() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         #\n\
         # Read by glib-compile-schemas, which folds it into gschemas.compiled.\n\
         \n\
         [org.gnome.desktop.interface]\n\
         icon-theme='{icon}'\n\
         gtk-theme='{gtk}'\n\
         cursor-theme='{cursor}'\n\
         color-scheme='{scheme}'\n",
        icon = PLASMA_ICON_THEME,
        gtk = PLASMA_GTK_THEME,
        cursor = PLASMA_CURSOR_THEME,
        scheme = COLOR_SCHEME,
    )
}

/// Renders `/etc/gtk-3.0/settings.ini` and `/etc/gtk-4.0/settings.ini` for
/// KDE Plasma.
pub fn plasma_gtk_settings_ini() -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         [Settings]\n\
         gtk-theme-name={gtk}\n\
         gtk-icon-theme-name={icon}\n\
         gtk-cursor-theme-name={cursor}\n\
         gtk-cursor-theme-size=24\n\
         gtk-application-prefer-dark-theme=1\n\
         gtk-button-images=1\n\
         gtk-menu-images=1\n",
        gtk = PLASMA_GTK_THEME,
        icon = PLASMA_ICON_THEME,
        cursor = PLASMA_CURSOR_THEME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_theme_is_the_dark_variant_of_papirus() {
        // Verified against the package's own `usr/share/icons/Papirus-Dark/index.theme`,
        // whose `Name=` is this string and nothing else.
        assert_eq!(ICON_THEME, "Papirus-Dark");
        assert!(ICON_THEME.starts_with("Papirus"));
    }

    #[test]
    fn the_gtk_theme_exists_in_the_package_that_is_installed() {
        // `gnome-themes-extra` ships `usr/share/themes/Adwaita-dark` and
        // `usr/share/themes/Adwaita` — checked on the package itself.
        assert_eq!(GTK_THEME, "Adwaita-dark");
        assert_eq!(GTK_THEME_PACKAGE, "gnome-themes-extra");
    }

    #[test]
    fn the_override_targets_the_interface_schema() {
        let conf = override_file();
        assert!(conf.contains("[org.gnome.desktop.interface]"));
        assert!(conf.contains("icon-theme='Papirus-Dark'"));
        assert!(conf.contains("gtk-theme='Adwaita-dark'"));
        assert!(conf.contains("color-scheme='prefer-dark'"));
    }

    #[test]
    fn the_values_are_quoted() {
        // A gschema override parses its values as GVariant. An unquoted string
        // is not valid there, and glib-compile-schemas reports the whole file
        // rather than the line — or ignores it entirely, depending on version.
        for line in override_file().lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key == "icon-theme" || key == "gtk-theme" || key == "color-scheme" {
                assert!(
                    value.starts_with('\'') && value.ends_with('\''),
                    "valeur non quotée : {line}"
                );
            }
        }
    }

    #[test]
    fn the_file_says_it_has_to_be_compiled() {
        // The one way this silently does nothing is being read as a plain file:
        // gsettings only sees the compiled database.
        assert!(override_file().contains("glib-compile-schemas"));
    }

    #[test]
    fn the_override_sorts_after_gnomes_own() {
        // GNOME ships overrides of its own. glib merges them, and a file sorting
        // before theirs could be the one that loses.
        assert!(OVERRIDE_FILE.starts_with("99-"));
        assert!(OVERRIDE_FILE.ends_with(".gschema.override"));
    }

    #[test]
    fn gtk_settings_carry_both_the_theme_and_the_dark_flag() {
        // The flag is what Electron reads: it has no theming of its own and asks
        // GTK whether the desktop prefers dark.
        let ini = gtk_settings_ini();
        assert!(ini.contains("[Settings]"));
        assert!(ini.contains("gtk-application-prefer-dark-theme=1"));
        assert!(ini.contains("gtk-theme-name=Adwaita-dark"));
        assert!(ini.contains("gtk-icon-theme-name=Papirus-Dark"));
    }

    #[test]
    fn each_qt_tool_gets_a_file_named_after_itself() {
        // The obvious loop writes one name into both directories and leaves
        // qt5ct — which is what VLC uses — reading nothing.
        let names: Vec<&str> = QT_CONF_FILES.iter().map(|(_, n)| *n).collect();
        assert_eq!(names, vec!["qt5ct.conf", "qt6ct.conf"]);
        for (dir, name) in QT_CONF_FILES {
            assert!(dir.ends_with(name.trim_end_matches(".conf")));
        }
    }

    #[test]
    fn qt_covers_both_toolkit_versions() {
        // VLC is Qt 5 on Arch, so a Qt 6-only setting would leave the media
        // player light. The colon is what makes one variable serve both.
        assert!(QT_PLATFORM_THEME.contains("qt5ct"));
        assert!(QT_PLATFORM_THEME.contains("qt6ct"));
        assert!(QT_PLATFORM_THEME.contains(':'));
    }

    #[test]
    fn the_qt_config_uses_the_keys_qt6ct_looks_for() {
        // Read out of qt6ct's binary with `strings`: the section is `Appearance`
        // and the keys are `color_scheme_path`, `custom_palette` and
        // `icon_theme`.
        let conf = qt_conf();
        assert!(conf.contains("[Appearance]"));
        assert!(conf.contains("custom_palette=true"));
        assert!(conf.contains(&format!("color_scheme_path={QT_COLOR_SCHEME}")));
        assert!(conf.contains(&format!("icon_theme={ICON_THEME}")));
    }

    #[test]
    fn the_qt_environment_names_the_platform_theme() {
        assert!(qt_environment().contains(&format!("QT_QPA_PLATFORMTHEME={QT_PLATFORM_THEME}")));
    }

    #[test]
    fn the_colour_scheme_comes_from_a_package_not_from_us() {
        // A hand-made Qt palette is twenty-one values in QPalette order; getting
        // the order wrong gives a plausible, wrongly-coloured application.
        assert!(QT_COLOR_SCHEME.starts_with("/usr/share/qt6ct/colors/"));
        assert!(QT_COLOR_SCHEME.ends_with(".conf"));
    }

    #[test]
    fn the_flatpak_install_names_the_extension_and_the_remote() {
        let args = flatpak_install_args();
        assert!(args.contains(&"--system".to_string()));
        assert!(args.contains(&"flathub".to_string()));
        assert!(args.contains(&FLATPAK_GTK3_THEME.to_string()));
        // Not an app: it is the GTK 3 theme runtime extension, so it is named
        // `org.gtk.Gtk3theme.*` rather than anything recognisable as an app.
        assert!(FLATPAK_GTK3_THEME.starts_with("org.gtk.Gtk3theme."));
    }

    #[test]
    fn the_theme_package_matches_the_theme_name() {
        // The package provides `Papirus`, `Papirus-Dark` and `Papirus-Light`;
        // naming a variant the package does not carry is the silent case.
        assert_eq!(ICON_THEME_PACKAGE, "papirus-icon-theme");
    }

    #[test]
    fn plasma_whitesur_liquid_configs_are_consistent() {
        let globals = plasma_kdeglobals();
        assert!(globals.contains("LookAndFeelPackage=com.github.vinceliuice.WhiteSur-dark"));
        assert!(globals.contains("ColorScheme=WhiteSurDark"));
        assert!(globals.contains("Theme=WhiteSur-dark"));
        assert!(globals.contains("widgetStyle=kvantum-dark"));

        let kwin = plasma_kwinrc();
        assert!(kwin.contains("blurEnabled=true"));
        assert!(kwin.contains("translucencyEnabled=true"));
        assert!(kwin.contains("ButtonsOnLeft=XIA"));
        assert!(kwin.contains("theme=__aurorae__svg__WhiteSur-dark-opaque"));

        let sddm = plasma_sddm_conf();
        assert!(sddm.contains("[General]"));
        assert!(sddm.contains("Numlock=on"));
        assert!(sddm.contains("Current=WhiteSur-dark"));
        assert!(sddm.contains("CursorTheme=WhiteSur-cursors"));

        let kcminput = plasma_kcminputrc();
        assert!(kcminput.contains("[Keyboard]"));
        assert!(kcminput.contains("NumLock=0"));
        assert!(kcminput.contains("cursorTheme=WhiteSur-cursors"));

        let kvantum = plasma_kvantum_kvconfig();
        assert!(kvantum.contains("theme=WhiteSur-opaqueDark"));

        let krunner = plasma_krunnerrc();
        assert!(krunner.contains("[General]"));
        assert!(krunner.contains("FreeFloating=true"));

        let lockscreen = plasma_kscreenlockerrc();
        assert!(lockscreen.contains("[Greeter][Wallpaper][org.kde.image][General]"));
        assert!(lockscreen.contains(PLASMA_DEFAULT_WALLPAPER));

        let gschema = plasma_override_file();
        assert!(gschema.contains("gtk-theme='WhiteSur-Dark-solid'"));
        assert!(gschema.contains("icon-theme='WhiteSur-dark'"));
        assert!(gschema.contains("cursor-theme='WhiteSur-cursors'"));
        assert!(gschema.contains("color-scheme='prefer-dark'"));
    }
}
