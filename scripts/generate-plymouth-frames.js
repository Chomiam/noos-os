#!/usr/bin/env bun
//
// Generates the animated Plymouth splash from noos-svg.svg.
//
//   bun scripts/generate-plymouth-frames.js
//
// Plymouth draws PNGs, not SVG, and it has no CSS engine — so the gradient
// beam that travels along the infinity has to be baked into a sequence of
// frames. Each frame is the same artwork with the beam at a different point,
// and the theme cycles through them.
//
// The frames are what the SVG's own animation looks like at each step: same
// path, same dash pattern, a different `stroke-dashoffset`.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";

const SOURCE = new URL("../noos-svg.svg", import.meta.url).pathname;
// `.pathname`, not the URL itself: stringifying a file: URL gives
// "file:///…", which rsvg-convert cannot open.
const THEME = new URL(
  "../iso/airootfs/usr/share/plymouth/themes/noos/",
  import.meta.url,
).pathname;

/** Frames per animation cycle. The SVG runs 3.6s; 24 frames is 6.7 fps. */
const FRAMES = 24;
/** Total dash travel over one cycle, from the SVG's keyframes. */
const TRAVEL = 440;
/** Rendered size. The wordmark is 3:1. */
const WIDTH = 720;
const HEIGHT = 240;

const scratch = "/tmp/noos-plymouth-frames";
rmSync(scratch, { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });
mkdirSync(THEME, { recursive: true });

const original = readFileSync(SOURCE, "utf8");

// The background rectangle is dropped: Plymouth paints its own background, and
// a second one behind the mark would read as a pasted-in image.
const transparent = original
  .replace(/\.bg \{[^}]*\}/, ".bg { fill: none; }")
  .replace(/<rect width="100%" height="100%" class="bg" \/>/, "");

for (let frame = 0; frame < FRAMES; frame += 1) {
  const offset = -Math.round((frame / FRAMES) * TRAVEL);

  // Freeze the animation at this point. Both beams are pinned to the same
  // offset so they stay in the phase relationship the SVG gives them.
  const still = transparent
    .replace(
      /animation: loopOrbit[^;]*;/g,
      `stroke-dashoffset: ${offset}px;`,
    )
    .replace(/@keyframes loopOrbit \{[\s\S]*?\n    \}/, "");

  const svgPath = `${scratch}/frame-${frame}.svg`;
  writeFileSync(svgPath, still);

  const pngPath = `${THEME}frame-${String(frame).padStart(2, "0")}.png`;
  execFileSync("rsvg-convert", [
    "-w", String(WIDTH),
    "-h", String(HEIGHT),
    "-o", pngPath,
    svgPath,
  ]);
}

// A single still frame, for the case where Plymouth has to draw something
// before the sequence is loaded.
writeFileSync(`${THEME}logo.png`, readFileSync(`${THEME}frame-00.png`));

console.log(
  `${FRAMES} images générées dans ${THEME.replace(process.cwd() + "/", "")}`,
);
console.log(`  frame-00.png … frame-${String(FRAMES - 1).padStart(2, "0")}.png`);
console.log(`  logo.png (image fixe de secours)`);
