"use client";

/*
 * The decay curve — the one chart the whole page argues.
 *
 * Two lines over a year of a page's life. Truth changes four times. The
 * hand-written page loses a slab of accuracy at every change nobody remembered
 * to copy in, and never gets it back. The compiled page takes the same hits —
 * and recomposes, every time, because the change itself is what triggers the
 * rebuild.
 *
 * HONESTY NOTE, load-bearing: this is a SCHEMATIC of the mechanism, with
 * authored data — not a benchmark, and the caption under it says so. A page
 * whose whole argument is "don't dress intent as fact" does not get to dress
 * an illustration as a measurement.
 *
 * Series colours are validated for the dark surface (lightness band + CVD
 * check); the deutan ΔE sits in the floor band, so identity never rides on
 * colour alone — the lines carry direct end-labels and opposite shapes
 * (staircase decay vs. flat-with-heal-spikes).
 */

import { useReducedMotion } from "framer-motion";
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { FONT_MONO } from "../design/theme";

const dim = (a: number) => `rgba(233,237,255,${a})`;

/* Validated against the page surface (#08070c): inside the dark lightness
   band, contrast ≥ 3:1. Deliberately deeper than the UI accent hues — these
   are area-covering marks, not chrome. */
const PROJ = "#2fae79";
const WIKI = "#e0568f";

/** Weeks at which the underlying truth changes. */
const EVENTS = [8, 18, 30, 40];

interface Point {
  week: number;
  wiki: number;
  proj: number;
}

const DATA: Point[] = Array.from({ length: 49 }, (_, week) => {
  const hits = EVENTS.filter((e) => week >= e).length;
  return {
    week,
    // the hand-written page: each change strands another slab of it
    wiki: 100 - hits * 14,
    // the projection: the change itself triggers the rebuild — a one-week dip
    proj: EVENTS.includes(week) ? 88 : 100,
  };
});

const LAST = DATA.length - 1;

function endLabel(text: string, color: string, dy = 3) {
  // recharts hands label callbacks x/y as string | number — coerce before math.
  return function EndLabel(props: { x?: number | string; y?: number | string; index?: number }) {
    if (props.index !== LAST || props.x == null || props.y == null) return null;
    return (
      <text x={Number(props.x) + 8} y={Number(props.y) + dy} fontSize={10} fill={color} fontFamily="var(--font-mono)">
        {text}
      </text>
    );
  };
}

function ChartTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: { dataKey: string; value: number; stroke: string }[];
  label?: number;
}) {
  if (!active || !payload?.length || label == null) return null;
  const months = (label / 4).toFixed(1).replace(/\.0$/, "");
  const by = Object.fromEntries(payload.map((p) => [p.dataKey, p.value]));
  return (
    <div
      className={`${FONT_MONO} rounded-md border px-3 py-2 text-[11px] leading-relaxed`}
      style={{ background: "#0b0a12", borderColor: dim(0.15), color: dim(0.7) }}
    >
      <div style={{ color: dim(0.45) }}>month {months}</div>
      <div style={{ color: PROJ }}>compiled page · {by.proj}% true</div>
      <div style={{ color: WIKI }}>written page · {by.wiki}% true</div>
    </div>
  );
}

export default function RotCurve() {
  const reduce = !!useReducedMotion();
  return (
    <div>
      {/* legend — identity in text tokens, colour only on the swatch */}
      <div className={`${FONT_MONO} flex flex-wrap items-center gap-x-6 gap-y-2 text-[11px]`} style={{ color: dim(0.6) }}>
        <span className="inline-flex items-center gap-2">
          <span className="h-[3px] w-5 rounded-full" style={{ background: PROJ }} />
          compiled page — the change is what triggers the rebuild
        </span>
        <span className="inline-flex items-center gap-2">
          <span
            className="h-0 w-5 border-t-2 border-dashed"
            style={{ borderColor: WIKI }}
          />
          hand-written page — decays until someone notices
        </span>
      </div>

      <div className="mt-4 h-[240px] w-full">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={DATA} margin={{ top: 12, right: 118, bottom: 4, left: 0 }}>
            <CartesianGrid vertical={false} stroke={dim(0.06)} />
            {EVENTS.map((e, i) => (
              <ReferenceLine
                key={e}
                x={e}
                stroke={dim(0.12)}
                strokeDasharray="2 4"
                label={
                  i === 0
                    ? { value: "the truth changes", position: "insideTopLeft", fill: dim(0.35), fontSize: 9, fontFamily: "var(--font-mono)" }
                    : undefined
                }
              />
            ))}
            <XAxis
              dataKey="week"
              ticks={[0, 12, 24, 36, 48]}
              tickFormatter={(w: number) => (w === 0 ? "day 1" : `${w / 4} mo`)}
              tick={{ fontSize: 10, fill: dim(0.4), fontFamily: "var(--font-mono)" }}
              axisLine={{ stroke: dim(0.12) }}
              tickLine={false}
            />
            <YAxis
              domain={[0, 106]}
              ticks={[0, 50, 100]}
              tickFormatter={(v: number) => `${v}%`}
              width={42}
              tick={{ fontSize: 10, fill: dim(0.4), fontFamily: "var(--font-mono)" }}
              axisLine={false}
              tickLine={false}
            />
            <Tooltip content={<ChartTooltip />} cursor={{ stroke: dim(0.15), strokeDasharray: "3 3" }} />
            <Line
              type="stepAfter"
              dataKey="wiki"
              stroke={WIKI}
              strokeWidth={2}
              strokeDasharray="5 4"
              dot={false}
              activeDot={{ r: 3, fill: WIKI, strokeWidth: 0 }}
              isAnimationActive={!reduce}
              label={endLabel("written", WIKI)}
            />
            <Line
              type="monotone"
              dataKey="proj"
              stroke={PROJ}
              strokeWidth={2}
              dot={false}
              activeDot={{ r: 3, fill: PROJ, strokeWidth: 0 }}
              isAnimationActive={!reduce}
              label={endLabel("compiled", PROJ, -4)}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>

      <p className={`${FONT_MONO} mt-2 text-[10px]`} style={{ color: dim(0.32) }}>
        schematic — this draws the mechanism, it is not a measurement. % of the page still true, over one
        year, four changes to the underlying truth.
      </p>
    </div>
  );
}
