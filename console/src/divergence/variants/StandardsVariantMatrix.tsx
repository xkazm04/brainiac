"use client";

/*
 * Standards — the interference pattern (prototype variant B).
 *
 * Two waves crossing produce a beat you cannot hear in either one alone. That is
 * what cross-team drift IS, and a column of cards can never show it: cards render
 * one divergence at a time, so the shape — which teams keep colliding, which
 * practices pulled in a third, where the clusters sit — stays invisible no matter
 * how well any single card is written. Sixty cards is sixty separate readings of
 * a field nobody ever sees.
 *
 * So: practices down, teams across, a filled cell where a team has an approach,
 * the cell's colour carrying impact. Totals ride both headers because the useful
 * reads here are marginal — the team that is in everything, the practice that
 * pulled in three. The clusters are the finding; the grid just stops hiding them.
 *
 * The field is a way IN, not a verdict. Selecting a cell opens the same argument
 * the shipped card makes — every team's approach, the recommendation as a
 * starting position a human ratifies, the adjudicating model named.
 *
 * A sparse field is the healthy one. Magenta stays rare because the sweep is
 * conservative; a grid dense with it would mean the detector had stopped being
 * worth reading. Density is a backlog, never a score — and an empty grid is the
 * good news, not a broken scan.
 */

import { useMemo, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";

import {
  band,
  type BandKey,
  BG,
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  MAGENTA_GLOW,
  PANEL,
  withAlpha,
} from "@/design/theme";
import type { PracticeDivergence, PracticeDivergences } from "@/lib/types";



type Approach = { team: string; approach: string };

/* `approaches` is `unknown` on the wire. Parsed, never trusted — a malformed
 * sweep result must cost one row its cells, not take the grid down. */
const readApproaches = (raw: unknown): Approach[] => {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((a): a is Approach => !!a && typeof a === "object")
    .map((a) => ({
      team: String((a as Approach).team ?? "—"),
      approach: String((a as Approach).approach ?? ""),
    }));
};

const IMPACTS = ["high", "medium", "low"] as const;
type Impact = (typeof IMPACTS)[number];

const laneOf = (impact: string): Impact =>
  impact === "high" ? "high" : impact === "medium" ? "medium" : "low";

type Hue = { line: string; wash: string; edge: string; glow?: string };

/* `band()` returns hsla, so alpha comes from the token rather than from the
 * withAlpha(accent, 0.08) hex-suffix trick, which only ever worked for MAGENTA. */
const hueOf = (key: BandKey, wash = 0.12): Hue => ({
  line: band(key),
  wash: band(key, 68, wash),
  edge: band(key, 68, 0.35),
});

/* The shipped intent, kept: high = MAGENTA (the alarm, reserved and glowing),
 * medium = GOLD, low = beta. */
const IMPACT_HUE: Record<Impact, Hue> = {
  high: { line: MAGENTA, wash: withAlpha(MAGENTA, 0.08), edge: MAGENTA, glow: MAGENTA_GLOW },
  medium: hueOf("gamma", 0.1),
  low: hueOf("beta", 0.08),
};

const TEAM_BANDS: BandKey[] = ["theta", "gamma", "beta", "delta", "alpha"];

/* Derived from the item, never the clock — a render that reads Date.now() tears
 * at hydration, and an unparseable stamp must not throw. */
const day = (iso: string): string => {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? "—" : new Date(t).toISOString().slice(0, 10);
};

type Row = {
  key: string;
  d: PracticeDivergence;
  approaches: Approach[];
  teams: string[];
  lane: Impact;
};

/* Sticky cells need an opaque ground or the rows scroll through them — but the
 * panel they freeze against is translucent, so the ground has to rebuild it:
 * BG, then PANEL, then any selection wash, layered rather than substituted. A
 * frozen column that is a shade off its own row reads as a seam. */
const stickyBg = (wash?: string) => {
  const layers = (wash ? [wash, PANEL] : [PANEL]).map((c) => `linear-gradient(${c}, ${c})`);
  return { background: BG, backgroundImage: layers.join(", ") };
};

function Argument({ row, teamHue }: { row: Row; teamHue: (t: string) => Hue }) {
  const { d } = row;
  const hue = IMPACT_HUE[row.lane];

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <span
          className={`${FONT_MONO} rounded-md px-2.5 py-1 text-[10px] uppercase tracking-[0.14em]`}
          style={{ color: hue.line, border: `1px solid ${hue.edge}`, background: hue.wash }}
        >
          {d.impact} impact
        </span>
      </div>

      <h3 className={`${FONT_DISPLAY} text-xl leading-tight`} style={{ color: INK }}>
        {d.practice}
      </h3>

      {d.summary && (
        <p className="text-sm leading-snug" style={{ color: INK_DIM }}>
          {d.summary}
        </p>
      )}

      {row.approaches.length > 0 ? (
        <div className="flex flex-col gap-px">
          {row.approaches.map((a, i) => {
            const th = teamHue(a.team);
            return (
              <div
                key={`${a.team}-${i}`}
                className="flex flex-col gap-2 p-3"
                style={{ background: "rgba(255,255,255,0.02)", border: `1px solid ${BORDER}` }}
              >
                <div className="flex items-center gap-2">
                  <span aria-hidden className="h-2 w-2 rounded-sm" style={{ background: th.line }} />
                  <span className={LABEL} style={{ color: th.line }}>
                    {a.team}
                  </span>
                </div>
                <p className={`${FONT_MONO} text-sm leading-relaxed`} style={{ color: INK }}>
                  {a.approach}
                </p>
              </div>
            );
          })}
        </div>
      ) : (
        <p className="text-sm" style={{ color: INK_FAINT }}>
          This divergence arrived without a readable per-team breakdown — the summary above is all
          the sweep recorded.
        </p>
      )}

      {d.recommended_standard && (
        <div
          className="flex flex-col gap-2 rounded-lg p-3"
          style={{ background: band("gamma", 68, 0.06), border: `1px solid ${band("gamma", 68, 0.25)}` }}
        >
          <span className={LABEL} style={{ color: GOLD }}>
            recommend
          </span>
          <p className={`${FONT_DISPLAY} text-base leading-snug`} style={{ color: INK }}>
            {d.recommended_standard}
          </p>
          <p className="text-sm leading-snug" style={{ color: INK_FAINT }}>
            A starting position, not a ruling — it is a lead&rsquo;s call to ratify, with these
            approaches in hand.
          </p>
        </div>
      )}

      <div
        className={`${FONT_MONO} flex flex-wrap justify-between gap-2 pt-3 text-[11px] uppercase tracking-[0.14em]`}
        style={{ borderTop: `1px solid ${BORDER}`, color: INK_FAINT }}
      >
        <span>adjudicated by {d.model_ref ?? "—"}</span>
        <span>{day(d.detected_at)}</span>
      </div>
    </div>
  );
}

