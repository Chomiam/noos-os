#!/usr/bin/env bun
//
// Generates installer/src/lib/keymaps.ts from the system's own XKB data.
//
//   bun scripts/generate-keymaps.js
//
// Why this exists
// ---------------
// `cage` reads the keyboard layout from the environment once, at startup, and
// no Wayland protocol lets a client change its compositor's keymap. Applying a
// layout to the running session therefore meant restarting the compositor —
// which showed the user a visible round-trip through the tty.
//
// This removes that entirely. The installer is the only thing the user types
// into, so it can translate keystrokes itself: `KeyboardEvent.code` names the
// *physical* key, which is layout-independent, and the tables below say what
// character that key produces under each layout.
//
// The tables come from `xkbcli`, which is part of libxkbcommon — the same
// library the compositor uses. So the mapping cannot drift from what the
// system would have done.
//
// `keysymdef.h` (from xorgproto) supplies the keysym-name to Unicode mapping.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const KEYSYM_HEADER = "/usr/include/X11/keysymdef.h";
const OUTPUT = new URL("../installer/src/lib/keymaps.ts", import.meta.url);

/**
 * `KeyboardEvent.code` to XKB key name.
 *
 * The XKB name is derived from the evdev scancode plus 8, which is why the
 * names look arbitrary. This is the standard table and does not vary between
 * systems — it is the physical key, not the character.
 */
const CODE_TO_XKB = {
  Backquote: "TLDE", Digit1: "AE01", Digit2: "AE02", Digit3: "AE03",
  Digit4: "AE04", Digit5: "AE05", Digit6: "AE06", Digit7: "AE07",
  Digit8: "AE08", Digit9: "AE09", Digit0: "AE10", Minus: "AE11",
  Equal: "AE12",

  KeyQ: "AD01", KeyW: "AD02", KeyE: "AD03", KeyR: "AD04", KeyT: "AD05",
  KeyY: "AD06", KeyU: "AD07", KeyI: "AD08", KeyO: "AD09", KeyP: "AD10",
  BracketLeft: "AD11", BracketRight: "AD12",

  KeyA: "AC01", KeyS: "AC02", KeyD: "AC03", KeyF: "AC04", KeyG: "AC05",
  KeyH: "AC06", KeyJ: "AC07", KeyK: "AC08", KeyL: "AC09", Semicolon: "AC10",
  Quote: "AC11",

  Backslash: "BKSL",
  KeyZ: "AB01", KeyX: "AB02", KeyC: "AB03", KeyV: "AB04", KeyB: "AB05",
  KeyN: "AB06", KeyM: "AB07", Comma: "AB08", Period: "AB09", Slash: "AB10",
  Space: "SPCE",
};

/** The layouts to generate, as `xkbcli` names them. */
const LAYOUTS = [
  { id: "us", x11: "us", variant: "" },
  { id: "gb", x11: "gb", variant: "" },
  { id: "fr", x11: "fr", variant: "" },
  { id: "be", x11: "be", variant: "" },
  { id: "ch-fr", x11: "ch", variant: "fr" },
  { id: "ch-de", x11: "ch", variant: "de" },
  { id: "ca-fr", x11: "ca", variant: "fr" },
  { id: "de", x11: "de", variant: "" },
  { id: "es", x11: "es", variant: "" },
  { id: "latam", x11: "latam", variant: "" },
  { id: "it", x11: "it", variant: "" },
  { id: "pt", x11: "pt", variant: "" },
  { id: "br", x11: "br", variant: "" },
  { id: "nl", x11: "nl", variant: "" },
  { id: "dk", x11: "dk", variant: "" },
  { id: "no", x11: "no", variant: "" },
  { id: "se", x11: "se", variant: "" },
  { id: "fi", x11: "fi", variant: "" },
  { id: "is", x11: "is", variant: "" },
  { id: "pl", x11: "pl", variant: "" },
  { id: "cz", x11: "cz", variant: "" },
  { id: "sk", x11: "sk", variant: "" },
  { id: "hu", x11: "hu", variant: "" },
  { id: "ro", x11: "ro", variant: "" },
  { id: "bg", x11: "bg", variant: "" },
  { id: "gr", x11: "gr", variant: "" },
  { id: "ru", x11: "ru", variant: "" },
  { id: "ua", x11: "ua", variant: "" },
  { id: "tr", x11: "tr", variant: "" },
  { id: "il", x11: "il", variant: "" },
  { id: "ara", x11: "ara", variant: "" },
  { id: "in", x11: "in", variant: "" },
  { id: "jp", x11: "jp", variant: "" },
  { id: "kr", x11: "kr", variant: "" },
  { id: "cn", x11: "cn", variant: "" },
  { id: "tw", x11: "tw", variant: "" },
  { id: "th", x11: "th", variant: "" },
  { id: "vn", x11: "vn", variant: "" },
  { id: "za", x11: "za", variant: "" },
  { id: "ng", x11: "ng", variant: "" },
];

// ------------------------------------------------------- keysym to Unicode ---

/** keysym name → single character, from keysymdef.h. */
const keysymToChar = new Map();

