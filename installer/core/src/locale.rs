//! `/etc/locale.gen` and the check that the locale it names really exists.
//!
//! Separate from [`crate::config`], which owns *which* locale the user picked:
//! this module owns the format of the file that turns that choice into
//! something glibc can use.

/// Uncomments the line that generates `wanted`, leaving every other line
/// byte-for-byte as it was.
///
/// The match is on the **first field** of the line, not on the whole line.
/// A line of `locale.gen` carries two fields — the locale and its charset:
///
/// ```text
/// #fr_FR.UTF-8 UTF-8
/// #fr_FR ISO-8859-1
/// ```
///
/// Comparing the whole line to `fr_FR.UTF-8` therefore never matched anything.
/// No line was enabled, `locale-gen` had nothing to generate, and it **exited
/// 0** — so the install completed with `LANG=fr_FR.UTF-8` in `/etc/locale.conf`
/// pointing at a locale that was never built. Every program fell back to C, and
/// the symptom reads as a translation problem (a half-English desktop, an
/// account shown as "Language: Unspecified [ANSI_X3.4-1968]") rather than as a
/// missing file.
///
/// Matching the first field also keeps `fr_FR` (ISO-8859-1) from being mistaken
/// for `fr_FR.UTF-8`: they are different locales and enabling the wrong one is
/// just as silent.
pub fn enable(locale_gen: &str, wanted: &str) -> String {
    let mut patched = locale_gen
        .lines()
        .map(|line| {
            let body = line.trim_start_matches('#').trim();
            match body.split_whitespace().next() {
                Some(name) if name == wanted => body.to_string(),
                _ => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // `lines()` drops the final newline, and a config file that loses one is a
    // gratuitous diff every time it is rewritten.
    if locale_gen.ends_with('\n') {
        patched.push('\n');
    }
    patched
}

/// Whether the output of `locale -a` lists `wanted`.
///
/// `locale -a` prints its own normalised spelling — `fr_FR.utf8`, not
/// `fr_FR.UTF-8` — so both sides are folded (lowercase, separators removed)
/// before comparing. Checking the *effect* rather than trusting that
/// `locale-gen` did something is the point: it reports success either way.
pub fn available(locale_a: &str, wanted: &str) -> bool {
    fn fold(name: &str) -> String {
        name.to_lowercase().replace(['-', '_'], "").replace('.', "")
    }
    let wanted = fold(wanted);
    !wanted.is_empty() && locale_a.lines().any(|line| fold(line.trim()) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Locale;

    /// A slice of the real `locale.gen`, comments and two-field lines included.
    const SAMPLE: &str = "\
# Configuration file for locale-gen
#
#en_US.UTF-8 UTF-8
#en_US ISO-8859-1
#fr_FR.UTF-8 UTF-8
#fr_FR ISO-8859-1
#fr_FR@euro ISO-8859-15
#de_DE.UTF-8 UTF-8
";

    fn enabled_lines(out: &str) -> Vec<&str> {
        out.lines().filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty()).collect()
    }

    #[test]
    fn enables_the_requested_locale() {
        // The regression: a two-field line compared as a whole never matched,
        // so this used to come back empty and the locale was never built.
        let out = enable(SAMPLE, Locale::Fr.glibc_name());
        assert_eq!(enabled_lines(&out), vec!["fr_FR.UTF-8 UTF-8"]);
    }

    #[test]
    fn does_not_enable_a_locale_that_merely_starts_the_same() {
        // `fr_FR` in ISO-8859-1 is a different locale from `fr_FR.UTF-8`.
        // Matching a prefix instead of the whole first field would enable both.
        let out = enable(SAMPLE, Locale::Fr.glibc_name());
        assert!(!out.contains("\nfr_FR ISO-8859-1"), "la locale non-UTF-8 a été activée :\n{out}");
        assert!(!out.contains("\nfr_FR@euro"), "la variante @euro a été activée :\n{out}");
    }

    #[test]
    fn leaves_every_other_line_untouched() {
        let out = enable(SAMPLE, Locale::Fr.glibc_name());
        for line in SAMPLE.lines() {
            if line.starts_with("#fr_FR.UTF-8") {
                continue;
            }
            assert!(out.contains(line), "ligne modifiée : {line:?}\n{out}");
        }
    }

    #[test]
    fn it_is_idempotent() {
        let once = enable(SAMPLE, Locale::Fr.glibc_name());
        assert_eq!(enable(&once, Locale::Fr.glibc_name()), once);
    }

    #[test]
    fn it_keeps_the_trailing_newline() {
        assert!(enable(SAMPLE, Locale::Fr.glibc_name()).ends_with('\n'));
    }

    #[test]
    fn it_handles_a_file_that_is_already_enabled() {
        let already = SAMPLE.replace("#fr_FR.UTF-8", "fr_FR.UTF-8");
        assert_eq!(enabled_lines(&enable(&already, Locale::Fr.glibc_name())), vec!["fr_FR.UTF-8 UTF-8"]);
    }

    #[test]
    fn both_choices_are_covered() {
        for locale in [Locale::Fr, Locale::En] {
            let out = enable(SAMPLE, locale.glibc_name());
            assert_eq!(
                enabled_lines(&out),
                vec![format!("{} UTF-8", locale.glibc_name())],
                "{locale:?} n'a pas été activée"
            );
        }
    }

    #[test]
    fn locale_a_is_read_in_its_own_spelling() {
        // What `locale -a` actually prints on an Arch system.
        const LIST: &str = "\
C
C.utf8
en_US.utf8
fr_FR.utf8
fr_FR@euro
POSIX
";
        assert!(available(LIST, "fr_FR.UTF-8"));
        assert!(available(LIST, "en_US.UTF-8"));
    }

    #[test]
    fn a_missing_locale_is_not_accepted() {
        // The ISO-8859-1 variant being present must not satisfy UTF-8.
        const LIST: &str = "C\nfr_FR\nfr_FR@euro\nPOSIX\n";
        assert!(!available(LIST, "fr_FR.UTF-8"), "fr_FR (ISO-8859-1) a été prise pour fr_FR.UTF-8");
        assert!(!available(LIST, "de_DE.UTF-8"));
    }

    #[test]
    fn an_empty_list_matches_nothing() {
        assert!(!available("", "fr_FR.UTF-8"));
    }
}
