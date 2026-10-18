#!/usr/bin/env bun
//
// Generates the rEFInd theme from noos-svg.svg.
//
//   bun scripts/generate-refind-theme.js
//
// rEFInd draws ICNS, PNG, BMP and JPEG — four **static** bitmap formats. It has
// no SVG support and no frame sequences, so the travelling beam that animates
// the wordmark elsewhere has to be frozen here. Plymouth is where the animation
// lives; this is the still version.
//
// The outputs are committed, like Plymouth's frames, and `build-iso.sh` only
// checks that they are present. Regenerate when the logo changes:
//
//   background.png      1920x1080  the full-screen banner
//   os_noos.png          256x256   the icon beside our menu entry
//   selection_big.png    144x144   highlighted-entry background (OS icons)
//   selection_small.png  144x144   same, for the small tool row
//
// Colours: the wordmark keeps its own gradient — it *is* the logo — while the
// chrome around it uses the installer's design tokens (--bg #0a0b0d,
// --surface #121417, --accent #8ab4f8), the same choice Plymouth's progress bar
// already makes.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";

const SOURCE = new URL("../noos-svg.svg", import.meta.url).pathname;
const THEME = new URL(
  "../iso/airootfs/usr/share/noos/refind-theme/",
  import.meta.url,
).pathname;

/** Design tokens, repeated from installer/src/styles/tokens.css. */
const BG_TOP = "#121417";
const BG_BOTTOM = "#0a0b0d";
const ACCENT = "#8ab4f8";
const BORDER = "#2e3338";

/** The logo is 3:1; its own viewBox, reused by every nested copy. */
const LOGO_W = 480;
const LOGO_H = 160;

/**
 * Where the travelling beam is frozen, in dash units.
 *
 * A quarter of the travel the keyframes describe (440), which puts the two
 * beams in a balanced composition rather than overlapping at the start.
 */
const FROZEN_OFFSET = -110;

const scratch = "/tmp/noos-refind-theme";
rmSync(scratch, { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });
mkdirSync(THEME, { recursive: true });

const original = readFileSync(SOURCE, "utf8");

// Two edits to the source, both for the same reason Plymouth makes them:
//
//  - the background rectangle goes, because every asset here paints its own
//    background and a second one would read as a pasted-in image;
//  - the CSS animation is replaced by a literal `stroke-dashoffset`, since
//    nothing rasterises a keyframe.
//
// Note the secondary beam also carries a static `stroke-dashoffset: -220`. That
// value is *overridden* by its animation while the animation runs, so freezing
// both beams at the same offset is what reproduces the animated appearance —
// keeping `-220` would put the second beam out of phase.
const frozen = original
  .replace(/\.bg \{[^}]*\}/, ".bg { fill: none; }")
  .replace(/<rect width="100%" height="100%" class="bg" \/>/, "")
  .replace(/animation: loopOrbit[^;]*;/g, `stroke-dashoffset: ${FROZEN_OFFSET}px;`)
  .replace(/@keyframes loopOrbit \{[\s\S]*?\n    \}/, "")
  .trim();

const assets = {
  // The banner fills the screen. The wordmark sits high, because rEFInd draws
  // the row of entry icons across the middle and a logo behind them reads as a
  // mistake rather than as a design.
  "background.png": {
    width: 1920,
    height: 1080,
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080" width="1920" height="1080">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${BG_TOP}"/>
      <stop offset="100%" stop-color="${BG_BOTTOM}"/>
    </linearGradient>
    <radialGradient id="glow" cx="50%" cy="18%" r="46%">
      <stop offset="0%" stop-color="${ACCENT}" stop-opacity="0.13"/>
      <stop offset="100%" stop-color="${ACCENT}" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1920" height="1080" fill="url(#bg)"/>
  <rect width="1920" height="1080" fill="url(#glow)"/>
  <svg x="800" y="120" width="320" height="107" viewBox="0 0 ${LOGO_W} ${LOGO_H}">
${frozen}
  </svg>
</svg>`,
  },

  // The entry's icon. Square, because rEFInd scales icons into a square slot;
  // the 3:1 wordmark is letterboxed on a dark tile instead of being stretched.
  "os_noos.png": {
    width: 256,
    height: 256,
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${BG_TOP}"/>
      <stop offset="100%" stop-color="${BG_BOTTOM}"/>
    </linearGradient>
  </defs>
  <rect x="8" y="8" width="240" height="240" rx="52" fill="url(#tile)" stroke="${BORDER}" stroke-width="2"/>
  <svg x="24" y="87" width="208" height="69" viewBox="0 0 ${LOGO_W} ${LOGO_H}">
${frozen}
  </svg>
</svg>`,
  },
};

// rEFInd merges a selection image *behind* the icon, so transparency is what
// lets the icon show through. A rounded rect with a soft accent fill reads as a
// highlight without hiding anything.
const selection = (id) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 144 144" width="144" height="144">
  <defs>
    <linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${ACCENT}" stop-opacity="0.22"/>
      <stop offset="100%" stop-color="${ACCENT}" stop-opacity="0.07"/>
    </linearGradient>
  </defs>
  <rect x="6" y="6" width="132" height="132" rx="30"
        fill="url(#${id})" stroke="${ACCENT}" stroke-opacity="0.55" stroke-width="3"/>
</svg>`;

assets["selection_big.png"] = { width: 144, height: 144, svg: selection("selb") };
assets["selection_small.png"] = { width: 144, height: 144, svg: selection("sels") };

for (const [name, asset] of Object.entries(assets)) {
  const svgPath = `${scratch}/${name.replace(/\.png$/, ".svg")}`;
  writeFileSync(svgPath, asset.svg);
  execFileSync("rsvg-convert", [
    "-w", String(asset.width),
    "-h", String(asset.height),
    "-o", `${THEME}${name}`,
    svgPath,
  ]);
}

console.log(`  ${Object.keys(assets).length} images générées dans ${THEME.replace(`${process.cwd()}/`, "")}`);
for (const name of Object.keys(assets)) {
  console.log(`    ${name}`);
}
