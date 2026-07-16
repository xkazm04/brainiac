"use client";

/*
 * Standards — the triage bench (prototype variant A).
 *
 * The shipped column asks a lead to read sixty arguments to find the three that
 * matter. The bench promotes impact from a badge to the LANE, so the work sorts
 * itself: the loud lane is unmissable and the quiet one stays quiet without
 * hiding. Rows collapse to one line because the practice plus who is involved is
 * enough to decide whether to open the argument — and opening one costs no
 * scroll position, so a lead can work top-down without losing their place.
 *
 * The card's stance survives the compression: an opened row still ends in an
 * argument, not a verdict. Every team's approach, the recommendation as a
 * starting position, and the adjudicating model named — because a lead ratifies
 * with the provenance in front of them, or the surface has decided for them.
 *
 * Two honesties the layout must not erase. Volume is not value: the lanes count,
 * they do not score, and "high" stays rare because the sweep is conservative. And
 * an empty board is the healthy end state — so it reads as good news, and stays
 * distinct from a filter that merely matched nothing, which is a different fact
 * about the filter rather than about the org.
 */

import { useMemo, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";

import {
  band,
  type BandKey,
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
 * sweep result must cost a row its detail, not take the board down. */
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

/* Anything the adjudicator did not call high or medium reads as low, matching
 * the shipped card's fallback rather than inventing a fourth state. */
const laneOf = (impact: string): Impact =>
  impact === "high" ? "high" : impact === "medium" ? "medium" : "low";

/* A hue always travels with its own wash and edge: `band()` returns hsla, so the
 * shipped card's withAlpha(accent, 0.08) hex-suffix trick would emit invalid CSS for
 * anything but MAGENTA. Alpha comes from the token, not from string surgery. */
type Hue = { line: string; wash: string; edge: string; glow?: string };

const hueOf = (key: BandKey, wash = 0.12): Hue => ({
  line: band(key),
  wash: band(key, 68, wash),
  edge: band(key, 68, 0.35),
});

const NEUTRAL_HUE: Hue = {
  line: INK,
  wash: "rgba(233,237,255,0.08)",
  edge: "rgba(233,237,255,0.30)",
};

/*
 * The shipped accent intent, kept: high = MAGENTA (the alarm, reserved), medium
 * = GOLD (gamma — the constructive band), low = beta. Only high carries a glow;
 * an alarm that everything shares is not an alarm.
 */
const IMPACT_ACCENT: Record<Impact, Hue> = {
  high: { line: MAGENTA, wash: withAlpha(MAGENTA, 0.08), edge: MAGENTA, glow: MAGENTA_GLOW },
  medium: hueOf("gamma", 0.1),
  low: hueOf("beta", 0.08),
};

const LANE_NOTE: Record<Impact, string> = {
  high: "rare by design — read these first",
  medium: "worth a decision this quarter",
  low: "safe to batch",
};

/* Colour is per TEAM, not per position in a card, so a team keeps one hue
 * everywhere on the board and the eye can track it down a lane. */
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
  haystack: string;
};

function Chip({
  label,
  hue,
  on,
  onClick,
}: {
  label: string;
  hue: Hue;
  on: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`${FONT_MONO} rounded-md px-2.5 py-1 text-[10px] uppercase tracking-[0.14em] transition`}
      style={{
        color: on ? hue.line : INK_FAINT,
        border: `1px solid ${on ? hue.edge : BORDER}`,
        background: on ? hue.wash : "transparent",
      }}
    >
      {label}
    </button>
  );
}

