/*
 * Knowledge Health — the leadership report (KB-PLAN KB0).
 *
 * The server has computed this composite since a229209; nothing rendered it,
 * so the product's own answer to "is our knowledge rotting?" was invisible.
 *
 * Design stance: a score with no next action is a vanity metric. The headline
 * exists to be argued with, so every pillar states what it measures, and the
 * attention list — not the number — is the surface's centre of gravity. The
 * trend line matters more than the point: one reading cannot tell a leader
 * whether governance is recovering or sliding.
 *
 * This is also the surface the document layer (KB3) will gate publishing on:
 * when currency or governance falls below threshold, external wiki sync pauses
 * rather than propagating stale beliefs into the company's Confluence.
 */

import {
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  PANEL,
  band,
} from "@/design/theme";
import type { KhAttention, KnowledgeHealth as Health } from "@/lib/types";

/** Grade → accent. Red is reserved: only a failing org gets the alarm colour. */
const gradeAccent = (grade: string): string => {
  switch (grade) {
    case "Healthy":
      return band("alpha");
    case "Watch":
      return band("gamma");
    default:
      return MAGENTA; // At risk | Critical
  }
};

const severityAccent = (s: string): string =>
  s === "critical" ? MAGENTA : s === "warning" ? band("gamma") : band("beta");

