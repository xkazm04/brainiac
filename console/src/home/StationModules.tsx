"use client";

/*
 * Station modules — the landing page's figures of the product itself.
 *
 * Each station used to end in a one-line mono string: a code-snippet-shaped
 * artifact that stated a fact but never showed the reader the thing. Each one
 * now ends in a minimized, animated rendering of the module its CTA links to —
 * the review gate, the contradiction pair, the cortex map, the standards board,
 * a compiled page, the health report — drawn in that module's own idiom (its
 * chips, its meters, its star field, its tracks), so the scroll story is a
 * preview of the console rather than an illustration beside it.
 *
 * HONESTY. Every figure is the example org (the Meridian fixture) and says so:
 * the `example` tag in the frame header, on every one of them, always. The live
 * truth — real team names, the real open-contradiction count, the real health
 * score — stays exactly where it already was: the caption under the figure,
 * computed in Home from LiveStats. A figure never mixes the two. A real score
 * sitting inside a fictional card is the one thing a governance product cannot
 * ship, and "some of these numbers are real" is not a claim a reader can audit.
 *
 * MOTION. The landing page is the brand surface, so — like the wave field — the
 * figures may loop; theme.ts's no-infinite-loops rule binds the utility pages.
 * They loop only while on screen (useInView), and restart their story from step
 * 0 each time they are scrolled back to, so the reader never arrives mid-beat.
 * prefers-reduced-motion pins each figure to its last step, which every figure
 * below defines as its resolved frame — the story already told.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion, useInView, useReducedMotion } from "framer-motion";

import { CANONICAL_DEMO, CONTRADICTION, QUEUE } from "../design/demo-data";
import {
  band,
  FONT_MONO,
  GOLD,
  LABEL,
  MAGENTA,
  withAlpha,
} from "../design/theme";

export type StationModuleKind =
  | "gate"
  | "contradiction"
  | "cortex"
  | "divergence"
  | "page"
  | "health"
  | "library";

const dim = (a: number) => `rgba(233,237,255,${a})`;
const ALPHA = band("alpha");
const THETA = band("theta");
const BETA = band("beta");
const DELTA = band("delta");

/**
 * Translucent edge of an accent. The palette mixes hex (MAGENTA) and hsla
 * (everything from band()), so the withAlpha(tone, 0.33) hex-append trick used elsewhere
 * in the console silently produces invalid CSS for half the theme. This does
 * not.
 */
function soft(tone: string, a: number): string {
  if (tone.startsWith("#")) {
    const raw = tone.slice(1);
    const hex = raw.length === 3 ? raw.replace(/./g, (c) => c + c) : raw.slice(0, 6);
    const n = parseInt(hex, 16);
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
  }
  const inner = tone.slice(tone.indexOf("(") + 1, tone.lastIndexOf(")"));
  const [h, s, l] = inner.split(",").map((p) => p.trim());
  return `hsla(${h}, ${s}, ${l}, ${a})`;
}

/** The ambient step clock every looping figure runs on. */
function useStep(steps: number, ms: number, active: boolean): number {
  const reduce = !!useReducedMotion();
  const [i, setI] = useState(0);

  useEffect(() => {
    if (!active) return;
    setI(0); // scrolled back to → tell the story from the top, not from the middle
    if (reduce) return;
    const t = window.setInterval(() => setI((n) => (n + 1) % steps), ms);
    return () => window.clearInterval(t);
  }, [active, reduce, steps, ms]);

  return reduce ? steps - 1 : i;
}

// ── shared chrome ────────────────────────────────────────────────────────────

function Frame({
  title,
  tone,
  caption,
  children,
}: {
  title: string;
  tone: string;
  caption: ReactNode;
  children: ReactNode;
}) {
  return (
    <figure className="mt-5 overflow-hidden rounded-xl border border-white/10 bg-white/[0.02]">
      <div className="flex items-center justify-between gap-3 border-b border-white/[0.07] px-3.5 py-2">
        <span className={LABEL} style={{ color: tone }}>
          {title}
        </span>
        <span
          className={`${FONT_MONO} text-[10px] uppercase tracking-[0.2em]`}
          style={{ color: dim(0.25) }}
        >
          example
        </span>
      </div>
      <div className="px-3.5 py-4">{children}</div>
      <figcaption
        className={`${FONT_MONO} border-t border-white/[0.07] px-3.5 py-2.5 text-[12px] leading-relaxed`}
        style={{ color: dim(0.55) }}
      >
        {caption}
      </figcaption>
    </figure>
  );
}