for (const line of readFileSync(KEYSYM_HEADER, "utf8").split("\n")) {
  const match = /^#define XK_(\w+)\s+(0x[0-9a-fA-F]+)\s+\/\*\s*(.*?)\s*\*\//.exec(line);
  if (!match) continue;

  const [, name, hex, comment] = match;
  const value = parseInt(hex, 16);

  // The Unicode is spelled out in the comment for everything that is not a
  // plain Latin-1 codepoint.
  const unicode = /U\+([0-9A-Fa-f]{4,6})/.exec(comment);
  const codepoint = unicode ? parseInt(unicode[1], 16) : value;

  // Keysyms in the Latin-1 range *are* their Unicode codepoint. Anything else
  // without an explicit U+ — dead keys, modifiers, function keys — is not a
  // character and is skipped.
  if (!unicode && !(codepoint >= 0x20 && codepoint <= 0xff)) continue;
  if (codepoint === 0) continue;

  try {
    keysymToChar.set(name, String.fromCodePoint(codepoint));
  } catch {
    // Out of range: not a real character.
  }
}

// ----------------------------------------------------------- compile layouts ---

/** `[unshifted, shifted, altgr, shift+altgr]`, with "" for "no character". */
const tables = {};

for (const layout of LAYOUTS) {
  const args = ["compile-keymap", "--layout", layout.x11];
  if (layout.variant) args.push("--variant", layout.variant);

  let keymap;
  try {
    keymap = execFileSync("xkbcli", args, { encoding: "utf8" });
  } catch (error) {
    console.error(`  échec pour ${layout.id} : ${error.message}`);
    process.exit(1);
  }

  const table = {};
  const line = /^\s+key <([A-Z0-9]+)>\s+\{\s+\[([^\]]*)\]/;

  for (const row of keymap.split("\n")) {
    const match = line.exec(row);
    if (!match) continue;

    const [, xkbName, levels] = match;
    const keysyms = levels.split(",").map((s) => s.trim());

    // Only the keys the browser can report.
    const code = Object.keys(CODE_TO_XKB).find((c) => CODE_TO_XKB[c] === xkbName);
    if (!code) continue;

    // `NoSymbol` and empty entries mean the key produces nothing at that
    // level; keeping them as "" lets the runtime fall through cleanly.
    //
    // Padded to four: a key may declare fewer levels than another (Space has
    // one, most letters four), and a fixed-length tuple keeps the runtime
    // lookup free of bounds checks.
    const chars = keysyms.slice(0, 4).map((sym) => keysymToChar.get(sym) ?? "");
    while (chars.length < 4) chars.push("");

    // Skip keys that produce nothing at any level — Caps Lock, the modifiers.
    if (chars.every((c) => c === "")) continue;
    table[code] = chars;
  }

  tables[layout.id] = table;
}

if (Object.keys(tables).length !== LAYOUTS.length) {
  console.error("certaines dispositions n'ont pas été générées");
  process.exit(1);
}

// ------------------------------------------------------------------- write ---

const generated = `// GENERATED FILE — do not edit by hand.
//
// Regenerate with: bun scripts/generate-keymaps.js
//
// Source: \`xkbcli compile-keymap\` (libxkbcommon), the same library the
// compositor uses, so these tables cannot drift from the system's own
// behaviour. Keysym names were resolved through keysymdef.h (xorgproto).
//
// Why the installer remaps keystrokes itself
// ------------------------------------------
// \`cage\` reads the layout from the environment once at startup, and no Wayland
// protocol lets a client change its compositor's keymap. Restarting the
// compositor to apply a layout showed a visible round-trip through the tty.
// Since the installer is the only thing the user types into, it translates
// keystrokes itself instead: \`KeyboardEvent.code\` names the physical key,
// which does not depend on the layout.
//
// Levels are, in order: unshifted, Shift, AltGr, Shift+AltGr. An empty string
// means the key produces no character at that level.

export type Levels = [string, string, string, string];
export type KeyTable = Record<string, Levels>;

/**
 * \`KeyboardEvent.code\` to XKB key name.
 *
 * The XKB name comes from the evdev scancode plus 8, which is why the names
 * look arbitrary. It is the same on every system: this names the physical key,
 * not the character it produces.
 *
 * Emitted here rather than hand-written elsewhere so the two cannot disagree —
 * a key present in one and missing from the other would silently stop being
 * remapped.
 */
export const CODE_TO_XKB: Record<string, string> = ${JSON.stringify(CODE_TO_XKB)};

/** The XKB key name for a physical key, if it is one we translate. */
export function xkbNameFor(code: string): string | undefined {
  return CODE_TO_XKB[code];
}

export const KEYMAPS: Record<string, KeyTable> = ${JSON.stringify(tables)};

/** Whether a table exists for this layout. */
export function hasKeymap(layout: string): boolean {
  return layout in KEYMAPS;
}
`;

writeFileSync(OUTPUT, generated);

const count = Object.values(tables).reduce((n, t) => n + Object.keys(t).length, 0);
const kb = (generated.length / 1024).toFixed(0);
console.log(
  `${Object.keys(tables).length} dispositions, ${count} touches mappées → ${kb} Ko`,
);
console.log(`  ${OUTPUT.pathname.replace(process.cwd() + "/", "")}`);
