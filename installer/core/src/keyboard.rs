//! Keyboard layouts.
//!
//! Two different things have to be set, and they are not the same list:
//!
//!  * the **console** keymap, from `/usr/share/kbd/keymaps`, written to
//!    `/etc/vconsole.conf` as `KEYMAP=`. This is what applies on a TTY and in
//!    the initramfs.
//!  * the **X11/Wayland** layout, from xkeyboard-config, written to
//!    `/etc/X11/xorg.conf.d/00-keyboard.conf`. This is what the desktop uses.
//!
//! A layout entry carries both, because picking one without the other gives a
//! system whose console and desktop disagree — a French keyboard that types
//! QWERTY on a TTY is a confusing thing to debug.
//!
//! The list is curated rather than exhaustive: `localectl` knows 252 console
//! keymaps and 99 X11 layouts, and offering all of them in a wizard would be
//! worse than offering the ones people actually pick. The search box covers
//! the rest of the need.

use serde::Serialize;

/// A keyboard layout.
///
/// `Serialize` only: the list is sent to the interface, while the config
/// carries just the layout's `id`. Deriving `Deserialize` would need owned
/// strings for `countries`, for no benefit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyboard {
    /// Stable identifier, and what travels in the config.
    pub id: &'static str,
    /// Display name, in the layout's own language — someone who has landed in
    /// the wrong layout recognises their own faster than a translation.
    pub label: &'static str,
    /// X11 layout code, as xkeyboard-config names it.
    pub x11: &'static str,
    /// X11 variant, or empty for the default.
    pub variant: &'static str,
    /// Console keymap, as `/usr/share/kbd/keymaps` names it.
    pub console: &'static str,
    /// ISO 3166 country codes this layout is the default for.
    ///
    /// Used to preselect from the time zone: `zone1970.tab` carries the
    /// countries each zone covers, so a zone is enough to guess the layout.
    pub countries: &'static [&'static str],
}

