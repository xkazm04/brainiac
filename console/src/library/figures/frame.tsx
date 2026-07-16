"use client";

/*
 * Shared scaffolding for the /library concept drawings. The rule every figure
 * follows (inherited from /kb, which took it from the product itself): a
 * diagram is COMPILED from structure, never imagined — hand-authored geometry
 * illustrating a mechanism, nothing that could be mistaken for live data.
 * Where a drawing animates, it animates the mechanism and respects
 * prefers-reduced-motion.
 *
 * Visual grammar, shared with the console:
 *   theta blue — the Library's band (divergence → standardization work)
 *   gold       — the constructive path (ratify, adopt, the gate)
 *   mint       — verified / running today
 *   magenta    — the drift, the forbidden path
 */

import { dim } from "../primitives";

export const MONO = "var(--font-mono)";

/** The quiet surface every card illustration sits on. */
export function Frame({
  viewBox,
  label,
  children,
  minWidth = 0,
}: {
  viewBox: string;
  label: string;
  children: React.ReactNode;
  minWidth?: number;
}) {
  return (
    <div
      className="w-full overflow-x-auto rounded-lg border"
      style={{ borderColor: dim(0.08), background: "rgba(255,255,255,0.015)" }}
    >
      <svg
        viewBox={viewBox}
        role="img"
        aria-label={label}
        className="h-auto w-full"
        style={minWidth ? { minWidth } : undefined}
      >
        {children}
      </svg>
    </div>
  );
}

/** Sampled polyline path for a function — the same trick the home waves use. */
export const plot = (fn: (x: number) => number, x0: number, x1: number, step = 4) => {
  let d = "";
  for (let x = x0; x <= x1; x += step) {
    const y = fn(x);
    d += d === "" ? `M${x.toFixed(1)} ${y.toFixed(1)}` : ` L${x.toFixed(1)} ${y.toFixed(1)}`;
  }
  return d;
};
