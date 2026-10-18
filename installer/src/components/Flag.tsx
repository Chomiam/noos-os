/**
 * Flags, drawn rather than typed.
 *
 * Flag emoji (🇫🇷, 🇬🇧) are a pair of regional-indicator code points, and
 * rendering them requires a colour emoji font that carries the flag glyphs.
 * `noto-fonts` does not — that is `noto-fonts-emoji`, a separate package — so
 * on a minimal live medium they render as two letter boxes. Drawing them is
 * a few lines and always works.
 */

interface FlagProps {
  /** Rendered height in pixels; width follows the 3:2 ratio. */
  height?: number;
  className?: string;
}

function Frame({ height = 20, className, children }: FlagProps & { children: React.ReactNode }) {
  return (
    <svg
      width={height * 1.5}
      height={height}
      viewBox="0 0 60 40"
      className={className}
      aria-hidden="true"
      focusable="false"
      style={{ borderRadius: 3, display: "block", flex: "none" }}
    >
      {children}
    </svg>
  );
}

export function FlagFrance(props: FlagProps) {
  return (
    <Frame {...props}>
      <rect width="60" height="40" fill="#fff" />
      <rect width="20" height="40" fill="#002395" />
      <rect x="40" width="20" height="40" fill="#ed2939" />
    </Frame>
  );
}

/**
 * Union Jack.
 *
 * Simplified to the recognisable essentials — the counterchanged saltire and
 * the broad red cross. At 30×20 px the fine offset of the saltire is below one
 * pixel, so drawing it exactly would only add path data nobody can see.
 */
export function FlagUnitedKingdom(props: FlagProps) {
  return (
    <Frame {...props}>
      <rect width="60" height="40" fill="#012169" />

      {/* White saltire. */}
      <path d="M0 0 L60 40 M60 0 L0 40" stroke="#fff" strokeWidth="8" />

      {/* Red saltire, clipped to the diagonals. */}
      <path d="M0 0 L60 40 M60 0 L0 40" stroke="#c8102e" strokeWidth="4" />

      {/* White cross, then the red cross on top. */}
      <path d="M30 0 V40 M0 20 H60" stroke="#fff" strokeWidth="13" />
      <path d="M30 0 V40 M0 20 H60" stroke="#c8102e" strokeWidth="8" />
    </Frame>
  );
}
