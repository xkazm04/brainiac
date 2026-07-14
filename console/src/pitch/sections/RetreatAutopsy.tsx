"use client";

/*
 * RETREAT — "the autopsy". Won the section prototype round against a trajectory
 * chart, then consolidated for compactness.
 *
 * ONE instrument, not four stacked components. The round-2 draft had a lane
 * chart, a separate cause-of-death bar, a separate row of vendor tabs, and a
 * detail card — which meant switching a vendor changed something you had to
 * scroll to see. The whole point of clicking a vendor is watching its impact
 * land, so everything now sits inside a single bordered panel:
 *
 *   - THE LANES ARE THE SELECTOR. A separate tab row was duplicating a control
 *     that already existed: the lanes were clickable all along.
 *   - CAUSE OF DEATH folds into the detail header, where it belongs — it is a
 *     property of the selected vendor, not a standing banner.
 *   - The detail is sized so the chart and the switched content are visible
 *     together in one eyeful.
 *
 * HONESTY NOTE on the lanes: these are lifespans of *learned* memory features.
 * Cursor and Windsurf did not ship regressions when they killed theirs; they
 * retreated to human-authored rules. That is the real shape of the story, and
 * the "kept instead" column says so rather than implying four companies simply
 * got worse.
 */

import { useState } from "react";
import { motion } from "framer-motion";

import { band, FONT_MONO, LABEL, MAGENTA } from "../../design/theme";
import { RETREAT } from "../pitch-data";

const GOLD = band("gamma");
const dim = (a: number) => `rgba(233,237,255,${a})`;

interface VendorArc {
  who: string;
  /** x positions on the shared 2025 → mid-2026 axis (0–1). */
  start: number;
  end: number;
  deathLabel: string;
  born: string;
  had: string;
  kept: string;
  cause: string;
}

const ARCS: VendorArc[] = [
  {
    who: "Cursor",
    start: 0.04,
    end: 0.62,
    deathLabel: "v2.1.17",
    born: "Memories · v1.0 beta",
    had: "implicit memory, auto-learned",
    kept: "Team Rules, written by a human",
    cause: "Never left beta. Removed with no changelog entry.",
  },
  {
    who: "Windsurf",
    start: 0.12,
    end: 0.86,
    deathLabel: "1 Jul 2026",
    born: "Cascade memories",
    had: "auto-generated Memories",
    kept: "Rules files, and a doc saying prefer them",
    cause: "Its own docs told you not to rely on it. Then Cascade was retired.",
  },
  {
    who: "OpenAI",
    start: 0.0,
    end: 0.78,
    deathLabel: "Jun 2026",
    born: "editable memory list",
    had: "an enumerable list you could inspect and edit",
    kept: "opaque background synthesis",
    cause: "Recall went up. Auditability went to zero.",
  },
  {
    who: "Mem0",
    start: 0.08,
    end: 0.55,
    deathLabel: "Apr 2026",
    born: "ADD · UPDATE · DELETE",
    had: "contradiction handling",
    kept: "ADD only. Both claims live, the ranker decides.",
    cause: "Traded correctness for latency, in one commit.",
  },
];

const LW = 900;
const LANE = 34;
const LEFT = 92;
const TOP = 26;

