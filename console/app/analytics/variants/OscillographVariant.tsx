"use client";

/*
 * Observatory variant C — "Oscillograph". Mental model: the dashboard as a
 * band readout — every statistic rendered as a wave whose amplitude IS the
 * number. Team channels oscillate at their capture volume; themes are
 * resonance rows; the governance section is a phase report. Data-driven
 * waveforms, drawn once on view (pathLength) — no ambient loops, per the
 * motion policy for utility pages.
 */

import { motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import type { ObservatoryData } from "../observatory-data";

const MINT = band("beta");

const rise = {
  hidden: { opacity: 0, y: 12 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.4 } },
};
const stagger = { visible: { transition: { staggerChildren: 0.07 } } };

function age(secs: number): string {
  if (secs <= 0) return "—";
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  if (secs < 86400) return `${(secs / 3600).toFixed(1)}h`;
  return `${(secs / 86400).toFixed(1)}d`;
}

/** A wave whose amplitude and cycle count are data. */
function wavePath(w: number, mid: number, amp: number, cycles: number, phase = 0): string {
  let d = "";
  for (let x = 0; x <= w; x += 3) {
    const u = x / w;
    const env = Math.sin(u * Math.PI); // taper at edges
    const y = mid + Math.sin(u * Math.PI * 2 * cycles + phase) * amp * env;
    d += x === 0 ? `M${x} ${y}` : ` L${x} ${y}`;
  }
  return d;
}

function TraceRow({
  label,
  sub,
  amp,
  cycles,
  tone,
  value,
  delay = 0,
}: {
  label: string;
  sub?: string;
  amp: number; // 0..1
  cycles: number;
  tone: string;
  value: string;
  delay?: number;
}) {
  const W = 640;
  const H = 56;
  return (
    <motion.div variants={rise} className="group flex items-center gap-4 border-b border-white/5 py-1.5">
      <div className="w-40 shrink-0">
        <div className={`${FONT_MONO} truncate text-sm text-white/85`}>{label}</div>
        {sub && (
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.3)" }}>
            {sub}
          </div>
        )}
      </div>
      <svg viewBox={`0 0 ${W} ${H}`} className="h-12 min-w-0 flex-1" preserveAspectRatio="none" aria-hidden>
        <line x1="0" y1={H / 2} x2={W} y2={H / 2} stroke="rgba(233,237,255,0.07)" strokeWidth="1" />
        <motion.path
          d={wavePath(W, H / 2, 4 + amp * 22, cycles)}
          fill="none"
          stroke={tone}
          strokeWidth="1.6"
          initial={{ pathLength: 0, opacity: 0 }}
          whileInView={{ pathLength: 1, opacity: 1 }}
          viewport={{ once: true, amount: 0.6 }}
          transition={{ duration: 0.9, delay }}
        />
      </svg>
      <div className={`${FONT_MONO} w-24 shrink-0 text-right text-sm`} style={{ color: tone }}>
        {value}
      </div>
    </motion.div>
  );
}

