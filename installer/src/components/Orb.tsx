/**
 * The waiting indicator shown while the installation runs.
 *
 * Drawn on a canvas by hand rather than taken from a component library. It is
 * about sixty lines, adds no dependency, and — more to the point — a bespoke
 * indicator can be tuned to the interface's own accent colour and made as
 * quiet as the rest of the design. Library orbs tend to be busier than a
 * full-screen installer wants.
 *
 * No WebGL: on a live ISO with software rendering that is a real risk, and a
 * plain 2D canvas costs nothing.
 */

import { useEffect, useRef } from "react";

interface OrbProps {
  size?: number;
  /** Set false to freeze the animation, e.g. once the install has finished. */
  active?: boolean;
}

const DOT_COUNT = 11;
const TAU = Math.PI * 2;

export function Orb({ size = 44, active = true }: OrbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const context = canvas.getContext("2d");
    if (!context) return;

    // Render at device resolution so the dots stay crisp on HiDPI panels.
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    context.scale(dpr, dpr);

    // Take the accent from the design tokens rather than hard-coding it, so
    // the orb can never drift from the rest of the palette.
    const accent =
      getComputedStyle(document.documentElement)
        .getPropertyValue("--accent")
        .trim() || "#8ab4f8";

    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    let frame = 0;
    let elapsed = 0;
    const centre = size / 2;
    const orbit = size * 0.31;

    const draw = () => {
      context.clearRect(0, 0, size, size);

      for (let index = 0; index < DOT_COUNT; index += 1) {
        const phase = (index / DOT_COUNT) * TAU;

        // Each dot breathes on its own offset, which is what stops the ring
        // from reading as a solid spinning shape.
        const breath = Math.sin(elapsed * 1.7 + phase * 2);
        const radius = orbit + breath * size * 0.045;
        const angle = phase + elapsed * 0.55;

        const x = centre + Math.cos(angle) * radius;
        const y = centre + Math.sin(angle) * radius;

        // Trailing dots fade out, giving the ring a direction.
        const opacity = 0.18 + 0.72 * (0.5 + 0.5 * breath);

        context.beginPath();
        context.arc(x, y, size * 0.042, 0, TAU);
        context.fillStyle = `color-mix(in srgb, ${accent} ${Math.round(opacity * 100)}%, transparent)`;
        context.fill();
      }

      if (active && !still) {
        elapsed += 0.016;
        frame = requestAnimationFrame(draw);
      }
    };

    draw();

    return () => cancelAnimationFrame(frame);
  }, [size, active]);

  return (
    <canvas
      ref={canvasRef}
      className="orb"
      style={{ width: size, height: size }}
      aria-hidden="true"
    />
  );
}
