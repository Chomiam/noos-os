#!/usr/bin/env bun
//
// Generates the Fastfetch logo (45°-tilted animated infinity symbol) from noos-svg.svg.
//
//   bun scripts/generate-fastfetch-logo.js
//
// Extracts the central infinity path and its neon gradient beams from
// `noos-svg.svg`, removes the outer "N" and "S" letters, and tilts the
// infinity loop at 45 degrees (`rotate(-45, 105, 55)`).
//
// Outputs:
//   iso/airootfs/usr/share/noos/fastfetch/noos-infinity.svg
//   iso/airootfs/usr/share/noos/fastfetch/noos-infinity.png  (512x512)

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const SOURCE = new URL("../noos-svg.svg", import.meta.url).pathname;
const AIROOTFS = new URL("../iso/airootfs", import.meta.url).pathname;
const OUT_DIR = `${AIROOTFS}/usr/share/noos/fastfetch/`;
const TAURI_ICONS = new URL("../installer/src-tauri/icons/", import.meta.url).pathname;

const SIZE = 512;
const PRIMARY_OFFSET = -35;
const SECONDARY_OFFSET = -255;

mkdirSync(OUT_DIR, { recursive: true });

const original = readFileSync(SOURCE, "utf8");
const pathMatch = original.match(/<path class="inf-track" d="([^"]+)"/);
if (!pathMatch) {
  throw new Error("Impossible de trouver `.inf-track` dans noos-svg.svg");
}
const infinityPath = pathMatch[1];

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="22 22 156 156" width="${SIZE}" height="${SIZE}">
  <defs>
    <!-- Dégradé lumineux élégant (Bleu saphir / Lavande / Rose doux) -->
    <linearGradient id="infinityGlow" gradientUnits="userSpaceOnUse" x1="30" y1="15" x2="180" y2="95">
      <stop offset="0%" stop-color="#89b4fa" />
      <stop offset="50%" stop-color="#cba6f7" />
      <stop offset="100%" stop-color="#f38ba8" />
    </linearGradient>

    <!-- Léger halo lumineux néon -->
    <filter id="neonBlur" filterUnits="userSpaceOnUse" x="0" y="0" width="200" height="200">
      <feGaussianBlur stdDeviation="3.5" result="blur" />
      <feMerge>
        <feMergeNode in="blur" />
        <feMergeNode in="SourceGraphic" />
      </feMerge>
    </filter>
  </defs>

  <style>
    /* Rail discret de base pour l'infini */
    .inf-track {
      fill: none;
      stroke: #36394e;
      stroke-width: 13;
      stroke-linecap: round;
      stroke-linejoin: round;
    }

    /* Double pulsation fluide le long du lemniscate */
    .inf-beam-primary {
      fill: none;
      stroke: url(#infinityGlow);
      stroke-width: 13;
      stroke-linecap: round;
      stroke-linejoin: round;
      stroke-dasharray: 110 330;
      stroke-dashoffset: ${PRIMARY_OFFSET};
      animation: loopOrbitPrimary 3.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
    }

    .inf-beam-secondary {
      fill: none;
      stroke: url(#infinityGlow);
      stroke-width: 13;
      stroke-linecap: round;
      stroke-linejoin: round;
      stroke-dasharray: 55 385;
      stroke-dashoffset: ${SECONDARY_OFFSET};
      opacity: 0.85;
      animation: loopOrbitSecondary 3.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
    }

    @keyframes loopOrbitPrimary {
      0% { stroke-dashoffset: ${PRIMARY_OFFSET}; }
      100% { stroke-dashoffset: ${PRIMARY_OFFSET - 440}; }
    }

    @keyframes loopOrbitSecondary {
      0% { stroke-dashoffset: ${SECONDARY_OFFSET}; }
      100% { stroke-dashoffset: ${SECONDARY_OFFSET - 440}; }
    }
  </style>

  <!-- Rail de guidage incliné à 45 degrés -->
  <g transform="translate(-5, 45) rotate(-45, 105, 55)">
    <path class="inf-track" fill="none" stroke="#36394e" stroke-width="13" stroke-linecap="round" stroke-linejoin="round" d="${infinityPath}" />
  </g>

  <!-- Impulsions lumineuses synchronisées -->
  <g filter="url(#neonBlur)">
    <g transform="translate(-5, 45) rotate(-45, 105, 55)">
      <path class="inf-beam-primary" fill="none" stroke="url(#infinityGlow)" stroke-width="13" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="110 330" stroke-dashoffset="${PRIMARY_OFFSET}" d="${infinityPath}" />
      <path class="inf-beam-secondary" fill="none" stroke="url(#infinityGlow)" stroke-width="13" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="55 385" stroke-dashoffset="${SECONDARY_OFFSET}" opacity="0.85" d="${infinityPath}" />
    </g>
  </g>
</svg>
`;

const svgPath = `${OUT_DIR}noos-infinity.svg`;
const pngPath = `${OUT_DIR}noos-infinity.png`;

writeFileSync(svgPath, svg);
execFileSync("rsvg-convert", [
  "-w",
  String(SIZE),
  "-h",
  String(SIZE),
  "-o",
  pngPath,
  svgPath,
]);

// Also install the 45°-tilted infinity logo as the system-wide distribution logo
// across pixmaps, hicolor, KDE Info Center (kcm-about-distrorc), and Tauri icons.
const pixmapsDir = `${AIROOTFS}/usr/share/pixmaps`;
const hicolorScalable = `${AIROOTFS}/usr/share/icons/hicolor/scalable/apps`;
const hicolor256 = `${AIROOTFS}/usr/share/icons/hicolor/256x256/apps`;
const hicolor512 = `${AIROOTFS}/usr/share/icons/hicolor/512x512/apps`;
const xdgDir = `${AIROOTFS}/etc/xdg`;

for (const dir of [pixmapsDir, hicolorScalable, hicolor256, hicolor512, xdgDir]) {
  mkdirSync(dir, { recursive: true });
}

copyFileSync(svgPath, `${pixmapsDir}/noos-logo.svg`);
copyFileSync(pngPath, `${pixmapsDir}/noos-logo.png`);

for (const name of [
  "archlinux-logo.svg",
  "noos-logo.svg",
  "distributor-logo.svg",
  "distributor-logo-archlinux.svg",
]) {
  copyFileSync(svgPath, `${hicolorScalable}/${name}`);
}

const png256 = `${hicolor256}/noos-logo.png`;
execFileSync("rsvg-convert", ["-w", "256", "-h", "256", "-o", png256, svgPath]);
for (const name of ["archlinux-logo.png", "distributor-logo.png"]) {
  copyFileSync(png256, `${hicolor256}/${name}`);
}
for (const name of ["archlinux-logo.png", "noos-logo.png", "distributor-logo.png"]) {
  copyFileSync(pngPath, `${hicolor512}/${name}`);
}

writeFileSync(
  `${xdgDir}/kcm-about-distrorc`,
  "[General]\nLogoPath=/usr/share/noos/fastfetch/noos-infinity.svg\n",
);

const tauriSizes = {
  "32x32.png": 32,
  "128x128.png": 128,
  "128x128@2x.png": 256,
  "icon.png": 512,
  "Square30x30Logo.png": 30,
  "Square44x44Logo.png": 44,
  "Square71x71Logo.png": 71,
  "Square89x89Logo.png": 89,
  "Square107x107Logo.png": 107,
  "Square142x142Logo.png": 142,
  "Square150x150Logo.png": 150,
  "Square284x284Logo.png": 284,
  "Square310x310Logo.png": 310,
  "StoreLogo.png": 50,
};
for (const [file, dim] of Object.entries(tauriSizes)) {
  execFileSync("rsvg-convert", [
    "-w",
    String(dim),
    "-h",
    String(dim),
    "-o",
    `${TAURI_ICONS}${file}`,
    svgPath,
  ]);
}

console.log(`Logo Fastfetch et logo de distribution générés dans ${OUT_DIR.replace(`${process.cwd()}/`, "")}`);
console.log("  noos-infinity.svg");
console.log("  noos-infinity.png");
