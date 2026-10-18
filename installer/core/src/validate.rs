//! Input validation for everything the user types.
//!
//! Kept free of any filesystem or process access so it can be unit-tested
//! without root — which matters, because the rest of the engine cannot be.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use zxcvbn::Score;

/// POSIX-portable account names: lowercase, must not start with a digit or a
/// dash, and `useradd` additionally rejects anything outside this set.
static USERNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z_][a-z0-9_-]*$").expect("valid regex"));

/// RFC 1123 host label: alphanumeric ends, hyphens inside, 63 chars max.
static HOSTNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$").expect("valid regex"));

/// Loose shape check for an IANA zone. Existence is verified separately
/// against `/usr/share/zoneinfo` when we actually set it.
static TIMEZONE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_+-]*(/[A-Za-z0-9_+-]+)*$").expect("valid regex"));

pub const USERNAME_MAX_LEN: usize = 32;

/// Names that already exist as accounts or groups on a stock Arch install, or
/// that would collide with one. `useradd` would refuse most of these anyway,
/// but failing early gives a much better message.
const RESERVED_USERNAMES: &[&str] = &[
    "root", "bin", "daemon", "mail", "ftp", "http", "nobody", "adm", "lp", "sync", "shutdown",
    "halt", "operator", "games", "news", "uucp", "proxy", "backup", "list", "irc", "man", "sys",
    "tty", "disk", "kmem", "wheel", "utmp", "locate", "rfkill", "users", "audio", "video",
    "optical", "storage", "power", "input", "kvm", "render", "dbus", "polkitd", "rtkit", "avahi",
    "colord", "cups", "geoclue", "git", "tss", "usbmux", "uuidd", "sddm", "lightdm", "gdm",
    "greeter", "admin", "administrator", "guest", "test", "user", "mysql", "postgres", "postfix",
    "nginx", "apache", "systemd", "systemd-network", "systemd-resolve", "systemd-timesync",
    "systemd-coredump", "systemd-oom", "systemd-journal-gateway", "systemd-journal-remote",
    "systemd-journal-upload", "nvidia-persistenced", "gnome-remote-desktop",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsernameError {
    Empty,
    TooLong(usize),
    IllegalChars,
    Reserved(String),
}

impl fmt::Display for UsernameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsernameError::Empty => write!(f, "le nom d'utilisateur est vide"),
            UsernameError::TooLong(n) => write!(
                f,
                "le nom d'utilisateur fait {n} caractères, maximum {USERNAME_MAX_LEN}"
            ),
            UsernameError::IllegalChars => write!(
                f,
                "caractères interdits : seuls les minuscules non accentuées, les chiffres, \
                 le tiret et le souligné sont acceptés, et le premier caractère doit être une \
                 lettre ou un souligné"
            ),
            UsernameError::Reserved(name) => {
                write!(f, "« {name} » est un nom réservé par le système")
            }
        }
    }
}

pub fn username(name: &str) -> std::result::Result<(), UsernameError> {
    if name.is_empty() {
        return Err(UsernameError::Empty);
    }
    // Count characters, not bytes: a user pasting "josé" should get the
    // "illegal characters" message, not a confusing length error.
    let len = name.chars().count();
    if len > USERNAME_MAX_LEN {
        return Err(UsernameError::TooLong(len));
    }
    if !USERNAME_RE.is_match(name) {
        return Err(UsernameError::IllegalChars);
    }
    if RESERVED_USERNAMES.contains(&name) {
        return Err(UsernameError::Reserved(name.to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostnameError {
    Empty,
    TooLong(usize),
    IllegalChars,
}

impl fmt::Display for HostnameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostnameError::Empty => write!(f, "le nom de machine est vide"),
            HostnameError::TooLong(n) => {
                write!(f, "le nom de machine fait {n} caractères, maximum 63")
            }
            HostnameError::IllegalChars => write!(
                f,
                "nom de machine invalide : lettres, chiffres et tirets uniquement, \
                 sans tiret au début ni à la fin"
            ),
        }
    }
}

pub fn hostname(name: &str) -> std::result::Result<(), HostnameError> {
    if name.is_empty() {
        return Err(HostnameError::Empty);
    }
    let len = name.chars().count();
    if len > 63 {
        return Err(HostnameError::TooLong(len));
    }
    if !HOSTNAME_RE.is_match(name) {
        return Err(HostnameError::IllegalChars);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimezoneError {
    Empty,
    Shape,
    /// The zone is well-formed but has no file under `/usr/share/zoneinfo`.
    Unknown(String),
}

impl fmt::Display for TimezoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimezoneError::Empty => write!(f, "aucun fuseau horaire sélectionné"),
            TimezoneError::Shape => write!(f, "fuseau horaire mal formé"),
            TimezoneError::Unknown(tz) => write!(f, "fuseau horaire inconnu : {tz}"),
        }
    }
}