/** Humanize an age in seconds — leaders read "2d 16h", not "232000". */
const age = (secs: number): string => {
  if (secs <= 0) return "—";
  const d = Math.floor(secs / 86_400);
  const h = Math.floor((secs % 86_400) / 3_600);
  if (d > 0) return `${d}d ${h}h`;
  const m = Math.floor((secs % 3_600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
};

const PILLAR_COPY: Record<string, { label: string; asks: string }> = {
  consistency: {
    label: "consistency",
    asks: "Does the org contradict itself? Cross-team conflicts hurt most.",
  },
  currency: {
    label: "currency",
    asks: "Is what we serve still true — or expired and never re-verified?",
  },
  liquidity: {
    label: "liquidity",
    asks: "Does knowledge cross team lines, or stay trapped where it was learned?",
  },
  governance: {
    label: "governance",
    asks: "Is the review queue actually being worked, against the 48h SLO?",
  },
};

function Pillar({ name, value }: { name: keyof typeof PILLAR_COPY; value: number }) {
  const copy = PILLAR_COPY[name];
  const accent = value >= 75 ? band("alpha") : value >= 50 ? band("gamma") : MAGENTA;
  return (
    <div
      className="flex flex-col gap-3 rounded-lg p-5"
      style={{ background: PANEL, border: `1px solid ${BORDER}` }}
    >
      <div className="flex items-baseline justify-between">
        <span className={LABEL} style={{ color: INK_DIM }}>
          {copy.label}
        </span>
        <span className={`${FONT_MONO} text-2xl`} style={{ color: accent }}>
          {value}
        </span>
      </div>
      {/* The meter, not a chart: one bar, one number, no axis to decode. */}
      <div className="h-1 w-full overflow-hidden rounded-full" style={{ background: BORDER }}>
        <div
          className="h-full rounded-full"
          style={{ width: `${Math.max(0, Math.min(100, value))}%`, background: accent }}
        />
      </div>
      <p className="text-[13px] leading-snug" style={{ color: INK_FAINT }}>
        {copy.asks}
      </p>
    </div>
  );
}

/** Composite score over time. Inline SVG — the trend is a line, not a library. */
function Trend({ points }: { points: Health["trend"] }) {
  if (points.length < 2) {
    return (
      <p className={`${FONT_MONO} text-[12px]`} style={{ color: INK_FAINT }}>
        No trend yet — the line appears once a second snapshot is recorded.
      </p>
    );
  }
  const w = 260;
  const h = 56;
  const xs = (i: number) => (i / (points.length - 1)) * w;
  const ys = (v: number) => h - (Math.max(0, Math.min(100, v)) / 100) * h;
  const d = points.map((p, i) => `${i === 0 ? "M" : "L"}${xs(i)},${ys(p.score)}`).join(" ");
  const last = points[points.length - 1];
  const first = points[0];
  const delta = last.score - first.score;
  const stroke = gradeAccent(last.score >= 80 ? "Healthy" : last.score >= 60 ? "Watch" : "At risk");
  return (
    <div className="flex items-center gap-4">
      <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden>
        <path d={d} fill="none" stroke={stroke} strokeWidth={1.5} />
        <circle cx={xs(points.length - 1)} cy={ys(last.score)} r={2.5} fill={stroke} />
      </svg>
      <div className={`${FONT_MONO} text-[12px]`} style={{ color: INK_DIM }}>
        <div style={{ color: delta < 0 ? MAGENTA : INK_DIM }}>
          {delta > 0 ? "+" : ""}
          {delta} over {points.length} snapshots
        </div>
        <div style={{ color: INK_FAINT }}>the line, not the point</div>
      </div>
    </div>
  );
}

function AttentionRow({ item }: { item: KhAttention }) {
  const accent = severityAccent(item.severity);
  return (
    <li
      className="flex flex-col gap-1.5 rounded-lg p-4"
      style={{ background: PANEL, border: `1px solid ${BORDER}`, borderLeft: `2px solid ${accent}` }}
    >
      <div className="flex items-center gap-3">
        <span className={LABEL} style={{ color: accent }}>
          {item.severity}
        </span>
        <span className={LABEL} style={{ color: INK_FAINT }}>
          {item.kind}
        </span>
      </div>
      <p className="text-[15px] leading-snug" style={{ color: INK }}>
        {item.headline}
      </p>
      <p className={`${FONT_MONO} text-[12px] leading-relaxed`} style={{ color: INK_DIM }}>
        {item.detail}
      </p>
    </li>
  );
}

function Signal({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex flex-col gap-1">
      <span className={`${FONT_MONO} text-lg`} style={{ color: INK }}>
        {value}
      </span>
      <span className={LABEL} style={{ color: INK_FAINT }}>
        {label}
      </span>
    </div>
  );
}

export default function KnowledgeHealthReport({ data }: { data: Health }) {
  const { score, grade, pillars, signals, attention, trend } = data;
  const accent = gradeAccent(grade);
  return (
    <main className="mx-auto flex max-w-5xl flex-col gap-10 px-6 py-12">
      <header className="flex flex-col gap-6">
        <div className="flex flex-col gap-2">
          <span className={LABEL} style={{ color: INK_FAINT }}>
            knowledge health
          </span>
          <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
            Is the org&rsquo;s knowledge still worth trusting?
          </h1>
        </div>
        <div className="flex flex-wrap items-end justify-between gap-8">
          <div className="flex items-end gap-5">
            <span className={`${FONT_MONO} text-7xl leading-none`} style={{ color: accent }}>
              {score}
            </span>
            <div className="flex flex-col gap-1 pb-2">
              <span className={`${FONT_DISPLAY} text-xl`} style={{ color: accent }}>
                {grade}
              </span>
              <span className={`${FONT_MONO} text-[12px]`} style={{ color: INK_FAINT }}>
                composite of four pillars
              </span>
            </div>
          </div>
          <Trend points={trend} />
        </div>
        {signals.cross_team_contradictions > 0 && (
          <p
            className={`${FONT_MONO} rounded-lg px-4 py-3 text-[12px] leading-relaxed`}
            style={{ background: PANEL, border: `1px solid ${MAGENTA}`, color: INK_DIM }}
          >
            The headline is capped: {signals.cross_team_contradictions} unreconciled cross-team
            contradiction
            {signals.cross_team_contradictions === 1 ? "" : "s"} means no volume of good memories
            can grade this org Healthy. Two teams are acting on incompatible truths and neither can
            see the other.
          </p>
        )}
      </header>

      <section className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Pillar name="consistency" value={pillars.consistency} />
        <Pillar name="currency" value={pillars.currency} />
        <Pillar name="liquidity" value={pillars.liquidity} />
        <Pillar name="governance" value={pillars.governance} />
      </section>

      <section className="flex flex-col gap-4">
        <div className="flex items-baseline gap-3">
          <h2 className={`${FONT_DISPLAY} text-xl`} style={{ color: INK }}>
            Needs your attention
          </h2>
          <span className={`${FONT_MONO} text-[12px]`} style={{ color: INK_FAINT }}>
            ranked — most urgent first
          </span>
        </div>
        {attention.length === 0 ? (
          <p
            className="rounded-lg p-5 text-[14px]"
            style={{ background: PANEL, border: `1px solid ${BORDER}`, color: INK_DIM }}
          >
            Nothing is on fire: no open cross-team contradictions, no expired beliefs being served,
            no SLO breach. This is the state the review gate exists to hold.
          </p>
        ) : (
          <ul className="flex flex-col gap-3">
            {attention.map((item, i) => (
              <AttentionRow key={`${item.kind}-${i}`} item={item} />
            ))}
          </ul>
        )}
      </section>

      <section className="flex flex-col gap-5">
        <h2 className={`${FONT_DISPLAY} text-xl`} style={{ color: INK }}>
          The corpus underneath
        </h2>
        <div
          className="grid grid-cols-2 gap-6 rounded-lg p-6 sm:grid-cols-3 lg:grid-cols-4"
          style={{ background: PANEL, border: `1px solid ${BORDER}` }}
        >
          <Signal label="memories" value={signals.total_memories} />
          <Signal label="canonical entities" value={signals.canonical_entities} />
          <Signal label="cross-team entities" value={signals.cross_team_entities} />
          <Signal label="liquidity" value={`${signals.liquidity_pct}%`} />
          <Signal label="open contradictions" value={signals.open_contradictions} />
          <Signal label="cross-team conflicts" value={signals.cross_team_contradictions} />
          <Signal label="stale beliefs" value={signals.stale_beliefs} />
          <Signal label="review backlog" value={signals.review_backlog} />
          <Signal label="oldest review" value={age(signals.oldest_review_secs)} />
          <Signal label="org-wide" value={signals.org_wide} />
          <Signal label="team-only" value={signals.team_only} />
          <Signal label="private" value={signals.siloed_private} />
        </div>
        <p className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
          embeddings: {data.embedding_model}
        </p>
      </section>
    </main>
  );
}
