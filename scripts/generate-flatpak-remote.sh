#!/usr/bin/env bash
#
# Regenerates installer/core/src/flatpak.rs from Flathub's own remote
# definition.
#
#     ./scripts/generate-flatpak-remote.sh
#
# Run this when Flathub rotates its signing key: the key is baked into the file
# below, so an ISO built before the rotation would add the remote and then
# reject every signature it downloads.
#
# Generated rather than typed: the key is 3.8 KB of base64, and a single wrong
# character produces a remote that looks configured and rejects everything.

set -euo pipefail

readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly OUT="${ROOT}/installer/core/src/flatpak.rs"
readonly URL="https://dl.flathub.org/repo/flathub.flatpakrepo"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

echo "  récupération de ${URL}"
curl -fsSL "$URL" -o "$tmp"

# A truncated definition is worse than none: it would configure a remote with
# no signing key, which fails later as "Flathub is broken" rather than here.
grep -q '^\[Flatpak Repo\]' "$tmp" || { echo "erreur : ce n'est pas un .flatpakrepo" >&2; exit 1; }
grep -q '^GPGKey='          "$tmp" || { echo "erreur : aucune clé GPG dans la définition" >&2; exit 1; }
grep -q '^Url=https://'     "$tmp" || { echo "erreur : l'URL n'est pas en https" >&2; exit 1; }

# A raw Rust string cannot contain the sequence that closes it. Checked with an
# explicit `if`: under `set -e` a `grep … && { …; }` list aborts the script on
# the *success* path.
if grep -q '"#' "$tmp"; then
  echo 'erreur : le fichier contient "#, impossible à embarquer en chaîne brute' >&2
  exit 1
fi

{
  cat <<'HEADER'
//! The Flathub remote, shipped as a file rather than added by a command.
//!
//! **GENERATED** — regenerate with `scripts/generate-flatpak-remote.sh` when
//! Flathub rotates its signing key. The key is baked in, so an ISO built before
//! a rotation would add the remote and then reject every signature it
//! downloads.
//!
//! Why a file rather than `flatpak remote-add`: flatpak reads every
//! `*.flatpakrepo` under `/etc/flatpak/remotes.d/` at startup and adds the
//! remotes it finds. The command form would have to run inside the target's
//! chroot, where there is no D-Bus — the same absence that made `snapper` fail
//! three steps away from anything that named it — and would fetch the key over
//! the network on top. A file needs neither.

/// Directory flatpak scans for remote definitions.
pub const REMOTES_DIR: &str = "/etc/flatpak/remotes.d";

/// The file name **is** the remote name: flatpak reads no name from the file,
/// so renaming this renames the remote, and every `flatpak install flathub …`
/// in every guide stops resolving.
pub const FLATHUB_FILE: &str = "flathub.flatpakrepo";

/// Contents of the file above, byte for byte as Flathub publishes it.
HEADER
  # Opened with printf and not inside the heredoc: a heredoc ends its line with
  # a newline, which would make the string start with a blank line and put the
  # first byte of the definition out of step with what Flathub serves.
  printf 'pub const FLATHUB_CONF: &str = r#"'
  cat "$tmp"
  printf '"#;\n'
  cat <<'FOOTER'

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remote_is_flathub_over_https() {
        assert!(FLATHUB_CONF.starts_with("[Flatpak Repo]"));
        assert!(FLATHUB_CONF.contains("Url=https://dl.flathub.org/repo/"));
        assert!(
            !FLATHUB_CONF.contains("Url=http://"),
            "un dépôt Flatpak doit être servi en https"
        );
    }

    #[test]
    fn the_definition_carries_a_signing_key() {
        // Without it flatpak adds the remote happily and then refuses every
        // application it downloads, which reads as "Flathub is broken".
        let key = FLATHUB_CONF
            .lines()
            .find_map(|l| l.strip_prefix("GPGKey="))
            .expect("GPGKey doit être présent");
        assert!(
            key.len() > 1000,
            "clé suspicieusement courte : {} octets",
            key.len()
        );
    }

    #[test]
    fn the_file_name_is_the_remote_name() {
        assert_eq!(FLATHUB_FILE, "flathub.flatpakrepo");
        assert_eq!(
            format!("{REMOTES_DIR}/{FLATHUB_FILE}"),
            "/etc/flatpak/remotes.d/flathub.flatpakrepo"
        );
    }
}
FOOTER
} > "$OUT"

echo "  écrit : ${OUT} ($(stat -c%s "$OUT") octets)"
