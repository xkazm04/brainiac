"use client";

/* The life of a rule as six stations on one rail. Solid where built, dashed
   where planned; the faint loop back over the top is retirement feeding
   detection again. */

import { dim, MINT } from "../primitives";
import { MONO } from "./frame";

export function LifeRail({ statuses }: { statuses: ("shipped" | "in_progress" | "roadmap")[] }) {
  const N = statuses.length;
  const w = 640;
  const y = 34;
  const xs = Array.from({ length: N }, (_, i) => 40 + (i * (w - 80)) / (N - 1));
  return (
    <svg viewBox={`0 0 ${w} 68`} role="img" aria-label="The life of a rule as six stations on one rail; the first runs today, the rest are the plan." className="h-auto w-full" style={{ minWidth: 480 }}>
      {/* the rail: solid where built, dashed where planned */}
      <line x1={xs[0]} y1={y} x2={xs[0] + (xs[1] - xs[0]) / 2} y2={y} stroke={MINT} strokeWidth={1.5} />
      <line x1={xs[0] + (xs[1] - xs[0]) / 2} y1={y} x2={xs[N - 1]} y2={y} stroke={dim(0.22)} strokeWidth={1.2} strokeDasharray="4 5" />
      {/* the loop back: retire feeds detection again */}
      <path d={`M${xs[N - 1]},${y} C ${xs[N - 1] + 26},${y} ${xs[N - 1] + 26},${y - 26} ${xs[N - 1] - 20},${y - 26} L${xs[0] + 20},${y - 26} C ${xs[0] - 26},${y - 26} ${xs[0] - 26},${y} ${xs[0]},${y}`} fill="none" stroke={dim(0.14)} strokeWidth={1} strokeDasharray="2 5" />
      {xs.map((x, i) => {
        const s = statuses[i];
        const tone = s === "shipped" ? MINT : dim(0.45);
        return (
          <g key={x}>
            <circle cx={x} cy={y} r={11} fill="#08070c" stroke={tone} strokeWidth={1.4} strokeDasharray={s === "roadmap" ? "3 3" : undefined} />
            <text x={x} y={y + 3.5} fontSize={9} textAnchor="middle" fill={tone} fontFamily={MONO}>
              {String(i + 1).padStart(2, "0")}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