/// The layouts the installer offers.
pub const KEYBOARDS: &[Keyboard] = &[
    Keyboard { id: "us", label: "English (US)", x11: "us", variant: "", console: "us",
               countries: &["US", "CA", "PH", "PR", "VI", "GU", "AS", "MP"] },
    Keyboard { id: "gb", label: "English (UK)", x11: "gb", variant: "", console: "uk",
               countries: &["GB", "IE", "GI", "IM", "JE", "GG"] },
    Keyboard { id: "fr", label: "Français", x11: "fr", variant: "", console: "fr-latin9",
               countries: &["FR", "MC"] },
    Keyboard { id: "be", label: "Belge", x11: "be", variant: "", console: "be-latin1",
               countries: &["BE"] },
    Keyboard { id: "ch-fr", label: "Suisse romande", x11: "ch", variant: "fr", console: "fr_CH",
               countries: &["CH", "LI"] },
    Keyboard { id: "ch-de", label: "Deutschschweiz", x11: "ch", variant: "de", console: "de_CH-latin1",
               countries: &[] },
    Keyboard { id: "ca-fr", label: "Français (Canada)", x11: "ca", variant: "fr", console: "cf",
               countries: &[] },
    Keyboard { id: "de", label: "Deutsch", x11: "de", variant: "", console: "de-latin1",
               countries: &["DE", "AT"] },
    Keyboard { id: "es", label: "Español", x11: "es", variant: "", console: "es",
               // Puerto Rico is deliberately absent: it uses US-layout
               // keyboards despite the language, so it belongs to `us`.
               countries: &["ES", "MX", "AR", "CO", "CL", "PE", "VE", "EC", "UY", "PY", "BO", "CR",
                            "CU", "DO", "GT", "HN", "NI", "PA", "SV"] },
    Keyboard { id: "latam", label: "Español (Latinoamérica)", x11: "latam", variant: "", console: "la-latin1",
               countries: &[] },
    Keyboard { id: "it", label: "Italiano", x11: "it", variant: "", console: "it",
               countries: &["IT", "SM", "VA"] },
    Keyboard { id: "pt", label: "Português", x11: "pt", variant: "", console: "pt-latin1",
               countries: &["PT"] },
    Keyboard { id: "br", label: "Português (Brasil)", x11: "br", variant: "", console: "br-abnt2",
               countries: &["BR"] },
    Keyboard { id: "nl", label: "Nederlands", x11: "nl", variant: "", console: "nl",
               countries: &["NL"] },
    Keyboard { id: "dk", label: "Dansk", x11: "dk", variant: "", console: "dk",
               countries: &["DK"] },
    Keyboard { id: "no", label: "Norsk", x11: "no", variant: "", console: "no",
               countries: &["NO", "SJ", "BV"] },
    Keyboard { id: "se", label: "Svenska", x11: "se", variant: "", console: "sv-latin1",
               countries: &["SE"] },
    Keyboard { id: "fi", label: "Suomi", x11: "fi", variant: "", console: "fi",
               countries: &["FI", "AX"] },
    Keyboard { id: "is", label: "Íslenska", x11: "is", variant: "", console: "is-latin1",
               countries: &["IS"] },
    Keyboard { id: "pl", label: "Polski", x11: "pl", variant: "", console: "pl",
               countries: &["PL"] },
    Keyboard { id: "cz", label: "Čeština", x11: "cz", variant: "", console: "cz-lat2",
               countries: &["CZ"] },
    Keyboard { id: "sk", label: "Slovenčina", x11: "sk", variant: "", console: "sk-qwertz",
               countries: &["SK"] },
    Keyboard { id: "hu", label: "Magyar", x11: "hu", variant: "", console: "hu",
               countries: &["HU"] },
    Keyboard { id: "ro", label: "Română", x11: "ro", variant: "", console: "ro",
               countries: &["RO", "MD"] },
    Keyboard { id: "bg", label: "Български", x11: "bg", variant: "", console: "bg_pho-utf8",
               countries: &["BG"] },
    Keyboard { id: "gr", label: "Ελληνικά", x11: "gr", variant: "", console: "gr",
               countries: &["GR"] },
    Keyboard { id: "ru", label: "Русский", x11: "ru", variant: "", console: "ru",
               countries: &["RU", "BY", "KZ", "KG", "TJ", "TM", "UZ"] },
    Keyboard { id: "ua", label: "Українська", x11: "ua", variant: "", console: "ua-utf",
               countries: &["UA"] },
    Keyboard { id: "tr", label: "Türkçe", x11: "tr", variant: "", console: "tr_q-latin5",
               countries: &["TR"] },
    Keyboard { id: "il", label: "עברית", x11: "il", variant: "", console: "il",
               countries: &["IL"] },
    Keyboard { id: "ara", label: "العربية", x11: "ara", variant: "", console: "us",
               countries: &["SA", "AE", "EG", "DZ", "MA", "TN", "LY", "JO", "LB", "SY", "IQ", "KW",
                            "QA", "BH", "OM", "YE", "SD"] },
    Keyboard { id: "in", label: "English (India)", x11: "in", variant: "", console: "us",
               countries: &["IN"] },
    Keyboard { id: "jp", label: "日本語", x11: "jp", variant: "", console: "jp106",
               countries: &["JP"] },
    Keyboard { id: "kr", label: "한국어", x11: "kr", variant: "", console: "us",
               countries: &["KR"] },
    Keyboard { id: "cn", label: "中文", x11: "cn", variant: "", console: "us",
               countries: &["CN"] },
    Keyboard { id: "tw", label: "繁體中文", x11: "tw", variant: "", console: "us",
               countries: &["TW"] },
    Keyboard { id: "th", label: "ไทย", x11: "th", variant: "", console: "th",
               countries: &["TH"] },
    Keyboard { id: "vn", label: "Tiếng Việt", x11: "vn", variant: "", console: "us",
               countries: &["VN"] },
    Keyboard { id: "za", label: "English (South Africa)", x11: "za", variant: "", console: "us",
               countries: &["ZA"] },
    Keyboard { id: "ng", label: "English (Nigeria)", x11: "ng", variant: "", console: "us",
               countries: &["NG", "GH", "KE", "TZ", "UG", "ET"] },
];