export function RetreatAutopsy() {
  const [active, setActive] = useState(ARCS[0].who);
  const arc = ARCS.find((a) => a.who === active)!;
  const detail = RETREAT.find((r) => r.who === active)!;

  const x = (t: number) => LEFT + t * (LW - LEFT - 24);

  return (
    <motion.div
      initial={{ opacity: 0, y: 14 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, amount: 0.15 }}
      transition={{ duration: 0.45 }}
      className="overflow-hidden rounded-xl border"
      style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
    >
      {/* ── the lanes: the chart AND the selector ───────────────────────────── */}
      <div className="px-5 pt-5">
        <svg
          viewBox={`0 0 ${LW} ${TOP + ARCS.length * LANE + 8}`}
          className="w-full"
          role="group"
          aria-label="Four vendors' learned-memory features on a shared timeline. Each bar is a lifespan ending in the moment the feature was killed. Select a vendor to see its autopsy."
        >
          <text x={LEFT} y={10} fill={dim(0.3)} fontSize={10} fontFamily="var(--font-mono)">
            2025
          </text>
          <text
            x={LW - 24}
            y={10}
            fill={dim(0.3)}
            fontSize={10}
            textAnchor="end"
            fontFamily="var(--font-mono)"
          >
            mid-2026
          </text>
          <line x1={LEFT} y1={17} x2={LW - 24} y2={17} stroke={dim(0.12)} strokeWidth={1} />

          {ARCS.map((a, i) => {
            const y = TOP + i * LANE + 12;
            const on = a.who === active;
            return (
              <g
                key={a.who}
                onClick={() => setActive(a.who)}
                style={{ cursor: "pointer" }}
                role="button"
                tabIndex={0}
                aria-pressed={on}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") setActive(a.who);
                }}
              >
                {/* the whole lane is the hit target */}
                <rect
                  x={0}
                  y={y - 14}
                  width={LW}
                  height={LANE - 4}
                  rx={5}
                  fill={on ? "hsla(46,90%,60%,0.06)" : "transparent"}
                />

                <text
                  x={10}
                  y={y + 4}
                  fill={on ? "#fff" : dim(0.5)}
                  fontSize={12}
                  fontWeight={on ? 600 : 400}
                  fontFamily="var(--font-mono)"
                >
                  {a.who}
                </text>

                <line x1={LEFT} y1={y} x2={LW - 24} y2={y} stroke={dim(0.06)} strokeWidth={1} />

                <motion.line
                  initial={{ pathLength: 0 }}
                  whileInView={{ pathLength: 1 }}
                  viewport={{ once: true }}
                  transition={{ duration: 0.7, delay: 0.1 * i }}
                  x1={x(a.start)}
                  y1={y}
                  x2={x(a.end)}
                  y2={y}
                  stroke={on ? GOLD : "hsla(46,80%,65%,0.3)"}
                  strokeWidth={on ? 4 : 2.5}
                  strokeLinecap="round"
                />
                <circle cx={x(a.start)} cy={y} r={3.2} fill={on ? GOLD : dim(0.3)} />
                <text
                  x={x(a.start) + 8}
                  y={y - 8}
                  fill={dim(on ? 0.55 : 0.28)}
                  fontSize={9}
                  fontFamily="var(--font-mono)"
                >
                  {a.born}
                </text>

                <motion.g
                  initial={{ opacity: 0, scale: 0.5 }}
                  whileInView={{ opacity: 1, scale: 1 }}
                  viewport={{ once: true }}
                  transition={{ delay: 0.7 + 0.1 * i, duration: 0.28 }}
                  style={{ transformOrigin: `${x(a.end)}px ${y}px` }}
                >
                  <line x1={x(a.end) - 5.5} y1={y - 5.5} x2={x(a.end) + 5.5} y2={y + 5.5} stroke={MAGENTA} strokeWidth={on ? 2.4 : 1.5} />
                  <line x1={x(a.end) + 5.5} y1={y - 5.5} x2={x(a.end) - 5.5} y2={y + 5.5} stroke={MAGENTA} strokeWidth={on ? 2.4 : 1.5} />
                  <text
                    x={x(a.end) + 12}
                    y={y + 4}
                    fill={on ? MAGENTA : dim(0.32)}
                    fontSize={9.5}
                    fontFamily="var(--font-mono)"
                  >
                    {a.deathLabel}
                  </text>
                </motion.g>
              </g>
            );
          })}
        </svg>

        <p className={`${FONT_MONO} pb-4 text-xs`} style={{ color: dim(0.32) }}>
          four features, four graves · select a lane
        </p>
      </div>

      {/* ── the autopsy: everything about the selected vendor, in one block ─── */}
      <motion.div
        key={active}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.25 }}
        className="border-t px-6 py-5"
        style={{ borderColor: "rgba(233,237,255,0.08)", background: "rgba(0,0,0,0.22)" }}
      >
        {/* header + cause of death, consolidated onto one line */}
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h3 className="text-lg font-semibold tracking-tight text-white">{detail.who}</h3>
          <span className={`${FONT_MONO} text-sm`} style={{ color: MAGENTA }}>
            {detail.what}
          </span>
          <span className={LABEL} style={{ color: dim(0.3) }}>
            {detail.when}
          </span>
          <span className="ml-auto text-sm" style={{ color: dim(0.75) }}>
            {arc.cause}
          </span>
        </div>

        {/* the illustrated downfall */}
        <div className="mt-4 grid items-stretch gap-2 md:grid-cols-[1fr_36px_1fr]">
          <div
            className="rounded-lg border px-4 py-3"
            style={{ borderColor: "rgba(255,93,162,0.32)", background: "rgba(255,93,162,0.05)" }}
          >
            <div className={LABEL} style={{ color: MAGENTA }}>
              removed
            </div>
            <p className={`${FONT_MONO} mt-1.5 text-sm`} style={{ color: "rgba(255,150,195,0.95)" }}>
              <span className="mr-2 select-none opacity-60">−</span>
              <span className="line-through decoration-1">{arc.had}</span>
            </p>
          </div>

          <div className="flex items-center justify-center">
            <svg viewBox="0 0 36 20" className="w-9" aria-hidden>
              <line x1={2} y1={10} x2={26} y2={10} stroke={dim(0.22)} strokeWidth={1.2} strokeDasharray="3 3" />
              <path d="M24 6 L32 10 L24 14 Z" fill={dim(0.28)} />
            </svg>
          </div>

          <div
            className="rounded-lg border px-4 py-3"
            style={{ borderColor: "hsla(46,90%,68%,0.32)", background: "hsla(46,90%,60%,0.06)" }}
          >
            <div className={LABEL} style={{ color: GOLD }}>
              kept instead
            </div>
            <p className={`${FONT_MONO} mt-1.5 text-sm`} style={{ color: "hsla(46,85%,80%,0.95)" }}>
              <span className="mr-2 select-none opacity-60">+</span>
              {arc.kept}
            </p>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap items-end justify-between gap-3">
          <p className="max-w-3xl text-sm leading-relaxed" style={{ color: dim(0.6) }}>
            {detail.detail}
          </p>
          <a
            href={detail.cite.href}
            target="_blank"
            rel="noreferrer noopener"
            className={`${FONT_MONO} shrink-0 text-xs underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
            style={{ color: dim(0.38) }}
          >
            {detail.cite.label} ↗
          </a>
        </div>
      </motion.div>
    </motion.div>
  );
}