function BenchRow({
  row,
  open,
  onToggle,
  teamHue,
  reduce,
}: {
  row: Row;
  open: boolean;
  onToggle: () => void;
  teamHue: (t: string) => Hue;
  reduce: boolean;
}) {
  const accent = IMPACT_ACCENT[row.lane];
  const { d } = row;

  return (
    <div style={{ borderTop: `1px solid ${BORDER}` }}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className="flex w-full items-center gap-3 px-4 py-2.5 text-left transition hover:bg-white/[0.03]"
        style={{ background: open ? accent.wash : "transparent" }}
      >
        <span
          aria-hidden
          className="h-4 w-[3px] shrink-0 rounded-full"
          style={{ background: accent.line, boxShadow: accent.glow ? `0 0 8px ${accent.glow}` : undefined }}
        />
        <span className={`${FONT_DISPLAY} shrink-0 text-sm`} style={{ color: INK }}>
          {d.practice}
        </span>

        <span className="flex shrink-0 flex-wrap gap-1">
          {row.teams.map((t) => {
            const th = teamHue(t);
            return (
              <span
                key={t}
                className={`${FONT_MONO} rounded px-1.5 py-0.5 text-[10px] uppercase tracking-[0.14em]`}
                style={{ color: th.line, border: `1px solid ${th.edge}` }}
              >
                {t}
              </span>
            );
          })}
        </span>

        {/* The gist, not the argument — enough to skip a row without opening it. */}
        <span className="hidden min-w-0 flex-1 truncate text-sm md:block" style={{ color: INK_FAINT }}>
          {d.recommended_standard}
        </span>

        <span
          aria-hidden
          className={`${FONT_MONO} ml-auto shrink-0 text-sm md:ml-0`}
          style={{ color: INK_FAINT }}
        >
          {open ? "−" : "+"}
        </span>
      </button>

      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: reduce ? 0 : 0.22, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="flex flex-col gap-5 px-4 pb-5 pt-1" style={{ background: accent.wash }}>
              {d.summary && (
                <p className="max-w-3xl text-sm leading-snug" style={{ color: INK_DIM }}>
                  {d.summary}
                </p>
              )}

              {/* Auto-fitting, because a third team's approach is common enough
                  that a two-column grid is a bug, not an edge case. */}
              {row.approaches.length > 0 ? (
                <div className="grid gap-px sm:grid-cols-2 lg:grid-cols-3">
                  {row.approaches.map((a, i) => {
                    const th = teamHue(a.team);
                    return (
                      <div
                        key={`${a.team}-${i}`}
                        className="flex flex-col gap-2 p-4"
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
                  This divergence arrived without a readable per-team breakdown — the summary above is
                  all the sweep recorded.
                </p>
              )}

              {d.recommended_standard && (
                <div
                  className="flex items-start gap-3 rounded-lg p-4"
                  style={{ background: band("gamma", 68, 0.06), border: `1px solid ${band("gamma", 68, 0.25)}` }}
                >
                  <span className={LABEL} style={{ color: GOLD, paddingTop: "2px" }}>
                    recommend
                  </span>
                  <p className={`${FONT_DISPLAY} text-base leading-snug`} style={{ color: INK }}>
                    {d.recommended_standard}
                  </p>
                </div>
              )}

              <div className="flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  disabled
                  className={`${FONT_MONO} shrink-0 cursor-not-allowed rounded-md px-3 py-1.5 text-sm`}
                  style={{ color: INK_FAINT, border: `1px dashed ${BORDER}` }}
                >
                  ratify as standard
                </button>
                <span className="min-w-0 flex-1 text-sm leading-snug" style={{ color: INK_FAINT }}>
                  Not wired yet — nothing on the server records a ratification. It is not the sweep&rsquo;s
                  call to make either: a lead ratifies this, with the approaches above in hand.
                </span>
              </div>

              <div
                className={`${FONT_MONO} flex flex-wrap justify-between gap-2 pt-1 text-[11px] uppercase tracking-[0.14em]`}
                style={{ borderTop: `1px solid ${BORDER}`, color: INK_FAINT, paddingTop: "12px" }}
              >
                <span>adjudicated by {d.model_ref ?? "—"}</span>
                <span>{day(d.detected_at)}</span>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default function StandardsVariantBoard({ data }: { data: PracticeDivergences }) {
  const reduce = !!useReducedMotion();

  const [query, setQuery] = useState("");
  const [team, setTeam] = useState<string>("all");
  const [impact, setImpact] = useState<string>("all");
  const [open, setOpen] = useState<string | null>(null);
  const [shut, setShut] = useState<Record<string, boolean>>({});

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
          haystack: `${d.practice} ${d.summary ?? ""}`.toLowerCase(),
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
    return (t: string) => m.get(t) ?? NEUTRAL_HUE;
  }, [roster]);

  const q = query.trim().toLowerCase();

  /* Scoped first, so the impact chips can carry counts that answer "what am I
     hiding?" rather than filtering against themselves. */
  const scoped = useMemo(
    () =>
      rows.filter(
        (r) =>
          (team === "all" || r.teams.includes(team)) && (q === "" || r.haystack.includes(q)),
      ),
    [rows, team, q],
  );

  const filtered = useMemo(
    () => (impact === "all" ? scoped : scoped.filter((r) => r.lane === impact)),
    [scoped, impact],
  );

  const laneCount = (k: Impact) => scoped.filter((r) => r.lane === k).length;
  const highTotal = rows.filter((r) => r.lane === "high").length;
  const dirty = q !== "" || team !== "all" || impact !== "all";

  const clear = () => {
    setQuery("");
    setTeam("all");
    setImpact("all");
  };

  return (
    <main className="mx-auto flex max-w-6xl flex-col gap-8 px-6 py-12">
      <header className="flex flex-col gap-3">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          standardization · triage bench
        </span>
        <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
          Where teams solved the same problem different ways
        </h1>
        <p className="max-w-3xl text-sm leading-snug" style={{ color: INK_DIM }}>
          A contradiction is two facts that can&rsquo;t both be true. This is subtler: teams each
          solving the <em>same</em> problem their own way, every choice locally reasonable, the drift
          invisible from inside a single team. The sweep only surfaces the genuine ones — length here
          is a backlog, not a score, and a short board is a healthy one.
        </p>
        {rows.length > 0 && (
          <p className={`${FONT_MONO} text-sm`} style={{ color: INK_FAINT }}>
            {rows.length} open · {highTotal} high impact · highest lane first
          </p>
        )}
      </header>

      {rows.length === 0 ? (
        <div
          className="flex flex-col gap-3 rounded-xl p-6"
          style={{ background: PANEL, border: `1px solid ${band("beta", 68, 0.3)}` }}
        >
          <span className={LABEL} style={{ color: band("beta") }}>
            in tune
          </span>
          <p className="max-w-2xl text-sm leading-snug" style={{ color: INK_DIM }}>
            Every shared practice is in tune — no cross-team divergence on the bench. Either the last
            sweep found every cluster consistent, or no sweep has run yet. This is the end state the
            board is for, not an empty screen: the detector is conservative on purpose, so nothing
            here means nothing worth a lead&rsquo;s afternoon.
          </p>
        </div>
      ) : (
        <>
          {/* Filters, because the first thing a sixty-row bench owes a lead is a
              way to make it smaller. */}
          <div
            className="flex flex-wrap items-center gap-x-6 gap-y-3 rounded-xl px-4 py-3"
            style={{ background: PANEL, border: `1px solid ${BORDER}` }}
          >
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="search practices and summaries"
              placeholder="search practice or summary…"
              className={`${FONT_MONO} w-full min-w-0 rounded-md border px-3 py-1.5 text-sm outline-none sm:w-64`}
              style={{ background: "rgba(0,0,0,0.25)", borderColor: BORDER, color: INK }}
            />

            <div className="flex flex-wrap items-center gap-2">
              <span className={LABEL} style={{ color: INK_FAINT }}>
                impact
              </span>
              <Chip label="all" hue={NEUTRAL_HUE} on={impact === "all"} onClick={() => setImpact("all")} />
              {IMPACTS.map((k) => (
                <Chip
                  key={k}
                  label={`${k} ${laneCount(k)}`}
                  hue={IMPACT_ACCENT[k]}
                  on={impact === k}
                  onClick={() => setImpact(k)}
                />
              ))}
            </div>

            {roster.length > 0 && (
              <div className="flex flex-wrap items-center gap-2">
                <span className={LABEL} style={{ color: INK_FAINT }}>
                  team
                </span>
                <Chip label="all" hue={NEUTRAL_HUE} on={team === "all"} onClick={() => setTeam("all")} />
                {roster.map((t) => (
                  <Chip
                    key={t}
                    label={t}
                    hue={teamHue(t)}
                    on={team === t}
                    onClick={() => setTeam(t)}
                  />
                ))}
              </div>
            )}

            {dirty && (
              <button
                type="button"
                onClick={clear}
                className={`${FONT_MONO} ml-auto text-sm underline underline-offset-4`}
                style={{ color: INK_FAINT }}
              >
                clear
              </button>
            )}
          </div>

          {filtered.length === 0 ? (
            /* Emphatically NOT the healthy-empty state above: this is a fact
               about the filter, not about the org. */
            <p
              className="rounded-xl p-6 text-sm leading-snug"
              style={{ background: PANEL, border: `1px solid ${BORDER}`, color: INK_DIM }}
            >
              No rows match these filters — {rows.length} still on the bench behind them.{" "}
              <button
                type="button"
                onClick={clear}
                className="underline underline-offset-4"
                style={{ color: INK }}
              >
                Clear the filters
              </button>{" "}
              to see them.
            </p>
          ) : (
            IMPACTS.map((k, li) => {
              const items = filtered.filter((r) => r.lane === k);
              if (items.length === 0) return null;
              const accent = IMPACT_ACCENT[k];
              const closed = !!shut[k];
              const loud = k === "high";

              return (
                <motion.section
                  key={k}
                  initial={reduce ? false : { opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.3, delay: reduce ? 0 : li * 0.06 }}
                  className="overflow-hidden rounded-xl"
                  style={{
                    background: PANEL,
                    border: `1px solid ${loud ? accent.line : BORDER}`,
                    boxShadow: loud && accent.glow ? `0 0 24px -6px ${accent.glow}` : undefined,
                  }}
                >
                  <button
                    type="button"
                    onClick={() => setShut((s) => ({ ...s, [k]: !s[k] }))}
                    aria-expanded={!closed}
                    className="flex w-full items-center gap-3 px-4 py-3 text-left transition hover:bg-white/[0.03]"
                    style={{ background: loud ? accent.wash : "transparent" }}
                  >
                    <span
                      aria-hidden
                      className="h-3 w-3 shrink-0 rounded-sm"
                      style={{
                        background: accent.line,
                        boxShadow: accent.glow ? `0 0 10px ${accent.glow}` : undefined,
                      }}
                    />
                    <span className={LABEL} style={{ color: accent.line }}>
                      {k} impact
                    </span>
                    <span className={`${FONT_MONO} text-sm`} style={{ color: INK }}>
                      {items.length}
                    </span>
                    <span className="hidden text-sm sm:block" style={{ color: INK_FAINT }}>
                      {LANE_NOTE[k]}
                    </span>
                    <span
                      aria-hidden
                      className={`${FONT_MONO} ml-auto text-sm`}
                      style={{ color: INK_FAINT }}
                    >
                      {closed ? "+" : "−"}
                    </span>
                  </button>

                  {!closed &&
                    items.map((r) => (
                      <BenchRow
                        key={r.key}
                        row={r}
                        open={open === r.key}
                        onToggle={() => setOpen((o) => (o === r.key ? null : r.key))}
                        teamHue={teamHue}
                        reduce={reduce}
                      />
                    ))}
                </motion.section>
              );
            })
          )}
        </>
      )}
    </main>
  );
}