export default function StandardsVariantMatrix({ data }: { data: PracticeDivergences }) {
  const reduce = !!useReducedMotion();
  const [sel, setSel] = useState<number>(0);

  const rows: Row[] = useMemo(
    () =>
      (data.divergences ?? []).map((d, i) => {
        const approaches = readApproaches(d.approaches);
        return {
          key: `${d.practice}-${i}`,
          d,
          approaches,
          teams: Array.from(new Set(approaches.map((a) => a.team))),
          lane: laneOf(d.impact),
        };
      }),
    [data],
  );

  const roster = useMemo(() => {
    const s = new Set<string>();
    for (const r of rows) for (const t of r.teams) s.add(t);
    return Array.from(s).sort();
  }, [rows]);

  const teamHue = useMemo(() => {
    const m = new Map(roster.map((t, i) => [t, hueOf(TEAM_BANDS[i % TEAM_BANDS.length])]));
    return (t: string) => m.get(t) ?? { line: INK_DIM, wash: "transparent", edge: BORDER };
  }, [roster]);

  /* The marginal read: how many divergences each team is caught in. The team at
     the top of this column is the one standardization has to go through. */
  const colTotal = useMemo(() => {
    const m = new Map<string, number>(roster.map((t) => [t, 0]));
    for (const r of rows) for (const t of r.teams) m.set(t, (m.get(t) ?? 0) + 1);
    return m;
  }, [rows, roster]);

  const highTotal = rows.filter((r) => r.lane === "high").length;
  const active = rows[sel] ?? null;

  if (rows.length === 0) {
    return (
      <main className="mx-auto flex max-w-6xl flex-col gap-8 px-6 py-12">
        <header className="flex flex-col gap-3">
          <span className={LABEL} style={{ color: INK_FAINT }}>
            standardization · interference pattern
          </span>
          <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
            Where teams solved the same problem different ways
          </h1>
        </header>
        <div
          className="flex flex-col gap-3 rounded-xl p-6"
          style={{ background: PANEL, border: `1px solid ${band("beta", 68, 0.3)}` }}
        >
          <span className={LABEL} style={{ color: band("beta") }}>
            in tune
          </span>
          <p className="max-w-2xl text-sm leading-snug" style={{ color: INK_DIM }}>
            No interference to plot — every shared practice is in tune. Either the last sweep found
            every cross-team cluster consistent, or no sweep has run yet. A flat field is the one
            you want: the detector is conservative on purpose, so an empty grid means there is
            nothing here worth a lead&rsquo;s afternoon.
          </p>
        </div>
      </main>
    );
  }

  return (
    <main className="mx-auto flex max-w-7xl flex-col gap-8 px-6 py-12">
      <header className="flex flex-col gap-3">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          standardization · interference pattern
        </span>
        <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
          Where teams solved the same problem different ways
        </h1>
        <p className="max-w-3xl text-sm leading-snug" style={{ color: INK_DIM }}>
          Every filled cell is one team&rsquo;s own answer to a shared problem, coloured by what the
          drift costs. Read down a column for the team standardization has to go through; read across
          a row for the practices that pulled in a third team. The sweep only surfaces genuine
          clusters — a sparse field is a healthy one, and density is a backlog, not a score.
        </p>
        <div className={`${FONT_MONO} flex flex-wrap items-center gap-x-5 gap-y-2 text-sm`}>
          <span style={{ color: INK_FAINT }}>
            {rows.length} practices × {roster.length} teams · {highTotal} high impact
          </span>
          {IMPACTS.map((k) => (
            <span key={k} className="flex items-center gap-1.5" style={{ color: INK_FAINT }}>
              <span
                aria-hidden
                className="h-2.5 w-2.5 rounded-[2px]"
                style={{
                  background: IMPACT_HUE[k].line,
                  boxShadow: IMPACT_HUE[k].glow ? `0 0 8px ${IMPACT_HUE[k].glow}` : undefined,
                }}
              />
              {k}
            </span>
          ))}
        </div>
      </header>

      <div className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_360px]">
        <motion.div
          initial={reduce ? false : { opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.3 }}
          className="overflow-auto rounded-xl"
          style={{ maxHeight: "72vh", background: PANEL, border: `1px solid ${BORDER}` }}
        >
          {/* border-separate: sticky cells drop their borders under
              border-collapse, which is exactly where the grid needs them. */}
          <table className="w-full" style={{ borderCollapse: "separate", borderSpacing: 0 }}>
            <caption className="sr-only">
              Practices by team. A filled cell means that team has its own approach to that practice;
              the cell&rsquo;s colour is the divergence&rsquo;s impact.
            </caption>
            <thead>
              <tr>
                <th
                  scope="col"
                  className="sticky left-0 top-0 z-30 px-3 py-2.5 text-left"
                  style={{ ...stickyBg(), borderBottom: `1px solid ${BORDER}`, minWidth: "220px" }}
                >
                  <span className={LABEL} style={{ color: INK_FAINT }}>
                    practice
                  </span>
                </th>
                {roster.map((t) => {
                  const th = teamHue(t);
                  const lit = !!active?.teams.includes(t);
                  return (
                    <th
                      key={t}
                      scope="col"
                      className="sticky top-0 z-20 px-2 py-2.5"
                      style={{ ...stickyBg(), borderBottom: `1px solid ${BORDER}`, minWidth: "76px" }}
                    >
                      <div className="flex flex-col items-center gap-1">
                        <span
                          className={`${FONT_MONO} text-[10px] uppercase tracking-[0.14em] transition`}
                          style={{ color: lit ? th.line : INK_FAINT }}
                        >
                          {t}
                        </span>
                        <span className={`${FONT_MONO} text-sm`} style={{ color: lit ? INK : INK_FAINT }}>
                          {colTotal.get(t) ?? 0}
                        </span>
                      </div>
                    </th>
                  );
                })}
                <th
                  scope="col"
                  className="sticky top-0 z-20 px-3 py-2.5"
                  style={{ ...stickyBg(), borderBottom: `1px solid ${BORDER}` }}
                >
                  <span className={LABEL} style={{ color: INK_FAINT }}>
                    teams
                  </span>
                </th>
              </tr>
            </thead>

            <tbody>
              {rows.map((r, i) => {
                const on = i === sel;
                const hue = IMPACT_HUE[r.lane];
                const wash = on ? "rgba(255,255,255,0.06)" : undefined;
                return (
                  <tr key={r.key}>
                    <th
                      scope="row"
                      className="sticky left-0 z-10 p-0 text-left font-normal"
                      style={{ ...stickyBg(wash), borderBottom: `1px solid ${BORDER}` }}
                    >
                      <button
                        type="button"
                        onClick={() => setSel(i)}
                        className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition hover:bg-white/[0.04]"
                        title={r.d.practice}
                      >
                        {/* The lane read survives a horizontal scroll, which is
                            the one place the cells' colour goes off-screen. */}
                        <span
                          aria-hidden
                          className="h-3.5 w-[3px] shrink-0 rounded-full"
                          style={{
                            background: hue.line,
                            boxShadow: hue.glow ? `0 0 8px ${hue.glow}` : undefined,
                          }}
                        />
                        <span
                          className="truncate text-sm"
                          style={{ color: on ? INK : INK_DIM }}
                        >
                          {r.d.practice}
                        </span>
                      </button>
                    </th>

                    {roster.map((t) => {
                      const has = r.teams.includes(t);
                      return (
                        <td
                          key={t}
                          className="p-0 text-center"
                          style={{
                            borderBottom: `1px solid ${BORDER}`,
                            background: on ? "rgba(255,255,255,0.06)" : undefined,
                          }}
                        >
                          {has ? (
                            <button
                              type="button"
                              onClick={() => setSel(i)}
                              aria-label={`${r.d.practice} — ${t}'s approach, ${r.d.impact} impact`}
                              className="flex h-full w-full items-center justify-center px-2 py-1.5 transition hover:bg-white/[0.05]"
                            >
                              <span
                                aria-hidden
                                className="h-3.5 w-3.5 rounded-[3px] transition"
                                style={{
                                  background: hue.line,
                                  opacity: on ? 1 : 0.78,
                                  boxShadow: hue.glow ? `0 0 9px ${hue.glow}` : undefined,
                                }}
                              />
                            </button>
                          ) : (
                            <span
                              aria-hidden
                              className="mx-auto block h-1 w-1 rounded-full"
                              style={{ background: "rgba(233,237,255,0.12)" }}
                            />
                          )}
                        </td>
                      );
                    })}

                    {/* Three teams is the shape a two-column card cannot hold —
                        so it is the number that earns full-strength ink. */}
                    <td
                      className={`${FONT_MONO} px-3 text-center text-sm`}
                      style={{
                        borderBottom: `1px solid ${BORDER}`,
                        background: on ? "rgba(255,255,255,0.06)" : undefined,
                        color: r.teams.length >= 3 ? INK : INK_FAINT,
                      }}
                    >
                      {r.teams.length}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </motion.div>

        <aside
          className="rounded-xl p-5 lg:sticky lg:top-6"
          style={{ background: PANEL, border: `1px solid ${BORDER}` }}
        >
          <AnimatePresence mode="wait">
            {active ? (
              <motion.div
                key={active.key}
                initial={reduce ? false : { opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                transition={{ duration: reduce ? 0 : 0.18 }}
              >
                <Argument row={active} teamHue={teamHue} />
              </motion.div>
            ) : (
              <p key="prompt" className="text-sm leading-snug" style={{ color: INK_DIM }}>
                Pick a cell to read the argument behind it.
              </p>
            )}
          </AnimatePresence>
        </aside>
      </div>
    </main>
  );
}