function Chip({ children, tone }: { children: ReactNode; tone?: string }) {
  return (
    <span
      className={`${FONT_MONO} whitespace-nowrap rounded-full border px-2 py-0.5 text-[10px]`}
      style={{
        borderColor: tone ? soft(tone, 0.35) : dim(0.14),
        color: tone ? soft(tone, 0.85) : dim(0.6),
      }}
    >
      {children}
    </span>
  );
}

interface FigureProps {
  tone: string;
  caption: ReactNode;
  active: boolean;
}

// ── 01 · the review gate ─────────────────────────────────────────────────────

const KIND_TONE: Record<string, string> = {
  pitfall: MAGENTA,
  decision: ALPHA,
  howto: BETA,
};

const GATE_DWELL = 2600;

/**
 * The triage rail, minimized — the shape of the surface an operator actually
 * works (app/console/modules/reviews/ReviewWorklist.tsx, which won the
 * 2026-07-15 round over batch-signing).
 *
 * It is a rail and not a stack of cards for the same reason the module is: the
 * queue is long, so a reviewer needs the shape of the backlog on screen and one
 * item under the head — not their scroll position standing in for both. The
 * cursor walks; the row under it opens in the pane beside it and is signed.
 * The approve/reject pair is the demo's inert stamp: nothing is wired behind it.
 */
function GateFigure({ tone, caption, active }: FigureProps) {
  const step = useStep(QUEUE.length, GATE_DWELL, active);
  const reduce = !!useReducedMotion();
  const focused = QUEUE[step % QUEUE.length];

  return (
    <Frame title="review gate · the rail" tone={tone} caption={caption}>
      <div className="flex flex-col gap-3">
        {/* the rail: every pending decision, one line each */}
        {/* min-w-0: without it a grid/flex child refuses to shrink below its
            content, and `truncate` never fires — the rail just overflows. */}
        <div className="flex min-w-0 flex-col gap-px">
          <div
            className={`${LABEL} flex items-center justify-between pb-1.5`}
            style={{ color: dim(0.3) }}
          >
            <span>queue</span>
            <span>{QUEUE.length} pending</span>
          </div>
          {QUEUE.map((q, i) => {
            const on = i === step % QUEUE.length;
            return (
              <motion.div
                key={q.id}
                className="relative flex items-center gap-2 rounded-md px-2 py-1.5"
                animate={{
                  backgroundColor: on ? soft(ALPHA, 0.1) : "rgba(0,0,0,0)",
                }}
                transition={{ duration: 0.25 }}
              >
                {/* the head: which row the keyboard is on */}
                <motion.span
                  className="absolute left-0 top-1/2 h-[70%] w-[2px] -translate-y-1/2 rounded-full"
                  style={{ background: ALPHA }}
                  animate={{ opacity: on ? 1 : 0 }}
                  transition={{ duration: 0.2 }}
                />
                <span
                  className="h-1.5 w-1.5 shrink-0 rounded-sm"
                  style={{ background: KIND_TONE[q.kind] ?? dim(0.3) }}
                />
                <span
                  className={`${FONT_MONO} min-w-0 flex-1 truncate text-[11px]`}
                  style={{ color: dim(on ? 0.85 : 0.4) }}
                >
                  {q.content}
                </span>
                <span
                  className={`${FONT_MONO} ml-auto shrink-0 text-[10px]`}
                  style={{ color: dim(on ? 0.45 : 0.25) }}
                >
                  {q.age}
                </span>
              </motion.div>
            );
          })}
          <div
            className={`${FONT_MONO} mt-1.5 flex gap-2 border-t border-white/[0.07] pt-1.5 text-[10px]`}
            style={{ color: dim(0.28) }}
          >
            <span>j/k move</span>
            <span>a approve</span>
            <span>r reject</span>
          </div>
        </div>

        {/* the pane: the one item under the head, in full */}
        <motion.div
          key={focused.id}
          initial={reduce ? false : { opacity: 0, x: 6 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3 }}
          className="rounded-lg border p-2.5"
          style={{ borderColor: soft(ALPHA, 0.3), background: soft(ALPHA, 0.04) }}
        >
          <div className="flex flex-wrap items-center gap-1.5">
            <Chip tone={KIND_TONE[focused.kind]}>{focused.kind}</Chip>
            <Chip>team {focused.team}</Chip>
          </div>
          <p
            className={`${FONT_MONO} mt-2 text-[12px] leading-relaxed`}
            style={{ color: dim(0.8) }}
          >
            {focused.content}
          </p>
          <p className={`${FONT_MONO} mt-2 text-[10px]`} style={{ color: dim(0.35) }}>
            {focused.rule} · waiting {focused.age}
          </p>
          <div className="mt-2.5 flex items-center gap-2 border-t border-white/[0.07] pt-2.5">
            <span
              className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-[10px]`}
              style={{ borderColor: soft(GOLD, 0.35), color: soft(GOLD, 0.75) }}
            >
              approve
            </span>
            <span
              className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-[10px]`}
              style={{ borderColor: soft(MAGENTA, 0.3), color: soft(MAGENTA, 0.65) }}
            >
              reject
            </span>
          </div>
          {/* the SLO clock on the item in the gate */}
          <div
            className="mt-2.5 h-[2px] w-full overflow-hidden rounded-full"
            style={{ background: "rgba(255,255,255,0.07)" }}
          >
            <motion.div
              className="h-full rounded-full"
              style={{ background: GOLD }}
              initial={{ width: reduce ? "100%" : "0%" }}
              animate={{ width: "100%" }}
              transition={{ duration: reduce ? 0 : GATE_DWELL / 1000, ease: "linear" }}
            />
          </div>
        </motion.div>
      </div>
    </Frame>
  );
}

