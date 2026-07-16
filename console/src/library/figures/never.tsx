"use client";

/* Four switch positions with no lever fitted — the refusals are structural,
   not settings. */

import { MAGENTA } from "../../design/theme";
import { dim } from "../primitives";
import { Frame, MONO } from "./frame";

export function NoLevers() {
  const rows = [0, 1, 2, 3];
  return (
    <Frame viewBox="0 0 300 190" label="Four switch positions with no lever fitted — the refusals are structural, not settings.">
      {rows.map((i) => {
        const y = 24 + i * 40;
        return (
          <g key={i}>
            <rect x={24} y={y} width={64} height={24} rx={12} fill="rgba(255,255,255,0.02)" stroke={dim(0.18)} strokeDasharray="3 3" />
            <line x1={40} y1={y + 5} x2={72} y2={y + 19} stroke={MAGENTA} strokeWidth={1.3} />
            <line x1={72} y1={y + 5} x2={40} y2={y + 19} stroke={MAGENTA} strokeWidth={1.3} />
            <text x={104} y={y + 16} fontSize={8.5} fill={dim(0.4)} fontFamily={MONO}>
              no lever fitted · not a setting
            </text>
          </g>
        );
      })}
    </Frame>
  );
}
