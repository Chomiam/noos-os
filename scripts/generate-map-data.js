#!/usr/bin/env bun
//
// Generates installer/src/lib/worldData.ts: the land outlines and the list of
// IANA time zones with their coordinates.
//
//   bun scripts/generate-map-data.js
//
// Both have to be compiled into the app. The installer runs from a live ISO
// with no network, so nothing may be fetched at runtime — and the 52 MB
// timezone-boundary dataset is far too large to embed anyway. Nearest-zone
// lookup over 312 city coordinates is accurate enough for picking a time zone
// by clicking a map, and costs about 8 KB.
//
// The projection is equirectangular with a viewBox of "0 0 360 180", so the
// SVG coordinates ARE degrees. That makes the click-to-latitude inverse
// trivial: lon = x - 180, lat = 90 - y.

import { readFileSync, writeFileSync } from "node:fs";

const LAND_SOURCE =
  "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_land.geojson";
const ZONE_TABLE = "/usr/share/zoneinfo/zone1970.tab";
const OUTPUT = new URL("../installer/src/lib/worldData.ts", import.meta.url);

/** Coordinates are rounded to this many decimals. */
const PRECISION = 1;

const round = (value) => Number(value.toFixed(PRECISION));

// ------------------------------------------------------------------ land ---

const response = await fetch(LAND_SOURCE);
if (!response.ok) {
  console.error(`échec du téléchargement : ${response.status}`);
  process.exit(1);
}
const land = await response.json();

const paths = [];
let pointCount = 0;

for (const feature of land.features) {
  const { type, coordinates } = feature.geometry;
  const polygons = type === "Polygon" ? [coordinates] : coordinates;

  for (const polygon of polygons) {
    // Only the outer ring: at this scale, holes are lakes the size of a
    // pixel, and dropping them roughly halves the payload.
    const ring = polygon[0];
    if (!ring || ring.length < 4) continue;

    let segment = "";
    let previous = null;

    for (const [lon, lat] of ring) {
      // Rounding makes consecutive points collide; emitting duplicates would
      // bloat the path for nothing.
      const x = round(lon + 180);
      const y = round(90 - lat);
      if (previous && previous[0] === x && previous[1] === y) continue;
      previous = [x, y];

      segment += `${segment ? "L" : "M"}${x} ${y}`;
      pointCount += 1;
    }

    paths.push(`${segment}Z`);
  }
}

// --------------------------------------------------------------- timezones ---

const zoneLines = readFileSync(ZONE_TABLE, "utf8")
  .split("\n")
  .filter((line) => line && !line.startsWith("#"));

/**
 * Parses the `±DDMM±DDDMM` coordinate field of zone1970.tab.
 * Example: "+4230+00131" is 42°30'N, 1°31'E.
 */
function parseCoordinates(field) {
  const match = /^([+-])(\d{2})(\d{2})([+-])(\d{3})(\d{2})$/.exec(field);
  if (!match) return null;

  const [, latSign, latDeg, latMin, lonSign, lonDeg, lonMin] = match;
  const sign = (s) => (s === "-" ? -1 : 1);

  return {
    lat: sign(latSign) * (Number(latDeg) + Number(latMin) / 60),
    lon: sign(lonSign) * (Number(lonDeg) + Number(lonMin) / 60),
  };
}

const zones = [];
for (const line of zoneLines) {
  const [countries, coordinates, id] = line.split("\t");
  const point = parseCoordinates(coordinates);
  if (!point || !id) continue;

  zones.push({
    id,
    countries,
    lat: round(point.lat),
    lon: round(point.lon),
  });
}

if (zones.length < 200) {
  console.error(`seulement ${zones.length} fuseaux extraits : parsing suspect`);
  process.exit(1);
}

// ------------------------------------------------------------------ write ---

const landPath = paths.join("");

const generated = `// GENERATED FILE — do not edit by hand.
//
// Regenerate with: bun scripts/generate-map-data.js
//
// Land outlines: Natural Earth 1:110m (public domain).
// Time zones: IANA zone1970.tab, shipped by tzdata.
//
// The projection is equirectangular with a viewBox of "0 0 360 180", so
// coordinates are degrees: x = longitude + 180, y = 90 - latitude.

/** Land outlines as a single SVG path, in degree space. */
export const LAND_PATH =
  "${landPath}";

/** Bounds of the projection, as an SVG viewBox. */
export const MAP_VIEWBOX = "0 0 360 180";

export interface Zone {
  /** IANA identifier, e.g. \`Europe/Paris\`. */
  id: string;
  /** ISO country codes the zone covers, for searching. */
  countries: string;
  lat: number;
  lon: number;
}

/** Every zone in zone1970.tab, with its reference city's coordinates. */
export const ZONES: Zone[] = ${JSON.stringify(zones)};
`;

writeFileSync(OUTPUT, generated);

const kb = (generated.length / 1024).toFixed(0);
console.log(
  `${paths.length} polygones (${pointCount} points) et ${zones.length} fuseaux → ${kb} Ko`,
);
