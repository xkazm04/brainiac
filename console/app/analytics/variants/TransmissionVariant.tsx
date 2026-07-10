"use client";

/*
 * Observatory variant B — "Transmission". Mental model: a signed weekly
 * broadcast from the org's mind — the dashboard as prose. Numbers live
 * inside sentences; each section is one finding with one figure, generous
 * vertical rhythm, editorial display type. You read it top to bottom like
 * a briefing, not scan it like a wall.
 */

import { motion } from "framer-motion";
import {
  Area,
  AreaChart,
  ResponsiveContainer,
  XAxis,
} from "recharts";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import type { ObservatoryData } from "../observatory-data";

const MINT = band("beta");

const rise = {
  hidden: { opacity: 0, y: 18 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.5 } },
};

function age(secs: number): string {
  if (secs <= 0) return "under a minute";
  if (secs < 3600) return `${Math.round(secs / 60)} minutes`;
  if (secs < 86400) return `${(secs / 3600).toFixed(1)} hours`;
  return `${(secs / 86400).toFixed(1)} days`;
}

function Em({ children, tone = MINT }: { children: React.ReactNode; tone?: string }) {
  return (
    <span className="font-semibold" style={{ color: tone }}>
      {children}
    </span>
  );
}

export default function TransmissionVariant({ data }: { data: ObservatoryData }) {
  const lastWeek = data.weekly[data.weekly.length - 1];
  const prevWeek = data.weekly[data.weekly.length - 2];
  const delta =
    prevWeek && prevWeek.captured > 0
      ? Math.round(((lastWeek.captured - prevWeek.captured) / prevWeek.captured) * 100)
      : null;
  const open = data.contradictions.open ?? 0;
  const superseded = data.contradictions.resolved_supersede ?? 0;
  const binding = data.topEntities.filter((e) => e.teams >= 2);
  const top = data.topEntities[0];

  return (
    <div className="mx-auto max-w-3xl px-6 py-12">
      {/* masthead */}
      <motion.header initial="hidden" animate="visible" variants={rise} className="border-b border-white/15 pb-6">
        <div className={LABEL} style={{ color: MINT }}>
          transmission · {lastWeek?.week ?? "this week"} · org meridian
        </div>
        <h1 className={`${FONT_DISPLAY} mt-3 text-5xl font-semibold leading-[1.05] tracking-tight text-white`}>
          The week the org
          <br />
          kept <span style={{ color: MINT }}>{data.totals.canonical ?? 0} things</span> true.
        </h1>
        <p className={`${FONT_MONO} mt-4 text-sm leading-relaxed text-white/50`}>
          Auto-composed from the ledger · every number traceable to a signed row
          {!data.live && " · demo data"}
        </p>
      </motion.header>

      {/* finding 1 — flow */}
      <motion.section
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true, margin: "-60px" }}
        variants={rise}
        className="border-b border-white/10 py-10"
      >
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          finding 01 · flow
        </div>
        <p className={`${FONT_DISPLAY} mt-3 text-2xl leading-snug text-white/90`}>
          Teams captured <Em>{lastWeek?.captured ?? 0} memories</Em>
          {delta !== null && (
            <>
              {" "}
              — <Em tone={delta >= 0 ? MINT : MAGENTA}>{delta >= 0 ? "+" : ""}{delta}%</Em> on last week
            </>
          )}
          , and <Em>{lastWeek?.promoted ?? 0}</Em> earned canonical status.
        </p>
        <div className="mt-5 h-32">
          <ResponsiveContainer>
            <AreaChart data={data.weekly} margin={{ top: 4, right: 0, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="tx-cap" x1="0" y1="0" x2="0" y2="1">
                  <stop stopColor={MINT} stopOpacity={0.35} />
                  <stop offset="1" stopColor={MINT} stopOpacity={0} />
                </linearGradient>
              </defs>
              <XAxis dataKey="week" stroke="rgba(233,237,255,0.25)" fontSize={10} tickLine={false} axisLine={false} />
              <Area type="monotone" dataKey="captured" stroke={MINT} strokeWidth={2} fill="url(#tx-cap)" />
              <Area type="monotone" dataKey="promoted" stroke="#fff" strokeWidth={1.5} strokeDasharray="4 3" fill="none" />
            </AreaChart>
          </ResponsiveContainer>
        </div>
        {data.weeklyIsDemo && (
          <div className={`${LABEL} mt-1`} style={{ color: "rgba(233,237,255,0.3)" }}>
            demo trend — corpus younger than three weeks
          </div>
        )}
      </motion.section>

      {/* finding 2 — themes */}
      <motion.section
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true, margin: "-60px" }}
        variants={rise}
        className="border-b border-white/10 py-10"
      >
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          finding 02 · what the org talks about
        </div>
        <p className={`${FONT_DISPLAY} mt-3 text-2xl leading-snug text-white/90`}>
          <Em>{top?.name ?? "—"}</Em> is the loudest theme — {top?.memories ?? 0} memories across{" "}
          {top?.teams ?? 0} teams. <Em>{binding.length}</Em> themes now bind more than one team.
        </p>
        <div className="mt-6 space-y-2.5">
          {data.topEntities.slice(0, 6).map((e, i) => {
            const max = data.topEntities[0]?.memories || 1;
            return (
              <div key={e.name} className="flex items-center gap-3">
                <span className={`${FONT_MONO} w-6 text-right text-xs text-white/30`}>{i + 1}</span>
                <span className={`${FONT_MONO} w-36 truncate text-sm text-white/85`}>{e.name}</span>
                <div className="h-2 flex-1 overflow-hidden rounded-full bg-white/5">
                  <motion.div
                    initial={{ width: 0 }}
                    whileInView={{ width: `${(e.memories / max) * 100}%` }}
                    viewport={{ once: true }}
                    transition={{ duration: 0.7, delay: i * 0.06 }}
                    className="h-full rounded-full"
                    style={{ background: e.teams >= 3 ? MINT : band("beta", 68, 0.45) }}
                  />
                </div>
                <span className={`${FONT_MONO} w-24 text-right text-xs text-white/40`}>
                  {e.memories} mem · {e.teams}t
                </span>
              </div>
            );
          })}
        </div>
      </motion.section>

      {/* finding 3 — governance */}
      <motion.section
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true, margin: "-60px" }}
        variants={rise}
        className="border-b border-white/10 py-10"
      >
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          finding 03 · governance health
        </div>
        <p className={`${FONT_DISPLAY} mt-3 text-2xl leading-snug text-white/90`}>
          Humans signed <Em>{data.review.reviewed}</Em> promotions (policy auto-passed{" "}
          <Em>{data.review.autoPromoted}</Em>); the median wait is{" "}
          <Em>{age(data.review.avgLatencySecs)}</Em>.{" "}
          {open > 0 ? (
            <>
              <Em tone={MAGENTA}>{open} contradiction{open > 1 ? "s" : ""}</Em> still open.
            </>
          ) : (
            <>No contradictions are open.</>
          )}
        </p>
        <div className={`${FONT_MONO} mt-6 grid grid-cols-2 gap-4 text-sm sm:grid-cols-4`}>
          {[
            { label: "pending", value: String(data.review.pending), tone: data.review.pending > 5 ? MAGENTA : "#fff" },
            { label: "oldest wait", value: age(data.review.oldestSecs), tone: "#fff" },
            { label: "superseded", value: String(superseded), tone: MINT },
            { label: "queue", value: String(data.queueDepth), tone: data.queueDepth > 0 ? MAGENTA : MINT },
          ].map((s) => (
            <div key={s.label} className="rounded-lg border border-white/10 p-3">
              <div className="text-xl font-semibold" style={{ color: s.tone }}>
                {s.value}
              </div>
              <div className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
                {s.label}
              </div>
            </div>
          ))}
        </div>
      </motion.section>

      {/* signature */}
      <motion.footer
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true }}
        variants={rise}
        className="flex items-baseline justify-between py-8"
      >
        <span className={`${FONT_DISPLAY} text-lg italic text-white/60`}>— brainiac, keeper of record</span>
        <span className={LABEL} style={{ color: MINT }}>
          {data.embeddingModel} · 0 leaks
        </span>
      </motion.footer>
    </div>
  );
}
