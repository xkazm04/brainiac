"use client";

/*
 * Observatory — the consolidated dashboard structure ("Mission Control",
 * winner of the 2026-07-10 prototype round). An operations wall: every
 * gauge visible at once, tiled small multiples, dense mono instrumentation.
 * Beta band (active recall) accent throughout.
 *
 * Pure presentation over ObservatoryData — consumed by /analytics (live,
 * demo fallback) and /demo (visitor-facing, always demo data).
 */

import { motion } from "framer-motion";
import Link from "next/link";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { band, bandGlow, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import type { ObservatoryData } from "./observatory-data";

const MINT = band("beta");
const MINT_DIM = band("beta", 68, 0.35);
const TILE = "rounded-lg border border-white/10 bg-white/[0.02] p-4";

const rise = {
  hidden: { opacity: 0, y: 10 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.35 } },
};
const stagger = { visible: { transition: { staggerChildren: 0.05 } } };

function age(secs: number): string {
  if (secs <= 0) return "—";
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  if (secs < 86400) return `${(secs / 3600).toFixed(1)}h`;
  return `${(secs / 86400).toFixed(1)}d`;
}

function ChartTip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: { name: string; value: number }[];
  label?: string;
}) {
  if (!active || !payload?.length) return null;
  return (
    <div className={`${FONT_MONO} rounded border border-white/15 bg-[#0b0a10] px-3 py-2 text-xs`}>
      <div className="text-white/40">{label}</div>
      {payload.map((p) => (
        <div key={p.name} className="text-white/85">
          {p.name}: <span style={{ color: MINT }}>{p.value}</span>
        </div>
      ))}
    </div>
  );
}