// ── 02 · the contradiction pair ──────────────────────────────────────────────

function ClaimCard({
  label,
  text,
  accent,
  state,
}: {
  label: string;
  text: string;
  accent: string;
  state: "held" | "superseded" | "canonical";
}) {
  const dead = state === "superseded";
  const won = state === "canonical";
  const edge = won ? GOLD : accent;
  return (
    <motion.div
      layout
      animate={{ opacity: dead ? 0.45 : 1 }}
      transition={{ duration: 0.4 }}
      className="rounded-lg border p-2.5"
      style={{
        borderColor: dead ? "rgba(233,237,255,0.10)" : soft(edge, 0.32),
        background: dead ? "transparent" : soft(edge, 0.05),
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <span className={LABEL} style={{ color: dead ? dim(0.35) : edge }}>
          {label}
        </span>
        {(won || dead) && (
          <motion.span
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className={`${FONT_MONO} text-[10px]`}
            style={{ color: won ? GOLD : MAGENTA }}
          >
            {won ? "✓ canonical" : "superseded"}
          </motion.span>
        )}
      </div>
      <p
        className={`${FONT_MONO} mt-1.5 text-[12px] leading-relaxed`}
        style={{
          color: dim(0.75),
          textDecoration: dead ? "line-through" : "none",
          textDecorationColor: MAGENTA,
        }}
      >
        {text}
      </p>
    </motion.div>
  );
}

/**
 * open → suggested → signed. The colour flip is the whole argument: B arrives as
 * the contradiction (magenta) and leaves as the canon (gold), while A is struck
 * through and kept. The loser is superseded, never deleted — so the strike is a
 * line, not a removal.
 */
function ContradictionFigure({ tone, caption, active }: FigureProps) {
  const step = useStep(3, 2600, active);
  const suggested = step >= 1;
  const signed = step === 2;

  return (
    <Frame title="contradiction · the seam" tone={tone} caption={caption}>
      <div className="flex flex-wrap items-center gap-1.5">
        <Chip tone={signed ? GOLD : MAGENTA}>{signed ? "resolved" : "open"}</Chip>
        <Chip>detected by scan</Chip>
      </div>

      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        <ClaimCard
          label="memory a"
          text={CONTRADICTION.a}
          accent={GOLD}
          state={signed ? "superseded" : "held"}
        />
        <ClaimCard
          label="memory b"
          text={CONTRADICTION.b}
          accent={MAGENTA}
          state={signed ? "canonical" : "held"}
        />
      </div>

      {/* Reserved, not collapsed: the row keeps its height across the loop so the
          station does not jump 40px every time the story comes round again. */}
      <div className="mt-3 min-h-[46px]">
        <AnimatePresence>
          {suggested && (
            <motion.div
              key="resolution"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.4 }}
              className="flex items-start gap-2.5 rounded-lg border px-2.5 py-2"
              style={{
                borderColor: soft(signed ? GOLD : ALPHA, 0.3),
                background: soft(signed ? GOLD : ALPHA, 0.05),
              }}
            >
              <span
                className={LABEL}
                style={{ color: signed ? GOLD : ALPHA, paddingTop: "3px" }}
              >
                {signed ? "signed" : "suggest"}
              </span>
              <p
                className={`${FONT_MONO} text-[12px] leading-relaxed`}
                style={{ color: dim(0.75) }}
              >
                {CONTRADICTION.suggestion}
              </p>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </Frame>
  );
}

// ── 03 · the cortex map ──────────────────────────────────────────────────────

/** The cortex team palette (alpha / delta / beta), matching the star chart. */
const TEAM_HUE = [190, 262, 158];

/** Deterministic, not random: the sky must be identical on the server and the client. */
const DUST = [
  { x: 24, y: 106, r: 1.1 },
  { x: 104, y: 14, r: 0.9 },
  { x: 148, y: 100, r: 1.4 },
  { x: 262, y: 112, r: 1.0 },
  { x: 292, y: 92, r: 1.7 },
  { x: 356, y: 40, r: 1.0 },
  { x: 120, y: 62, r: 1.2 },
  { x: 246, y: 26, r: 1.1 },
  { x: 316, y: 128, r: 0.8 },
];

const STAR_SEATS = [
  { x: 48, y: 34 },
  { x: 330, y: 38 },
  { x: 74, y: 124 },
];

/**
 * The star chart, minimized: three teams' surface forms for the same thing,
 * scattered in the sky, and the constellation that binds them to one canonical
 * star. Unbound, the centre is a faint speck no one is looking at.
 */
function CortexFigure({ tone, caption, active }: FigureProps) {
  const step = useStep(2, 3000, active);
  const bound = step === 1;
  const W = 380;
  const H = 152;
  const cx = 200;
  const cy = 74;

  const stars = CANONICAL_DEMO.aliases.map((a, i) => ({
    ...a,
    hue: TEAM_HUE[i % TEAM_HUE.length],
    ...STAR_SEATS[i % STAR_SEATS.length],
  }));

  return (
    <Frame title="cortex map · binding" tone={tone} caption={caption}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="w-full"
        role="img"
        aria-label={`Three teams' names for the same thing — ${stars
          .map((s) => s.name)
          .join(", ")} — bound by constellation lines into one canonical star, ${CANONICAL_DEMO.name}.`}
      >
        {DUST.map((d) => (
          <circle key={`${d.x}-${d.y}`} cx={d.x} cy={d.y} r={d.r} fill={dim(0.16)} />
        ))}

        {stars.map((s, i) => (
          <motion.line
            key={`edge-${s.team}`}
            x1={s.x}
            y1={s.y}
            x2={cx}
            y2={cy}
            stroke={`hsla(${s.hue}, 85%, 68%, 0.5)`}
            strokeWidth={1}
            initial={false}
            animate={{ pathLength: bound ? 1 : 0, opacity: bound ? 1 : 0 }}
            transition={{ duration: 0.7, delay: bound ? i * 0.14 : 0, ease: "easeOut" }}
          />
        ))}

        {/* the canonical star — a speck until the org agrees what to call it */}
        <motion.circle
          cx={cx}
          cy={cy}
          fill={GOLD}
          initial={false}
          animate={{ r: bound ? 6.5 : 2.5, opacity: bound ? 1 : 0.35 }}
          transition={{ duration: 0.5, ease: "easeOut" }}
        />
        {bound && (
          <motion.circle
            cx={cx}
            cy={cy}
            fill="none"
            stroke={GOLD}
            strokeWidth={1}
            initial={{ r: 7, opacity: 0.55 }}
            animate={{ r: 20, opacity: 0 }}
            transition={{ duration: 1.6, repeat: Infinity, ease: "easeOut" }}
          />
        )}
        <motion.text
          x={cx}
          y={cy + 22}
          textAnchor="middle"
          fontSize="9.5"
          fill={GOLD}
          initial={false}
          animate={{ opacity: bound ? 1 : 0 }}
          transition={{ duration: 0.4, delay: bound ? 0.5 : 0 }}
        >
          {CANONICAL_DEMO.name} · bound across {CANONICAL_DEMO.aliases.length} teams
        </motion.text>

        {stars.map((s) => (
          <g key={s.team}>
            <circle cx={s.x} cy={s.y} r={3.2} fill={`hsla(${s.hue}, 85%, 68%, 1)`} />
            <text
              x={s.x}
              y={s.y - 9}
              textAnchor="middle"
              fontSize="9.5"
              fill={`hsla(${s.hue}, 85%, 74%, 0.95)`}
            >
              “{s.name}”
            </text>
            <text
              x={s.x}
              y={s.y + 16}
              textAnchor="middle"
              fontSize="8"
              letterSpacing="1.4"
              fill={dim(0.35)}
            >
              {s.team}
            </text>
          </g>
        ))}
      </svg>
    </Frame>
  );
}

// ── 04 · the standards board ─────────────────────────────────────────────────

/*
 * The retry-policy finding from the fixture sweep, compacted to what fits on a
 * track. Team accents are theta and delta here, not the board's theta/gamma:
 * gamma is gold, and gold is spoken for — it marks the standard the two teams
 * converge on, so it cannot also mark one of the teams.
 */
const PRACTICE = {
  title: "service retry policy",
  impact: "high",
  standardAt: 16,
  standardLabel: "2 s · 3 attempts",
  standard: "Adopt a 2 s retry cap, 3 attempts, for every internal service call.",
  teams: [
    { team: "platform", label: "2 s cap · 3 attempts", at: 16, tone: THETA },
    { team: "payments", label: "30 s cap · jitter", at: 88, tone: DELTA },
  ],
};

/**
 * Not a contradiction — a detune. One axis, two teams, and the distance between
 * their knobs IS the divergence: locally reasonable at each end, only visible
 * when you put both on the same track. The sweep names it and proposes one
 * standard; the knobs converge on it.
 */
function DivergenceFigure({ tone, caption, active }: FigureProps) {
  const step = useStep(3, 2600, active);
  const marked = step >= 1;
  const converged = step === 2;

  return (
    <Frame title="standards board" tone={tone} caption={caption}>
      <div className="flex flex-wrap items-center gap-2">
        <Chip tone={MAGENTA}>{PRACTICE.impact} impact</Chip>
        <span className="text-[15px] leading-none text-white">{PRACTICE.title}</span>
      </div>

      <div className="mt-4 flex flex-col gap-4">
        {PRACTICE.teams.map((t) => (
          <div key={t.team}>
            <div className="flex items-baseline justify-between gap-3">
              <span className={LABEL} style={{ color: t.tone }}>
                {t.team}
              </span>
              <motion.span
                className={`${FONT_MONO} text-[11px]`}
                animate={{ color: converged ? GOLD : dim(0.6) }}
                transition={{ duration: 0.4 }}
              >
                {converged ? PRACTICE.standardLabel : t.label}
              </motion.span>
            </div>
            <div
              className="relative mt-2 h-[3px] w-full rounded-full"
              style={{ background: "rgba(255,255,255,0.07)" }}
            >
              {/* the standard the sweep proposes */}
              <motion.span
                className="absolute -top-[4px] h-[11px] w-[2px] rounded-full"
                style={{ background: GOLD, left: `${PRACTICE.standardAt}%` }}
                initial={false}
                animate={{ opacity: marked ? 1 : 0 }}
                transition={{ duration: 0.4 }}
              />
              {/* where this team actually landed */}
              <motion.span
                className="absolute -top-[3px] h-[9px] w-[9px] -translate-x-1/2 rounded-full"
                style={{ background: t.tone }}
                initial={false}
                animate={{ left: `${converged ? PRACTICE.standardAt : t.at}%` }}
                transition={{ type: "spring", stiffness: 80, damping: 15 }}
              />
            </div>
          </div>
        ))}
      </div>

      <div className="mt-4 flex items-start gap-2.5 border-t border-white/[0.07] pt-3">
        <span className={LABEL} style={{ color: BETA, paddingTop: "3px" }}>
          recommend
        </span>
        <motion.p
          className="text-[13px] leading-snug"
          style={{ color: dim(0.85) }}
          initial={false}
          animate={{ opacity: marked ? 1 : 0.25 }}
          transition={{ duration: 0.4 }}
        >
          {PRACTICE.standard}
        </motion.p>
      </div>
    </Frame>
  );
}

// ── 05 · the page that cannot rot ────────────────────────────────────────────

const PAGE_LINES = [
  { id: "l1", text: "Retry internal calls at most 3 times." },
  { id: "l2", text: "Cap the retry window at 30 seconds, with jitter." },
  { id: "l3", text: "Keep idempotency keys for 24 hours." },
];

const RECOMPILED_LINE = "Cap the retry window at 2 seconds.";

/**
 * composed → dirty → recompiled. The memory under line two is superseded by the
 * standard from station 04; for one beat the page is provably stale and says so
 * — and then it recompiles, because it is a projection and never a place the
 * sentence lives. The wiki's failure mode is that the middle beat lasts a year.
 */
function PageFigure({ tone, caption, active }: FigureProps) {
  const step = useStep(3, 2800, active);
  const dirty = step === 1;
  const recompiled = step === 2;
  const changed = dirty || recompiled;

  const memories = [
    { id: "m1", text: "retry: 3 attempts max", superseded: false },
    {
      id: "m2",
      text: changed ? "retry cap 2 s · standard" : "retry cap 30 s · jitter",
      superseded: changed,
    },
    { id: "m3", text: "idempotency keys: 24 h", superseded: false },
  ];

  return (
    <Frame title="knowledge base · a page" tone={tone} caption={caption}>
      <div className={LABEL} style={{ color: dim(0.35) }}>
        canonical memories
      </div>
      <div className="mt-2 flex flex-col gap-1.5">
        {memories.map((m) => (
          <motion.div
            key={m.id}
            layout
            className="flex items-center gap-2 rounded-md border px-2 py-1.5"
            animate={{
              borderColor: m.superseded ? soft(GOLD, 0.35) : "rgba(255,255,255,0.08)",
              backgroundColor: m.superseded ? soft(GOLD, 0.05) : "rgba(255,255,255,0)",
            }}
            transition={{ duration: 0.4 }}
          >
            <motion.span
              className="h-1.5 w-1.5 shrink-0 rounded-full"
              animate={{ backgroundColor: m.superseded ? GOLD : dim(0.25) }}
            />
            <motion.span
              key={m.text}
              initial={{ opacity: 0, x: -4 }}
              animate={{ opacity: 1, x: 0 }}
              className={`${FONT_MONO} text-[11px]`}
              style={{ color: dim(0.72) }}
            >
              {m.text}
            </motion.span>
            {m.superseded && (
              <motion.span
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                className={`${FONT_MONO} ml-auto text-[10px]`}
                style={{ color: GOLD }}
              >
                new · v2
              </motion.span>
            )}
          </motion.div>
        ))}
      </div>

      <div
        className={`${LABEL} mt-3 flex items-center gap-2`}
        style={{ color: recompiled ? DELTA : dim(0.3) }}
      >
        <motion.span
          animate={{ y: recompiled ? [0, 3, 0] : 0 }}
          transition={{ duration: 0.6, repeat: recompiled ? Infinity : 0 }}
        >
          ↓
        </motion.span>
        compose
      </div>

      <div
        className="mt-2 rounded-lg border p-3"
        style={{ borderColor: soft(DELTA, 0.25), background: soft(DELTA, 0.04) }}
      >
        <div className="flex items-center justify-between gap-2">
          <span className={`${FONT_MONO} text-[12px]`} style={{ color: dim(0.9) }}>
            std-retry
          </span>
          <AnimatePresence mode="wait">
            {dirty && (
              <motion.span
                key="stale"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                className={`${FONT_MONO} text-[10px]`}
                style={{ color: MAGENTA }}
              >
                ● stale — a memory moved
              </motion.span>
            )}
            {recompiled && (
              <motion.span
                key="fresh"
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0 }}
                className={`${FONT_MONO} text-[10px]`}
                style={{ color: GOLD }}
              >
                ✓ recompiled
              </motion.span>
            )}
          </AnimatePresence>
        </div>
        <div className="mt-2 flex flex-col gap-1.5">
          {PAGE_LINES.map((l) => {
            const rewrites = l.id === "l2";
            const text = rewrites && recompiled ? RECOMPILED_LINE : l.text;
            return (
              <motion.p
                key={`${l.id}-${text}`}
                initial={rewrites ? { opacity: 0, x: -6 } : false}
                animate={{
                  opacity: 1,
                  x: 0,
                  color: rewrites && dirty ? MAGENTA : dim(0.7),
                }}
                transition={{ duration: 0.4 }}
                className={`${FONT_MONO} text-[11px] leading-relaxed`}
              >
                {text}
              </motion.p>
            );
          })}
        </div>
      </div>
    </Frame>
  );
}