/// The layout used when nothing matches.
pub const DEFAULT_ID: &str = "us";

pub fn by_id(id: &str) -> Option<&'static Keyboard> {
    KEYBOARDS.iter().find(|keyboard| keyboard.id == id)
}

/// The layout to preselect for a set of countries.
///
/// `countries` comes from the time zone: `zone1970.tab` lists the ISO codes
/// each zone covers, and a zone almost always implies a layout. The first
/// country with a match wins — a zone covering several countries (CET covers
/// a dozen) resolves to whichever is listed first, which is the country the
/// zone is named after in practice.
pub fn for_countries(countries: &[&str]) -> &'static Keyboard {
    for country in countries {
        if let Some(keyboard) = KEYBOARDS
            .iter()
            .find(|keyboard| keyboard.countries.contains(country))
        {
            return keyboard;
        }
    }
    by_id(DEFAULT_ID).expect("le layout par défaut doit exister")
}

/// `/etc/vconsole.conf` for the installed system.
///
/// `lat9w-16` is the Latin-9 console font: it carries the accented letters,
/// the Euro sign and the oe ligature that the Western European layouts need.
/// It is the right choice for all of them, which is why there is no per-layout
/// font table.
pub fn vconsole_conf(keyboard: &Keyboard) -> String {
    format!(
        "# Generated by the noos-os installer.\n\
         KEYMAP={keymap}\n\
         FONT=lat9w-16\n",
        keymap = keyboard.console,
    )
}