export default function Observatory({ data }: { data: ObservatoryData }) {
  /* The lifecycle, in order — raw arrives, candidate is proposed, canonical is
     signed for, deprecated is kept and outranked. Gold marks the only tier the
     org actually stands behind. */
  const STATUS_ROWS = [
    { key: "canonical", label: "canonical", tone: band("gamma") },
    { key: "candidate", label: "candidate", tone: band("alpha") },
    { key: "raw", label: "raw", tone: band("theta") },
    { key: "deprecated", label: "superseded", tone: "rgba(233,237,255,0.5)" },
  ];

  const teams = [...new Set(data.byKind.map((k) => k.team))].sort();
  const kinds = [...new Set(data.byKind.map((k) => k.kind))].sort();
  const kindCount = (kind: string, team: string) =>
    data.byKind.find((k) => k.kind === kind && k.team === team)?.count ?? 0;
  const maxKind = Math.max(1, ...data.byKind.map((k) => k.count));
  const totalMemories = Object.values(data.totals).reduce((a, b) => a + b, 0);
  const promotionRate = data.review.reviewed + data.review.autoPromoted;
  const corpusTotal = STATUS_ROWS.reduce((s, r) => s + (data.totals[r.key] ?? 0), 0);

  return (
    <motion.div initial="hidden" animate="visible" variants={stagger} className="mx-auto max-w-7xl px-6 py-8">
      {/* headline gauges — live gauges with a queue behind them drill down */}
      <motion.div variants={rise} className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        {[
          { label: "canonical", value: data.totals.canonical ?? 0, tone: MINT },
          { label: "total memories", value: totalMemories, tone: "#fff" },
          { label: "pending review", value: data.review.pending, tone: data.review.pending > 5 ? MAGENTA : "#fff", href: "/console/reviews#promotions-h" },
          { label: "oldest pending", value: age(data.review.oldestSecs), tone: "#fff", href: "/console/reviews#promotions-h" },
          { label: "queue depth", value: data.queueDepth, tone: data.queueDepth > 0 ? MAGENTA : MINT },
        ].map((g) => {
          const body = (
            <>
              <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
                {g.label}
              </div>
              <div className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight`} style={{ color: g.tone }}>
                {g.value}
              </div>
            </>
          );
          return data.live && g.href ? (
            <Link key={g.label} href={g.href} className={`${TILE} block transition hover:border-white/25`}>
              {body}
            </Link>
          ) : (
            <div key={g.label} className={TILE}>
              {body}
            </div>
          );
        })}
      </motion.div>

      {/*
       * One grid, two rows, three columns:
       *
       *   ┌ knowledge flow (2) ─────┬ governance ┐
       *   ├ loudest themes  (2) ─────┤  (rows 1-2) │
       *   └──────────────────────────┴─────────────┘
       *   ┌ kind × team — its own full-width row ──┐
       *
       * Kind × team used to sit in this grid's third column, which was fine at
       * three teams and unreadable at twelve: a matrix's width grows with the
       * org, so it cannot share a row with anything. It gets the full width
       * below. Governance inherits the space that frees up — it is a list of
       * numbers, so it grows down happily where the matrix could not grow across.
       */}
      <div className="mt-3 grid gap-3 lg:grid-cols-3">
        {/* flow */}
        <motion.div variants={rise} className={`${TILE} lg:col-span-2`}>
          <div className="flex items-baseline justify-between">
            <h2 className={`${FONT_DISPLAY} text-lg font-semibold text-white`}>Knowledge flow</h2>
            <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
              captured vs promoted / week{data.weeklyIsDemo ? " · demo trend" : ""}
            </span>
          </div>
          <div className="mt-3 h-52">
            <ResponsiveContainer>
              <AreaChart data={data.weekly} margin={{ top: 4, right: 4, left: -24, bottom: 0 }}>
                <defs>
                  <linearGradient id="mc-cap" x1="0" y1="0" x2="0" y2="1">
                    <stop stopColor={MINT} stopOpacity={0.4} />
                    <stop offset="1" stopColor={MINT} stopOpacity={0} />
                  </linearGradient>
                </defs>
                <XAxis dataKey="week" stroke="rgba(233,237,255,0.25)" fontSize={11} tickLine={false} axisLine={false} />
                <YAxis stroke="rgba(233,237,255,0.25)" fontSize={11} tickLine={false} axisLine={false} />
                <Tooltip content={<ChartTip />} cursor={{ stroke: "rgba(255,255,255,0.15)" }} />
                <Area type="monotone" dataKey="captured" stroke={MINT} strokeWidth={2} fill="url(#mc-cap)" />
                <Area type="monotone" dataKey="promoted" stroke="#fff" strokeWidth={1.5} strokeDasharray="4 3" fill="none" />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </motion.div>

        {/* governance column — spans both rows of this grid */}
        <motion.div variants={rise} className={`${TILE} lg:row-span-2 lg:flex lg:flex-col`}>
          <h2 className={`${FONT_DISPLAY} text-lg font-semibold text-white`}>Governance</h2>
          <dl className={`${FONT_MONO} mt-3 space-y-2.5 text-sm`}>
            <div className="flex justify-between">
              <dt className="text-white/45">reviewed by humans</dt>
              <dd style={{ color: MINT }}>{data.review.reviewed}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-white/45">auto-promoted by policy</dt>
              <dd className="text-white/85">{data.review.autoPromoted}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-white/45">avg review latency</dt>
              <dd className="text-white/85">{age(data.review.avgLatencySecs)}</dd>
            </div>
            <div className="flex justify-between border-t border-white/10 pt-2.5">
              <dt className="text-white/45">contradictions open</dt>
              <dd style={{ color: (data.contradictions.open ?? 0) > 0 ? MAGENTA : MINT }}>
                {data.live ? (
                  <Link href="/console/reviews?cstatus=open#contradictions-h" className="underline decoration-white/20 underline-offset-4 transition hover:decoration-white/60">
                    {data.contradictions.open ?? 0}
                  </Link>
                ) : (
                  data.contradictions.open ?? 0
                )}
              </dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-white/45">superseded</dt>
              <dd className="text-white/85">
                {data.live ? (
                  <Link href="/console/reviews?cstatus=resolved_supersede#contradictions-h" className="underline decoration-white/20 underline-offset-4 transition hover:decoration-white/60">
                    {data.contradictions.resolved_supersede ?? 0}
                  </Link>
                ) : (
                  data.contradictions.resolved_supersede ?? 0
                )}
              </dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-white/45">coexist / dismissed</dt>
              <dd className="text-white/85">
                {data.live ? (
                  <Link href="/console/reviews?cstatus=all#contradictions-h" className="underline decoration-white/20 underline-offset-4 transition hover:decoration-white/60">
                    {(data.contradictions.resolved_coexist ?? 0) + (data.contradictions.dismissed ?? 0)}
                  </Link>
                ) : (
                  (data.contradictions.resolved_coexist ?? 0) + (data.contradictions.dismissed ?? 0)
                )}
              </dd>
            </div>
          </dl>

          {/*
           * The corpus itself. This column got twice the height when the matrix
           * moved out, and the honest way to spend it is the breakdown the tiles
           * above only total: how much of what the org "knows" has actually been
           * signed for, versus still sitting raw.
           *
           * Everything here is already in the observatory payload. The stats a
           * governance panel most wants — rubber-stamp rate, contradiction
           * dismiss rate, flagged memories — are computed by the server but only
           * exposed on /v1/analytics, not /v1/observatory, so surfacing them is
           * an API change rather than a UI one. Worth doing; not smuggled in here.
           */}
          <div className="mt-5 border-t border-white/10 pt-4">
            <div className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
              the corpus
            </div>
            <dl className={`${FONT_MONO} mt-3 space-y-2.5 text-sm`}>
              {STATUS_ROWS.map((s) => {
                const n = data.totals[s.key] ?? 0;
                const share = corpusTotal ? Math.round((n / corpusTotal) * 100) : 0;
                return (
                  <div key={s.key} className="flex items-baseline justify-between gap-3">
                    <dt className="text-white/45">{s.label}</dt>
                    <dd className="flex items-baseline gap-2">
                      {/* The bar makes the pyramid legible: a corpus that is 80%
                          canonical is governed; one that is 80% raw is a pile. */}
                      <span
                        className="hidden h-1 rounded-full sm:inline-block"
                        style={{
                          width: `${Math.max(2, share * 0.6)}px`,
                          background: s.tone,
                          opacity: n ? 0.75 : 0.15,
                        }}
                      />
                      <span style={{ color: n ? s.tone : "rgba(233,237,255,0.25)" }}>{n}</span>
                      <span className="w-9 text-right text-white/30">{share}%</span>
                    </dd>
                  </div>
                );
              })}
            </dl>
          </div>

          {/* mt-auto: the footer sits at the bottom of the taller column rather
              than floating under the list. */}
          <div
            className={`${LABEL} mt-4 border-t border-white/10 pt-3 lg:mt-auto`}
            style={{ color: "rgba(233,237,255,0.35)" }}
          >
            {promotionRate} promotions ledgered · SLO &lt; 48h
          </div>
        </motion.div>

        {/* themes — row 2 of the same grid, beside the governance column */}
        <motion.div variants={rise} className={`${TILE} lg:col-span-2`}>
          <div className="flex items-baseline justify-between">
            <h2 className={`${FONT_DISPLAY} text-lg font-semibold text-white`}>Loudest themes</h2>
            <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
              canonical entities · anchored memories · team spread
              {data.live && (
                <>
                  {" · "}
                  <Link href="/console/graph" className="transition hover:text-white" style={{ color: MINT }}>
                    explore graph →
                  </Link>
                </>
              )}
            </span>
          </div>
          <div className="mt-3 h-64">
            <ResponsiveContainer>
              <BarChart data={data.topEntities.slice(0, 8)} layout="vertical" margin={{ top: 0, right: 46, left: 14, bottom: 0 }}>
                <XAxis type="number" hide />
                <YAxis
                  type="category"
                  dataKey="name"
                  width={110}
                  tickLine={false}
                  axisLine={false}
                  fontSize={12}
                  stroke="rgba(233,237,255,0.7)"
                />
                <Tooltip content={<ChartTip />} cursor={{ fill: "rgba(255,255,255,0.04)" }} />
                <Bar dataKey="memories" barSize={12}>
                  {data.topEntities.slice(0, 8).map((e) => (
                    <Cell key={e.name} fill={e.teams >= 3 ? MINT : e.teams === 2 ? MINT_DIM : "rgba(233,237,255,0.25)"} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          </div>
          <div className={`${LABEL} mt-1`} style={{ color: "rgba(233,237,255,0.35)" }}>
            <span style={{ color: MINT }}>■</span> 3 teams · <span style={{ color: MINT_DIM }}>■</span> 2 teams ·{" "}
            <span className="text-white/25">■</span> 1 team — brightness = binding
          </div>
        </motion.div>
      </div>

      {/* kind × team matrix — its own full-width row (see the note above) */}
      <div className="mt-3">
        <motion.div variants={rise} className={TILE}>
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <h2 className={`${FONT_DISPLAY} text-lg font-semibold text-white`}>Kind × team</h2>
            <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
              {teams.length} teams · where each kind of knowledge lives
            </span>
          </div>
          {/* A matrix gets one more column per team the org grows, so the width
              is not ours to control — it scrolls in its own box rather than
              crushing the labels or pushing the page sideways. */}
          <div className="mt-3 overflow-x-auto">
            <table className={`${FONT_MONO} w-full min-w-[42rem] text-xs`}>
            <thead>
              <tr>
                <th className="pb-2 text-left font-normal text-white/35"></th>
                {teams.map((t) => (
                  <th key={t} className={`${LABEL} pb-2 text-right font-normal`} style={{ color: "rgba(233,237,255,0.4)" }} title={t}>
                    {t.slice(0, 4)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {kinds.map((k) => (
                <tr key={k} className="border-t border-white/5">
                  <td className="py-2 text-white/60">{k}</td>
                  {teams.map((t) => {
                    const n = kindCount(k, t);
                    const heat = n / maxKind;
                    return (
                      <td key={t} className="py-2 text-right">
                        <span
                          className="inline-block min-w-8 rounded px-1.5 py-0.5 text-right"
                          style={{
                            background: n ? band("beta", 60, 0.08 + heat * 0.3) : "transparent",
                            color: n ? band("beta", 78) : "rgba(233,237,255,0.2)",
                            boxShadow: heat > 0.7 ? `0 0 12px ${bandGlow("beta", 0.25)}` : undefined,
                          }}
                        >
                          {n || "·"}
                        </span>
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
            </table>
          </div>
          <div className={`${LABEL} mt-3 border-t border-white/10 pt-3`} style={{ color: "rgba(233,237,255,0.35)" }}>
            embeddings: {data.embeddingModel}
            {!data.live && " · demo data"}
          </div>
        </motion.div>
      </div>
    </motion.div>
  );
}
