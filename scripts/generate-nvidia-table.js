#!/usr/bin/env bun
//
// Generates core/src/nvidia_devices.rs from NVIDIA's open-gpu-kernel-modules
// README, which carries the authoritative list of GPUs the open kernel modules
// support — i.e. everything from Turing onwards.
//
//   bun scripts/generate-nvidia-table.js
//
// The installer needs this table because Arch ships no equivalent of
// `nvidia-detect`: the detection has to work offline, from a live ISO, with no
// network and no extra tooling.

import { writeFileSync } from "node:fs";

const SOURCE =
  "https://raw.githubusercontent.com/NVIDIA/open-gpu-kernel-modules/main/README.md";
const OUTPUT = new URL("../installer/core/src/nvidia_devices.rs", import.meta.url);

const response = await fetch(SOURCE);
if (!response.ok) {
  console.error(`échec du téléchargement : ${response.status} ${response.statusText}`);
  process.exit(1);
}
const text = await response.text();

// Rows look like:  | NVIDIA GeForce RTX 2080 Ti | 1E04 |
// Some carry a subsystem pair after the device id:  | Quadro RTX 8000 | 1E30 1028 129E |
const TABLE_START = text.indexOf("| Product Name");
if (TABLE_START === -1) {
  console.error("tableau introuvable : le format du README a changé");
  process.exit(1);
}

const deviceIds = new Set();
for (const line of text.slice(TABLE_START).split("\n")) {
  const match = line.match(
    /^\|\s*[^|]+\|\s*([0-9A-Fa-f]{4})(?:\s+[0-9A-Fa-f]{4}\s+[0-9A-Fa-f]{4})?\s*\|/,
  );
  if (match) deviceIds.add(parseInt(match[1], 16));
}

if (deviceIds.size < 100) {
  console.error(`seulement ${deviceIds.size} identifiants extraits : parsing suspect`);
  process.exit(1);
}

const sorted = [...deviceIds].sort((a, b) => a - b);
const rows = [];
for (let i = 0; i < sorted.length; i += 8) {
  rows.push(
    "    " +
      sorted
        .slice(i, i + 8)
        .map((id) => `0x${id.toString(16).toUpperCase().padStart(4, "0")}`)
        .join(", ") +
      ",",
  );
}

const generated = `//! PCI device IDs of NVIDIA GPUs supported by the open kernel modules.
//!
//! GENERATED FILE — do not edit by hand.
//! Regenerate with: bun scripts/generate-nvidia-table.js
//!
//! Source: ${SOURCE}
//! ${sorted.length} device IDs, every one Turing or newer.
//!
//! This exists because Arch has no \`nvidia-detect\`: the installer runs from a
//! live ISO with no guaranteed network, so the GPU has to be classified from a
//! table shipped inside the binary. A device absent from this list is Pascal,
//! Volta, Maxwell or older, and needs a legacy driver from the AUR.

/// Sorted, so membership is a binary search.
const SUPPORTED_BY_OPEN_MODULES: &[u16] = &[
${rows.join("\n")}
];

/// Whether the open kernel modules drive this GPU.
///
/// The open modules cover Turing (GTX 16xx, RTX 20xx) and everything newer.
/// Note that the GTX 1650 and GTX 1660 *are* Turing, despite looking like
/// Pascal-era cards by name.
pub fn is_supported_by_open_modules(device_id: u16) -> bool {
    SUPPORTED_BY_OPEN_MODULES.binary_search(&device_id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        assert!(
            SUPPORTED_BY_OPEN_MODULES.windows(2).all(|w| w[0] < w[1]),
            "la table doit être triée et sans doublon pour la recherche binaire"
        );
    }

    #[test]
    fn recognises_turing_and_newer() {
        // The boundary that matters: GTX 16xx is Turing, not Pascal.
        for (id, name) in [
            (0x2184u16, "GeForce GTX 1660"),
            (0x1F82, "GeForce GTX 1650"),
            (0x1E04, "GeForce RTX 2080 Ti"),
            (0x2204, "GeForce RTX 3090"),
            (0x2684, "GeForce RTX 4090"),
        ] {
            assert!(
                is_supported_by_open_modules(id),
                "{name} (0x{id:04X}) devrait être supporté par les modules ouverts"
            );
        }
    }

    #[test]
    fn rejects_pascal_and_older() {
        // These need a legacy branch from the AUR.
        for (id, name) in [
            (0x1B80u16, "GeForce GTX 1080"),
            (0x1C82, "GeForce GTX 1050 Ti"),
            (0x1380, "GeForce GTX 750 Ti"),
            (0x1180, "GeForce GTX 680"),
        ] {
            assert!(
                !is_supported_by_open_modules(id),
                "{name} (0x{id:04X}) ne doit pas être annoncé comme supporté"
            );
        }
    }

    #[test]
    fn unknown_ids_are_not_supported() {
        assert!(!is_supported_by_open_modules(0x0000));
        assert!(!is_supported_by_open_modules(0xFFFF));
    }
}
`;

writeFileSync(OUTPUT, generated);
console.log(
  `${sorted.length} identifiants écrits dans ${OUTPUT.pathname.replace(process.cwd() + "/", "")}`,
);
