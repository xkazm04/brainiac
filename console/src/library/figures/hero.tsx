"use client";

/* The hero drawing: two team practices leaving the written standard, slowly —
   and the gap at year's end that nothing was measuring. */

import { MAGENTA } from "../../design/theme";
import { THETA, dim } from "../primitives";
import { Frame, MONO, plot } from "./frame";

export function DriftBeat() {
  const w = 520;
  const REF = 92; // the written standard — a flat reference pitch
  // Two practices: in tune with the reference in January, detuning over time.
  // Amplitude of the deviation grows with x — locally reasonable at each step,
  // unmistakable in aggregate. Deterministic; the drift IS the geometry.
  const teamA = (x: number) => REF - (x / w) ** 1.7 * 52 + Math.sin(x / 26) * 3.5;
  const teamB = (x: number) => REF + (x / w) ** 1.6 * 64 + Math.sin(x / 31 + 2) * 3.5;
  return (
    <Frame viewBox="0 0 520 232" label="Two team practices drifting away from the written standard over a year, with nothing measuring the gap." minWidth={430}>
      {/* the written standard: a reference line, dashed — a wish, not a measure */}
      <line x1={16} y1={REF} x2={w - 60} y2={REF} stroke={dim(0.3)} strokeWidth={1.2} strokeDasharray="5 5" />
      <text x={18} y={REF - 8} fontSize={9} fill={dim(0.5)} fontFamily={MONO} letterSpacing="1">
        the written standard
      </text>

      {/* the two practices */}
      <path d={plot(teamA, 16, w - 60)} fill="none" stroke={THETA} strokeWidth={1.8} />
      <path d={plot(teamB, 16, w - 60)} fill="none" stroke={THETA} strokeWidth={1.8} opacity={0.75} />
      <text x={w - 54} y={teamA(w - 60) + 3} fontSize={9} fill={THETA} fontFamily={MONO}>
        team a
      </text>
      <text x={w - 54} y={teamB(w - 60) + 3} fontSize={9} fill={THETA} fontFamily={MONO} opacity={0.75}>
        team b
      </text>

      {/* the gap nobody measured */}
      <path
        d={`${plot(teamA, w * 0.72, w - 60)} ${plot(teamB, w - 60, w * 0.72, -4).replace("M", "L")} Z`}
        fill="rgba(255,93,162,0.08)"
        stroke="none"
      />
      <line
        x1={w - 78}
        y1={teamA(w - 78)}
        x2={w - 78}
        y2={teamB(w - 78)}
        stroke={MAGENTA}
        strokeWidth={1.2}
        strokeDasharray="3 3"
      />
      <text x={w - 190} y={214} fontSize={9} fill={MAGENTA} fontFamily={MONO} letterSpacing="1">
        the gap nobody measured
      </text>

      {/* time axis */}
      <line x1={16} y1={196} x2={w - 60} y2={196} stroke={dim(0.14)} strokeWidth={1} />
      <text x={16} y={212} fontSize={9} fill={dim(0.4)} fontFamily={MONO}>
        january — everyone read the guide
      </text>
    </Frame>
  );
}
