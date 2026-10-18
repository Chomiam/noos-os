/**
 * The noos-os wordmark: a geometric N, an infinity mark, a geometric S.
 *
 * Derived from `noos-svg.svg` at the repository root, with three changes:
 *
 *  - The background rectangle is gone. The installer already has its own
 *    surface, and a second one behind the mark would read as a pasted-in
 *    image rather than part of the interface.
 *  - Colours come from the design tokens instead of literals, so the mark
 *    cannot drift from the palette.
 *  - Gradient and filter ids are generated per instance. Two copies of the
 *    same hard-coded id would collide and the second would render wrong.
 *
 * The pulsing beam along the infinity is kept: it is the one piece of motion
 * in the interface that runs while nothing is happening, which is exactly what
 * the header needs.
 */

import { useId } from "react";

interface LogoProps {
  /** Rendered height in pixels; the wordmark is 3:1. */
  height?: number;
  /** Set false to freeze the beam, e.g. while the installation runs. */
  animated?: boolean;
  className?: string;
}

export function Logo({ height = 26, animated = true, className }: LogoProps) {
  const uid = useId().replace(/:/g, "");
  const gradient = `noos-gradient-${uid}`;
  const glow = `noos-glow-${uid}`;

  return (
    <svg
      viewBox="0 0 480 160"
      width={height * 3}
      height={height}
      className={className}
      role="img"
      aria-label="noos-os"
    >
      <defs>
        <linearGradient id={gradient} x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="var(--accent)" />
          <stop offset="50%" stopColor="#cba6f7" />
          <stop offset="100%" stopColor="#f38ba8" />
        </linearGradient>

        <filter id={glow} x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="3.5" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      {animated && (
        <style>{`
          @keyframes noos-orbit {
            from { stroke-dashoffset: 0; }
            to   { stroke-dashoffset: -440; }
          }
          .${uid}-beam {
            animation: noos-orbit 3.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
          }
          @media (prefers-reduced-motion: reduce) {
            .${uid}-beam { animation: none; }
          }
        `}</style>
      )}

      {/* N */}
      <path
        fill="var(--text)"
        d="M 80,40 L 96,40 L 134,98 L 134,40 L 150,40 L 150,120 L 134,120 L 96,62 L 96,120 L 80,120 Z"
      />

      <g transform="translate(140, 25)">
        {/* The rail the beam travels along. */}
        <path
          fill="none"
          stroke="var(--border-strong)"
          strokeWidth="13"
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M 105,55 C 135,15 180,15 180,55 C 180,95 135,95 105,55 C 75,15 30,15 30,55 C 30,95 75,95 105,55 Z"
        />

        <path
          className={animated ? `${uid}-beam` : undefined}
          fill="none"
          stroke={`url(#${gradient})`}
          strokeWidth="13"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeDasharray="110 330"
          filter={`url(#${glow})`}
          d="M 105,55 C 135,15 180,15 180,55 C 180,95 135,95 105,55 C 75,15 30,15 30,55 C 30,95 75,95 105,55 Z"
        />

        <path
          className={animated ? `${uid}-beam` : undefined}
          fill="none"
          stroke={`url(#${gradient})`}
          strokeWidth="13"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeDasharray="55 385"
          strokeDashoffset="-220"
          opacity="0.85"
          filter={`url(#${glow})`}
          d="M 105,55 C 135,15 180,15 180,55 C 180,95 135,95 105,55 C 75,15 30,15 30,55 C 30,95 75,95 105,55 Z"
        />
      </g>

      {/* S */}
      <path
        fill="var(--text)"
        d="M 403,58.5 L 386,58.5 C 386,52 380,48 373,48 C 363,48 357.5,53.5 357.5,59.5 C 357.5,66.5 363.5,70.5 376,74.5 L 381,76 C 397,81 405,87.5 405,99.5 C 405,113.5 393.5,122 373,122 C 354.5,122 341,114 340,99.5 L 357,99.5 C 358,107 364.5,111.5 373,111.5 C 383,111.5 388.5,106.5 388.5,99.5 C 388.5,92.5 382,88.5 369.5,84.5 L 364.5,83 C 348.5,78 341,71.5 341,59.5 C 341,46.5 353,38 373,38 C 390,38 402,46.5 403,58.5 Z"
      />
    </svg>
  );
}