export default function OscillographVariant({ data }: { data: ObservatoryData }) {
  const teams = [...new Set(data.byKind.map((k) => k.team))].sort();
  const teamTotals = teams.map((t) => ({
    team: t,
    count: data.byKind.filter((k) => k.team === t).reduce((s, k) => s + k.count, 0),
  }));
  const maxTeam = Math.max(1, ...teamTotals.map((t) => t.count));
  const maxTheme = Math.max(1, ...data.topEntities.map((e) => e.memories));
  const open = data.contradictions.open ?? 0;
  const lastWeek = data.weekly[data.weekly.length - 1];

  return (
    <div className="mx-auto max-w-5xl px-6 py-10">
      {/* header readout */}
      <motion.div initial="hidden" animate="visible" variants={rise} className="flex flex-wrap items-end justify-between gap-4 border-b border-white/15 pb-5">
        <div>
          <div className={LABEL} style={{ color: MINT }}>
            β · observatory · gain auto
          </div>
          <h1 className={`${FONT_DISPLAY} mt-2 text-4xl font-semibold tracking-tight text-white`}>
            Reading the org at working speed.
          </h1>
        </div>
        <div className={`${FONT_MONO} text-right text-xs text-white/40`}>
          <div>
            <span style={{ color: MINT }}>{data.totals.canonical ?? 0}</span> canonical ·{" "}
            {data.review.pending} pending · queue {data.queueDepth}
          </div>
          <div className="mt-1">
            {data.embeddingModel}
            {!data.live && " · demo data"}
          </div>
        </div>
      </motion.div>

      {/* team channels */}
      <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="pt-8">
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          ch 01–0{teams.length} · team emission — amplitude = memories held
        </div>
        <div className="mt-3">
          {teamTotals.map((t, i) => (
            <TraceRow
              key={t.team}
              label={t.team}
              sub={`ch 0${i + 1}`}
              amp={t.count / maxTeam}
              cycles={3 + i * 1.5}
              tone={MINT}
              value={`${t.count} mem`}
              delay={i * 0.12}
            />
          ))}
          {/* the sum — this week's flow */}
          <TraceRow
            label="Σ this week"
            sub={data.weeklyIsDemo ? "demo trend" : lastWeek?.week}
            amp={1}
            cycles={8}
            tone="#ffffff"
            value={`${lastWeek?.captured ?? 0} / ${lastWeek?.promoted ?? 0}↑`}
            delay={teams.length * 0.12}
          />
        </div>
      </motion.section>

      {/* resonance — themes */}
      <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="pt-10">
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          resonance · themes the org keeps returning to — brightness = team binding
        </div>
        <div className="mt-3">
          {data.topEntities.slice(0, 7).map((e, i) => (
            <TraceRow
              key={e.name}
              label={e.name}
              sub={e.kind}
              amp={e.memories / maxTheme}
              cycles={2 + e.teams * 2}
              tone={e.teams >= 3 ? band("gamma") : e.teams === 2 ? MINT : band("beta", 68, 0.45)}
              value={`${e.memories} · ${e.teams}t`}
              delay={i * 0.08}
            />
          ))}
        </div>
        <div className={`${LABEL} mt-2`} style={{ color: "rgba(233,237,255,0.3)" }}>
          <span style={{ color: band("gamma") }}>gold</span> = binds all three teams (γ) ·{" "}
          <span style={{ color: MINT }}>mint</span> = two · dim = one
        </div>
      </motion.section>

      {/* phase report — governance */}
      <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="pt-10 pb-6">
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          phase report · governance
        </div>
        <motion.div variants={rise} className={`${FONT_MONO} mt-3 grid gap-px overflow-hidden rounded-lg border border-white/10 bg-white/10 sm:grid-cols-2 lg:grid-cols-4`}>
          {[
            {
              label: "in phase — reviewed",
              value: `${data.review.reviewed} signed`,
              sub: `avg wait ${age(data.review.avgLatencySecs)}`,
              tone: MINT,
            },
            {
              label: "policy auto-passed",
              value: `${data.review.autoPromoted}`,
              sub: "high-confidence pitfalls & decisions",
              tone: "#fff",
            },
            {
              label: open > 0 ? "out of phase — open" : "no open contradictions",
              value: `${open}`,
              sub: `${data.contradictions.resolved_supersede ?? 0} superseded to date`,
              tone: open > 0 ? MAGENTA : MINT,
            },
            {
              label: "awaiting a human",
              value: `${data.review.pending}`,
              sub: `oldest ${age(data.review.oldestSecs)}`,
              tone: data.review.pending > 5 ? MAGENTA : "#fff",
            },
          ].map((c) => (
            <div key={c.label} className="bg-[#0a090f] p-4">
              <div className="text-2xl font-semibold" style={{ color: c.tone }}>
                {c.value}
              </div>
              <div className={`${LABEL} mt-1`} style={{ color: "rgba(233,237,255,0.4)" }}>
                {c.label}
              </div>
              <div className="mt-1 text-xs text-white/35">{c.sub}</div>
            </div>
          ))}
        </motion.div>
      </motion.section>
    </div>
  );
}