// ── 06 · the vital sign ──────────────────────────────────────────────────────

/*
 * The report, not a feed: its motion is the reveal, and it plays on scroll
 * rather than looping. Consistency is the pillar that tells the story — it runs
 * up to where the volume of good memories would put it, and is then hauled back
 * down by the one unresolved cross-team contradiction. That cap is the product's
 * whole opinion about knowledge: you cannot outvote a contradiction.
 */
const CAPPED_CONSISTENCY = 42;
const UNCAPPED_CONSISTENCY = 78;

const PILLARS = [
  { key: "consistency", value: CAPPED_CONSISTENCY, capped: true },
  { key: "currency", value: 71, capped: false },
  { key: "liquidity", value: 66, capped: false },
  { key: "governance", value: 84, capped: false },
];

const TREND = [46, 52, 49, 58, 55, 61];

const pillarTone = (v: number) => (v >= 75 ? ALPHA : v >= 50 ? GOLD : MAGENTA);

function HealthFigure({ tone, caption }: FigureProps) {
  const reduce = !!useReducedMotion();
  const w = 96;
  const h = 26;
  // Scaled to its own range, not to 0–100: a governance score moves in single
  // digits, and against a 0–100 axis at this size every trend is a flat line.
  const lo = Math.min(...TREND) - 3;
  const hi = Math.max(...TREND) + 3;
  const trend = TREND.map(
    (v, i) =>
      `${i === 0 ? "M" : "L"}${(i / (TREND.length - 1)) * w},${h - ((v - lo) / (hi - lo)) * h}`,
  ).join(" ");

  return (
    <Frame title="knowledge health" tone={tone} caption={caption}>
      <div className="flex items-end justify-between gap-4">
        <div className="flex items-end gap-3">
          <span className={`${FONT_MONO} text-5xl leading-none`} style={{ color: GOLD }}>
            61
          </span>
          <span className={`${FONT_MONO} pb-1 text-[13px]`} style={{ color: GOLD }}>
            Watch
          </span>
        </div>
        <div className="flex flex-col items-end gap-1">
          <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden>
            <motion.path
              d={trend}
              fill="none"
              stroke={GOLD}
              strokeWidth={1.4}
              initial={{ pathLength: reduce ? 1 : 0 }}
              whileInView={{ pathLength: 1 }}
              viewport={{ once: false, amount: 0.6 }}
              transition={{ duration: 1.2, ease: "easeOut" }}
            />
          </svg>
          <span className={`${FONT_MONO} text-[10px]`} style={{ color: dim(0.35) }}>
            the line, not the point
          </span>
        </div>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-3">
        {PILLARS.map((p, i) => {
          const accent = pillarTone(p.value);
          return (
            <div key={p.key} className="flex flex-col gap-1.5">
              <div className="flex items-baseline justify-between">
                <span className={LABEL} style={{ color: dim(0.5) }}>
                  {p.key}
                </span>
                <span className={`${FONT_MONO} text-[13px]`} style={{ color: accent }}>
                  {p.value}
                </span>
              </div>
              <div
                className="h-1 w-full overflow-hidden rounded-full"
                style={{ background: "rgba(255,255,255,0.08)" }}
              >
                {/* Consistency overshoots to where the raw volume of good memories
                    would put it — then the cap hauls it back. */}
                <motion.div
                  className="h-full rounded-full"
                  style={{ background: accent }}
                  initial={{ width: reduce ? `${p.value}%` : "0%" }}
                  whileInView={{
                    width: p.capped
                      ? [`0%`, `${UNCAPPED_CONSISTENCY}%`, `${p.value}%`]
                      : `${p.value}%`,
                  }}
                  viewport={{ once: false, amount: 0.6 }}
                  transition={{
                    duration: p.capped ? 1.7 : 0.9,
                    times: p.capped ? [0, 0.55, 1] : undefined,
                    delay: 0.15 * i,
                    ease: "easeOut",
                  }}
                />
              </div>
            </div>
          );
        })}
      </div>

      <motion.div
        className="mt-4 flex items-start gap-2.5 rounded-lg border px-2.5 py-2"
        style={{ borderColor: soft(MAGENTA, 0.28), background: soft(MAGENTA, 0.05) }}
        initial={{ opacity: reduce ? 1 : 0, y: reduce ? 0 : 6 }}
        whileInView={{ opacity: 1, y: 0 }}
        viewport={{ once: false, amount: 0.6 }}
        transition={{ duration: 0.5, delay: 1.4 }}
      >
        <span className={LABEL} style={{ color: MAGENTA, paddingTop: "3px" }}>
          cap
        </span>
        <p className={`${FONT_MONO} text-[12px] leading-relaxed`} style={{ color: dim(0.75) }}>
          1 unresolved cross-team contradiction — no volume of good memories outvotes it.
        </p>
      </motion.div>
    </Frame>
  );
}