/// Shape-only check: no filesystem access, so it is unit-testable anywhere.
pub fn timezone_shape(tz: &str) -> std::result::Result<(), TimezoneError> {
    if tz.is_empty() {
        return Err(TimezoneError::Empty);
    }
    if !TIMEZONE_RE.is_match(tz) || tz.contains("..") {
        return Err(TimezoneError::Shape);
    }
    Ok(())
}

/// Shape check plus confirmation that `tzdata` actually ships this zone.
pub fn timezone(tz: &str) -> std::result::Result<(), TimezoneError> {
    timezone_shape(tz)?;
    let path = std::path::Path::new("/usr/share/zoneinfo").join(tz);
    if path.is_file() {
        Ok(())
    } else {
        Err(TimezoneError::Unknown(tz.to_string()))
    }
}

/// zxcvbn score, 0 (trivially guessable) to 4 (very unguessable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score0To4(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrengthLabel {
    VeryWeak,
    Weak,
    Fair,
    Strong,
    VeryStrong,
}

impl StrengthLabel {
    pub fn from_score(score: Score0To4) -> Self {
        match score.0 {
            0 => StrengthLabel::VeryWeak,
            1 => StrengthLabel::Weak,
            2 => StrengthLabel::Fair,
            3 => StrengthLabel::Strong,
            _ => StrengthLabel::VeryStrong,
        }
    }
}

/// Result of evaluating a password, shaped for direct display in the UI meter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordStrength {
    pub score: Score0To4,
    pub label: StrengthLabel,
    /// Why the password is weak, when zxcvbn has something specific to say.
    pub warning: Option<String>,
    /// Concrete things the user could do to improve it.
    pub suggestions: Vec<String>,
}

/// Minimum score when the "require a strong password" box is ticked.
pub const MIN_STRONG_SCORE: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    Empty,
    TooWeak(Score0To4),
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasswordError::Empty => write!(f, "le mot de passe est vide"),
            PasswordError::TooWeak(s) => write!(
                f,
                "mot de passe trop faible (niveau {}/4, minimum {MIN_STRONG_SCORE} requis)",
                s.0
            ),
        }
    }
}

fn to_score(score: Score) -> Score0To4 {
    // `Score` is #[non_exhaustive] upstream, so the wildcard is required —
    // it also means a future zxcvbn variant lands on the safest value.
    let raw = match score {
        Score::Zero => 0,
        Score::One => 1,
        Score::Two => 2,
        Score::Three => 3,
        _ => 4,
    };
    Score0To4(raw)
}

/// Evaluates a password and returns everything the meter needs.
///
/// `user_inputs` should carry the username and full name: zxcvbn penalises
/// passwords built out of them, which is exactly the mistake people make.
pub fn password_strength(password: &str, user_inputs: &[&str]) -> PasswordStrength {
    let entropy = zxcvbn::zxcvbn(password, user_inputs);
    let score = to_score(entropy.score());

    let (warning, suggestions) = match entropy.feedback() {
        Some(feedback) => (
            // `warning()` yields a `Warning` enum, not a &str.
            feedback.warning().map(|w| w.to_string()),
            feedback.suggestions().iter().map(|s| s.to_string()).collect(),
        ),
        None => (None, Vec::new()),
    };

    PasswordStrength {
        score,
        label: StrengthLabel::from_score(score),
        warning,
        suggestions,
    }
}

