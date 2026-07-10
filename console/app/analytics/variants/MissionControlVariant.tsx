"use client";

/*
 * Observatory variant A — "Mission Control". Mental model: an operations
 * wall — every gauge visible at once, tiled small multiples, dense mono
 * instrumentation. You stand back and scan; nothing requires scrolling a
 * story. Beta band (active recall) accent throughout.
 */

import { motion } from "framer-motion";
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

import type { ObservatoryData } from "../observatory-data";

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

export default function MissionControlVariant({ data }: { data: ObservatoryData }) {
  const teams = [...new Set(data.byKind.map((k) => k.team))].sort();
  const kinds = [...new Set(data.byKind.map((k) => k.kind))].sort();
  const kindCount = (kind: string, team: string) =>
    data.byKind.find((k) => k.kind === kind && k.team === team)?.count ?? 0;
  const maxKind = Math.max(1, ...data.byKind.map((k) => k.count));
  const totalMemories = Object.values(data.totals).reduce((a, b) => a + b, 0);
  const promotionRate = data.review.reviewed + data.review.autoPromoted;

  return (
    <motion.div initial="hidden" animate="visible" variants={stagger} className="mx-auto max-w-7xl px-6 py-8">
      {/* headline gauges */}
      <motion.div variants={rise} className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        {[
          { label: "canonical", value: data.totals.canonical ?? 0, tone: MINT },
          { label: "total memories", value: totalMemories, tone: "#fff" },
          { label: "pending review", value: data.review.pending, tone: data.review.pending > 5 ? MAGENTA : "#fff" },
          { label: "oldest pending", value: age(data.review.oldestSecs), tone: "#fff" },
          { label: "queue depth", value: data.queueDepth, tone: data.queueDepth > 0 ? MAGENTA : MINT },
        ].map((g) => (
          <div key={g.label} className={TILE}>
            <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
              {g.label}
            </div>
            <div className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight`} style={{ color: g.tone }}>
              {g.value}
            </div>
          </div>
        ))}
      </motion.div>

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

        {/* governance column */}
        <motion.div variants={rise} className={TILE}>
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
                {data.contradictions.open ?? 0}
              </dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-white/45">superseded</dt>
              <dd className="text-white/85">{data.contradictions.resolved_supersede ?? 0}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-white/45">coexist / dismissed</dt>
              <dd className="text-white/85">
                {(data.contradictions.resolved_coexist ?? 0) + (data.contradictions.dismissed ?? 0)}
              </dd>
            </div>
          </dl>
          <div className={`${LABEL} mt-4 border-t border-white/10 pt-3`} style={{ color: "rgba(233,237,255,0.35)" }}>
            {promotionRate} promotions ledgered · SLO &lt; 48h
          </div>
        </motion.div>
      </div>

      <div className="mt-3 grid gap-3 lg:grid-cols-3">
        {/* themes */}
        <motion.div variants={rise} className={`${TILE} lg:col-span-2`}>
          <div className="flex items-baseline justify-between">
            <h2 className={`${FONT_DISPLAY} text-lg font-semibold text-white`}>Loudest themes</h2>
            <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
              canonical entities · anchored memories · team spread
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

        {/* kind × team matrix */}
        <motion.div variants={rise} className={TILE}>
          <h2 className={`${FONT_DISPLAY} text-lg font-semibold text-white`}>Kind × team</h2>
          <table className={`${FONT_MONO} mt-3 w-full text-xs`}>
            <thead>
              <tr>
                <th className="pb-2 text-left font-normal text-white/35"></th>
                {teams.map((t) => (
                  <th key={t} className={`${LABEL} pb-2 text-right font-normal`} style={{ color: "rgba(233,237,255,0.4)" }}>
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
          <div className={`${LABEL} mt-3 border-t border-white/10 pt-3`} style={{ color: "rgba(233,237,255,0.35)" }}>
            embeddings: {data.embeddingModel}
            {!data.live && " · demo data"}
          </div>
        </motion.div>
      </div>
    </motion.div>
  );
}