// ── the switchboard ──────────────────────────────────────────────────────────

/** The Library station: a proposed rule waits at the gate, a named human
 *  adopts it, and an agent fetches it — the three-beat loop of the normative
 *  layer (LIBRARY-PLAN), told with the same vocabulary the console uses. */
function LibraryFigure({ tone, caption, active }: FigureProps) {
  const step = useStep(3, 2600, active);
  const adopted = step >= 1;
  const fetched = step === 2;

  return (
    <Frame title="library · a rule" tone={tone} caption={caption}>
      <div className={LABEL} style={{ color: dim(0.35) }}>
        the rule shelf
      </div>
      <div className="mt-2 flex flex-col gap-1.5">
        <div
          className="flex items-center gap-2 rounded-md border px-2 py-1.5"
          style={{ borderColor: "rgba(255,255,255,0.08)" }}
        >
          <span className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: BETA }} />
          <span className={`${FONT_MONO} text-[11px]`} style={{ color: dim(0.72) }}>
            no-unwrap-in-handlers · mandatory
          </span>
        </div>
        <motion.div
          className="flex items-center gap-2 rounded-md border px-2 py-1.5"
          animate={{
            borderColor: adopted ? soft(BETA, 0.35) : soft(GOLD, 0.45),
            backgroundColor: adopted ? soft(BETA, 0.04) : soft(GOLD, 0.05),
          }}
          transition={{ duration: 0.4 }}
          style={{ borderStyle: adopted ? "solid" : "dashed" }}
        >
          <motion.span
            className="h-1.5 w-1.5 shrink-0 rounded-full"
            animate={{ backgroundColor: adopted ? BETA : GOLD }}
          />
          <span className={`${FONT_MONO} text-[11px]`} style={{ color: dim(0.72) }}>
            service-retry-policy
          </span>
          <motion.span
            key={adopted ? "adopted" : "gate"}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className={`${FONT_MONO} ml-auto text-[10px]`}
            style={{ color: adopted ? BETA : GOLD }}
          >
            {adopted ? "adopted · signed" : "at the gate — a human decides"}
          </motion.span>
        </motion.div>
      </div>

      <div
        className={`${LABEL} mt-3 flex items-center gap-2`}
        style={{ color: fetched ? THETA : dim(0.3) }}
      >
        <motion.span
          animate={{ y: fetched ? [0, 3, 0] : 0 }}
          transition={{ duration: 0.6, repeat: fetched ? Infinity : 0 }}
        >
          ↓
        </motion.span>
        served — adopted rules only
      </div>

      <motion.div
        className="mt-2 flex items-center gap-2 rounded-lg border p-3"
        animate={{
          borderColor: fetched ? soft(THETA, 0.35) : "rgba(255,255,255,0.08)",
          backgroundColor: fetched ? soft(THETA, 0.04) : "rgba(255,255,255,0)",
        }}
        transition={{ duration: 0.4 }}
      >
        <span className={`${FONT_MONO} text-[11px]`} style={{ color: dim(0.72) }}>
          coding agent · standards_for(&quot;rust&quot;)
        </span>
        <motion.span
          className={`${FONT_MONO} ml-auto text-[10px]`}
          animate={{ opacity: fetched ? 1 : 0 }}
          style={{ color: THETA }}
        >
          2 rules · usage → team, never a name
        </motion.span>
      </motion.div>
    </Frame>
  );
}

const FIGURES: Record<StationModuleKind, (p: FigureProps) => ReactNode> = {
  gate: GateFigure,
  contradiction: ContradictionFigure,
  cortex: CortexFigure,
  divergence: DivergenceFigure,
  page: PageFigure,
  health: HealthFigure,
  library: LibraryFigure,
};

export default function StationModule({
  kind,
  tone,
  caption,
}: {
  kind: StationModuleKind;
  tone: string;
  caption: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Figures loop only while on screen: six ambient animations racing the wave
  // canvas for frames, all of them off-screen, is a battery bill for nothing.
  const active = useInView(ref, { amount: 0.35 });
  const Figure = FIGURES[kind];
  return <div ref={ref}>{Figure({ tone, caption, active })}</div>;
}
