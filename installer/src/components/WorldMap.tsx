/**
 * Interactive world map for picking a time zone.
 *
 * Fully offline: the land outlines and the zone coordinates are compiled into
 * the bundle by `scripts/generate-map-data.js`. The 52 MB timezone-boundary
 * dataset would be far too large to embed, and nearest-zone lookup over 265
 * reference cities is accurate enough for clicking a map.
 *
 * The SVG viewBox is "0 0 360 180" and the container is forced to a 2:1
 * aspect ratio, so the projection is exactly linear and a click converts to
 * latitude and longitude with two subtractions.
 */

import { useMemo, useRef, useState } from "react";

import { useI18n } from "../i18n";
import { LAND_PATH, MAP_VIEWBOX, ZONES, type Zone } from "../lib/worldData";
import { Search } from "./icons";

/** Added by hand: zone1970.tab lists geographic zones only. */
const UTC_ZONE: Zone = { id: "UTC", countries: "", lat: 51.5, lon: 0 };

const ALL_ZONES: Zone[] = [...ZONES, UTC_ZONE];

const toRadians = (degrees: number) => (degrees * Math.PI) / 180;

/**
 * Great-circle distance between two points, in radians of arc.
 *
 * Haversine rather than a flat difference: a flat metric badly misjudges
 * distances at high latitude, which is where several zones sit close together.
 */
function angularDistance(
  latA: number,
  lonA: number,
  latB: number,
  lonB: number,
): number {
  const deltaLat = toRadians(latB - latA);
  const deltaLon = toRadians(lonB - lonA);
  const h =
    Math.sin(deltaLat / 2) ** 2 +
    Math.cos(toRadians(latA)) * Math.cos(toRadians(latB)) * Math.sin(deltaLon / 2) ** 2;
  return 2 * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** The zone whose reference city is closest to a point. */
export function nearestZone(lat: number, lon: number): Zone {
  let best = ALL_ZONES[0];
  let bestDistance = Infinity;

  for (const zone of ALL_ZONES) {
    const distance = angularDistance(lat, lon, zone.lat, zone.lon);
    if (distance < bestDistance) {
      bestDistance = distance;
      best = zone;
    }
  }

  return best;
}

/** "America/Argentina/Buenos_Aires" → "Buenos Aires". */
export function zoneCity(id: string): string {
  const last = id.split("/").pop() ?? id;
  return last.replace(/_/g, " ");
}

export function WorldMap({
  selected,
  onSelect,
}: {
  selected: string;
  onSelect: (zoneId: string) => void;
}) {
  const { t } = useI18n();
  const svgRef = useRef<SVGSVGElement>(null);
  const [query, setQuery] = useState("");

  const selectedZone = useMemo(
    () => ALL_ZONES.find((zone) => zone.id === selected),
    [selected],
  );

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return [];
    return ALL_ZONES.filter(
      (zone) =>
        zone.id.toLowerCase().includes(needle) ||
        zone.countries.toLowerCase().includes(needle),
    ).slice(0, 8);
  }, [query]);

  const handleClick = (event: React.MouseEvent<SVGSVGElement>) => {
    const svg = svgRef.current;
    if (!svg) return;

    const bounds = svg.getBoundingClientRect();
    // Linear because preserveAspectRatio is "none" and the box is 2:1.
    const x = ((event.clientX - bounds.left) / bounds.width) * 360;
    const y = ((event.clientY - bounds.top) / bounds.height) * 180;

    onSelect(nearestZone(90 - y, x - 180).id);
  };

  return (
    <div className="map">
      <div className="map__search">
        <Search size={16} />
        <input
          className="map__search-input"
          value={query}
          placeholder={t("timezone.search")}
          onChange={(event) => setQuery(event.target.value)}
          spellCheck={false}
          autoComplete="off"
        />
      </div>

      {matches.length > 0 && (
        <ul className="map__results">
          {matches.map((zone) => (
            <li key={zone.id}>
              <button
                type="button"
                className="map__result"
                onClick={() => {
                  onSelect(zone.id);
                  setQuery("");
                }}
              >
                <span className="map__result-city">{zoneCity(zone.id)}</span>
                <span className="map__result-id">{zone.id}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="map__canvas">
        <svg
          ref={svgRef}
          viewBox={MAP_VIEWBOX}
          preserveAspectRatio="none"
          className="map__svg"
          onClick={handleClick}
          role="button"
          tabIndex={0}
          aria-label={t("timezone.title")}
        >
          <path d={LAND_PATH} className="map__land" />

          {/* Graticule every 30°, as a quiet reference grid. */}
          {[30, 60, 90, 120, 150, 210, 240, 270, 300, 330].map((x) => (
            <line key={`v${x}`} x1={x} y1={0} x2={x} y2={180} className="map__grid" />
          ))}
          {[30, 60, 90, 120, 150].map((y) => (
            <line key={`h${y}`} x1={0} y1={y} x2={360} y2={y} className="map__grid" />
          ))}

          {selectedZone && (
            <g className="map__marker">
              <circle cx={selectedZone.lon + 180} cy={90 - selectedZone.lat} r={7} className="map__marker-halo" />
              <circle cx={selectedZone.lon + 180} cy={90 - selectedZone.lat} r={2.6} className="map__marker-dot" />
            </g>
          )}
        </svg>
      </div>

      <div className="map__selected">
        <span className="dim">{t("timezone.selected")}</span>
        <span className="map__selected-city">{zoneCity(selected)}</span>
        <span className="mono dim">{selected}</span>
      </div>
    </div>
  );
}