/// Enforces the installer's own policy.
///
/// Note this deliberately does *not* rely on the target system's
/// `pam_pwquality`: the accounts are created with `chpasswd`, which writes the
/// hash straight into `/etc/shadow` and never consults PAM.
pub fn password(
    password: &str,
    user_inputs: &[&str],
    require_strong: bool,
) -> std::result::Result<PasswordStrength, PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }

    let strength = password_strength(password, user_inputs);
    if require_strong && strength.score.0 < MIN_STRONG_SCORE {
        return Err(PasswordError::TooWeak(strength.score));
    }
    Ok(strength)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_usernames() {
        for name in ["chomiam", "j_doe", "_svc", "a", "user123", "a-b-c"] {
            assert!(username(name).is_ok(), "{name} devrait être accepté");
        }
    }

    #[test]
    fn rejects_malformed_usernames() {
        assert_eq!(username(""), Err(UsernameError::Empty));
        assert_eq!(username("Chomiam"), Err(UsernameError::IllegalChars));
        assert_eq!(username("1abc"), Err(UsernameError::IllegalChars));
        assert_eq!(username("-abc"), Err(UsernameError::IllegalChars));
        assert_eq!(username("jean dupont"), Err(UsernameError::IllegalChars));
        assert_eq!(username("josé"), Err(UsernameError::IllegalChars));
        assert_eq!(username("a.b"), Err(UsernameError::IllegalChars));
        assert_eq!(username("user@host"), Err(UsernameError::IllegalChars));
    }

    #[test]
    fn enforces_username_length() {
        let ok = "a".repeat(USERNAME_MAX_LEN);
        assert!(username(&ok).is_ok());
        let too_long = "a".repeat(USERNAME_MAX_LEN + 1);
        assert_eq!(
            username(&too_long),
            Err(UsernameError::TooLong(USERNAME_MAX_LEN + 1))
        );
    }

    #[test]
    fn rejects_reserved_usernames() {
        assert_eq!(
            username("root"),
            Err(UsernameError::Reserved("root".into()))
        );
        assert_eq!(
            username("daemon"),
            Err(UsernameError::Reserved("daemon".into()))
        );
        assert_eq!(
            username("wheel"),
            Err(UsernameError::Reserved("wheel".into()))
        );
    }

    #[test]
    fn counts_characters_not_bytes_for_length() {
        // Four characters, eight bytes. Should fail on charset, not length.
        assert_eq!(username("éééé"), Err(UsernameError::IllegalChars));
    }

    #[test]
    fn accepts_ordinary_hostnames() {
        for name in ["noos", "noos-os", "archbox", "a", "host123"] {
            assert!(hostname(name).is_ok(), "{name} devrait être accepté");
        }
    }

    #[test]
    fn rejects_malformed_hostnames() {
        assert_eq!(hostname(""), Err(HostnameError::Empty));
        assert_eq!(hostname("-noos"), Err(HostnameError::IllegalChars));
        assert_eq!(hostname("noos-"), Err(HostnameError::IllegalChars));
        assert_eq!(hostname("noos_os"), Err(HostnameError::IllegalChars));
        assert_eq!(hostname("noos os"), Err(HostnameError::IllegalChars));
        assert_eq!(hostname(&"a".repeat(64)), Err(HostnameError::TooLong(64)));
    }

    #[test]
    fn accepts_iana_timezone_shapes() {
        for tz in ["Europe/Paris", "UTC", "America/Argentina/Buenos_Aires", "Etc/GMT+2"] {
            assert!(timezone_shape(tz).is_ok(), "{tz} devrait passer");
        }
    }

    #[test]
    fn rejects_bad_timezone_shapes() {
        assert_eq!(timezone_shape(""), Err(TimezoneError::Empty));
        assert_eq!(timezone_shape("/etc/passwd"), Err(TimezoneError::Shape));
        assert_eq!(timezone_shape("../../etc/passwd"), Err(TimezoneError::Shape));
        assert_eq!(timezone_shape("Europe/../../etc"), Err(TimezoneError::Shape));
    }

    #[test]
    fn scores_passwords_monotonically() {
        let weak = password_strength("123456", &[]);
        let strong = password_strength("correct horse battery staple 47!", &[]);
        assert!(
            strong.score > weak.score,
            "un mot de passe long devrait mieux scorer : {weak:?} vs {strong:?}"
        );
        assert_eq!(weak.label, StrengthLabel::VeryWeak);
    }

    #[test]
    fn penalises_passwords_built_from_the_username() {
        let plain = password_strength("chomiam", &[]);
        let contextual = password_strength("chomiam", &["chomiam"]);
        assert!(
            contextual.score <= plain.score,
            "fournir le nom d'utilisateur ne doit pas augmenter le score"
        );
    }

    #[test]
    fn enforces_the_strong_password_box() {
        assert_eq!(password("", &[], true), Err(PasswordError::Empty));
        // "azerty" is a classic French keyboard walk: always weak.
        assert!(matches!(
            password("azerty", &[], true),
            Err(PasswordError::TooWeak(_))
        ));
        assert!(password("azerty", &[], false).is_ok());
        assert!(password("correct horse battery staple 47!", &[], true).is_ok());
    }

    #[test]
    fn score_maps_to_label() {
        assert_eq!(StrengthLabel::from_score(Score0To4(0)), StrengthLabel::VeryWeak);
        assert_eq!(StrengthLabel::from_score(Score0To4(2)), StrengthLabel::Fair);
        assert_eq!(StrengthLabel::from_score(Score0To4(3)), StrengthLabel::Strong);
        assert_eq!(StrengthLabel::from_score(Score0To4(4)), StrengthLabel::VeryStrong);
    }
}