/// `/etc/X11/xorg.conf.d/00-keyboard.conf` for the installed system.
///
/// Written for X11, and read by Wayland compositors too — they take the layout
/// from the same configuration when the session does not override it.
pub fn x11_conf(keyboard: &Keyboard) -> String {
    // An empty variant must not be emitted as an empty `XkbVariant=` option:
    // X would reject the whole section.
    let variant_line = if keyboard.variant.is_empty() {
        String::new()
    } else {
        format!("        Option \"XkbVariant\" \"{}\"\n", keyboard.variant)
    };

    format!(
        "# Generated by the noos-os installer.\n\
         #\n\
         # Read by the X server and, through it, by Wayland compositors that do\n\
         # not set their own layout.\n\
         \n\
         Section \"InputClass\"\n\
         \x20       Identifier \"system-keyboard\"\n\
         \x20       MatchIsKeyboard \"on\"\n\
         \x20       Option \"XkbLayout\" \"{layout}\"\n\
         {variant}EndSection\n",
        layout = keyboard.x11,
        variant = variant_line,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique() {
        let unique: HashSet<_> = KEYBOARDS.iter().map(|k| k.id).collect();
        assert_eq!(unique.len(), KEYBOARDS.len(), "identifiant de clavier en double");
    }

    #[test]
    fn the_default_exists() {
        assert!(by_id(DEFAULT_ID).is_some());
    }

    #[test]
    fn every_layout_has_a_console_keymap_and_an_x11_layout() {
        for keyboard in KEYBOARDS {
            assert!(!keyboard.console.is_empty(), "{} sans keymap", keyboard.id);
            assert!(!keyboard.x11.is_empty(), "{} sans disposition X11", keyboard.id);
            assert!(!keyboard.label.is_empty(), "{} sans nom", keyboard.id);
        }
    }

    #[test]
    fn the_french_layout_uses_the_latin9_keymap() {
        // fr-latin1 predates the Euro sign and puts some punctuation
        // elsewhere; latin9 is the modern French console keymap.
        let french = by_id("fr").expect("fr doit exister");
        assert_eq!(french.console, "fr-latin9");
        assert_eq!(french.x11, "fr");
    }

    #[test]
    fn switzerland_gets_the_french_layout_by_default() {
        // zone1970.tab lists CH under a single zone, so the country cannot
        // distinguish the two Swiss layouts; French is the tie-break.
        let swiss = for_countries(&["CH"]);
        assert_eq!(swiss.id, "ch-fr");
    }

    #[test]
    fn preselects_from_a_country() {
        assert_eq!(for_countries(&["FR"]).id, "fr");
        assert_eq!(for_countries(&["DE"]).id, "de");
        assert_eq!(for_countries(&["JP"]).id, "jp");
        assert_eq!(for_countries(&["BR"]).id, "br");
        assert_eq!(for_countries(&["RU"]).id, "ru");
    }

    #[test]
    fn preselects_from_a_zone_covering_several_countries() {
        // A multi-country zone resolves to the first listed match rather than
        // falling back to the default.
        let layout = for_countries(&["CN", "HK"]);
        assert_eq!(layout.id, "cn");
    }

    #[test]
    fn unknown_countries_fall_back_to_the_default() {
        assert_eq!(for_countries(&[]).id, DEFAULT_ID);
        assert_eq!(for_countries(&["ZZ"]).id, DEFAULT_ID);
        assert_eq!(for_countries(&["ZZ", "FR"]).id, "fr");
    }

    #[test]
    fn no_country_is_claimed_by_two_layouts() {
        // An overlap would make the preselection depend on the order of the
        // list, which is exactly the kind of bug nobody notices.
        let mut seen: HashSet<&str> = HashSet::new();
        for keyboard in KEYBOARDS {
            for country in keyboard.countries {
                assert!(
                    seen.insert(country),
                    "{country} est revendiqué par plusieurs dispositions"
                );
            }
        }
    }

    #[test]
    fn vconsole_names_the_chosen_keymap() {
        let conf = vconsole_conf(by_id("fr").unwrap());
        assert!(conf.contains("KEYMAP=fr-latin9"));
        assert!(conf.contains("FONT="));

        let us = vconsole_conf(by_id("us").unwrap());
        assert!(us.contains("KEYMAP=us"));
    }

    #[test]
    fn x11_conf_omits_an_empty_variant() {
        // `XkbVariant ""` is not the same as no variant at all: X rejects the
        // section.
        let plain = x11_conf(by_id("fr").unwrap());
        assert!(plain.contains("XkbLayout\" \"fr\""));
        assert!(
            !plain.contains("XkbVariant"),
            "une variante vide ne doit pas être écrite :\n{plain}"
        );
    }

    #[test]
    fn x11_conf_writes_a_real_variant() {
        let swiss = x11_conf(by_id("ch-de").unwrap());
        assert!(swiss.contains("XkbLayout\" \"ch\""));
        assert!(swiss.contains("XkbVariant\" \"de\""));
    }

    #[test]
    fn x11_conf_is_a_valid_section() {
        for keyboard in KEYBOARDS {
            let conf = x11_conf(keyboard);
            assert_eq!(
                conf.matches("Section \"InputClass\"").count(),
                1,
                "{} : section malformée",
                keyboard.id
            );
            assert_eq!(conf.matches("EndSection").count(), 1);
        }
    }

    #[test]
    fn cjk_layouts_keep_a_usable_console() {
        // Japanese, Korean and Chinese input goes through an IME, not a
        // keymap. The console still needs to type ASCII, so those layouts must
        // not point the console at something that cannot.
        for id in ["kr", "cn", "tw", "vn"] {
            let keyboard = by_id(id).expect("disposition attendue");
            assert_eq!(keyboard.console, "us", "{id} doit garder un clavier console utilisable");
        }
        // Japanese has a real physical-layout keymap, so it uses it.
        assert_eq!(by_id("jp").unwrap().console, "jp106");
    }
}
