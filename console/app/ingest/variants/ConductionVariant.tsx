"use client";

/*
 * Ingest variant A — "Conduction". The EEG idea from the identity lab,
 * reborn as a monitor: six lanes (capture → distribute), every source a
 * pulse positioned by recency, sitting in the lane of its current stage.
 * Stuck sources flare magenta on their lane. Data-gated motion only —
 * pulses redraw when the feed refreshes, nothing loops.
 */

import { useMemo, useState } from "react";
import { motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import { ageLabel, STAGES, stageOf, type IngestData } from "../ingest-data";
import SubmitBox from "../SubmitBox";
import { useIngestFeed } from "../useIngestFeed";

const THETA = band("theta");
const W = 1000;
const LANE_H = 64;
const LABEL_W = 120;

export default function ConductionVariant({ data: initial }: { data: IngestData }) {
  const { data, refresh } = useIngestFeed(initial);
  const [selected, setSelected] = useState<string | null>(null);

  // x by age: newest right (the "now" edge), log scale over 48h.
  const pulses = useMemo(() => {
    const now = Date.now();
    return data.sources.map((s) => {
      const ageMin = Math.max(0.5, (now - new Date(s.created_at).getTime()) / 60000);
      const frac = Math.min(1, Math.log10(ageMin + 1) / Math.log10(2880));
      const { stage, stuck } = stageOf(s);
      return { ...s, stage, stuck, x: LABEL_W + (1 - frac) * (W - LABEL_W - 40) };
    });
  }, [data.sources]);

  const H = STAGES.length * LANE_H;
  const sel = pulses.find((p) => p.id === selected);

  return (
    <div className="mx-auto max-w-7xl px-6 py-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: THETA }}>
            θ · ingest monitor · conduction
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            Watch knowledge conduct through the pipeline.
          </h1>
        </div>
        <SubmitBox live={data.live} onSubmitted={refresh} />
      </div>

      {/* the strip */}
      <div className="relative mt-4 overflow-hidden rounded-xl border border-white/10 bg-white/[0.015]">
        <svg viewBox={`0 0 ${W} ${H}`} className="h-auto w-full" role="img" aria-label="Pipeline conduction strip">
          {STAGES.map((stage, i) => {
            const y = i * LANE_H + LANE_H / 2;
            return (
              <g key={stage}>
                <line x1={LABEL_W} y1={y} x2={W - 20} y2={y} stroke="rgba(233,237,255,0.08)" strokeWidth="1" />
                <text x={16} y={y + 4} fontSize="11" fill="rgba(233,237,255,0.45)" style={{ textTransform: "uppercase", letterSpacing: "0.16em" }}>
                  {String(i + 1).padStart(2, "0")} {stage}
                </text>
              </g>
            );
          })}
          {/* now edge */}
          <line x1={W - 30} y1={0} x2={W - 30} y2={H} stroke={THETA} strokeOpacity="0.25" strokeDasharray="3 5" />
          <text x={W - 36} y={14} fontSize="10" fill={THETA} textAnchor="end" style={{ textTransform: "uppercase", letterSpacing: "0.16em" }}>
            now
          </text>
          {/* conduction traces + pulses */}
          {pulses.map((p) => {
            const y = p.stage * LANE_H + LANE_H / 2;
            const isSel = selected === p.id;
            const tone = p.stuck ? MAGENTA : p.stage >= 4 ? band("gamma") : THETA;
            return (
              <g key={p.id} className="cursor-pointer" onClick={() => setSelected(isSel ? null : p.id)}>
                <title>{`${p.external_ref ?? p.kind} · ${p.status} · ${p.memories} memories · ${ageLabel(p.created_at)} ago`}</title>
                {/* trace of the path already conducted */}
                <motion.path
                  d={`M ${p.x} ${LANE_H / 2} ${STAGES.slice(1, p.stage + 1)
                    .map((_, k) => `L ${p.x} ${(k + 1) * LANE_H + LANE_H / 2}`)
                    .join(" ")}`}
                  fill="none"
                  stroke={tone}
                  strokeOpacity={isSel ? 0.55 : 0.18}
                  strokeWidth="1.2"
                  initial={{ pathLength: 0 }}
                  animate={{ pathLength: 1 }}
                  transition={{ duration: 0.6 }}
                />
                {isSel && <circle cx={p.x} cy={y} r={12} fill="none" stroke={tone} strokeWidth="1.2" strokeDasharray="3 4" />}
                <motion.circle
                  cx={p.x}
                  cy={y}
                  r={4 + Math.min(5, p.memories)}
                  fill={tone}
                  fillOpacity={p.stuck ? 0.9 : 0.75}
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  transition={{ type: "spring", bounce: 0.4, duration: 0.5 }}
                  style={{ transformOrigin: `${p.x}px ${y}px` }}
                />
              </g>
            );
          })}
        </svg>
        <div className={`${LABEL} absolute bottom-2 left-4`} style={{ color: "rgba(233,237,255,0.3)" }}>
          pulse size = memories extracted · gold = reached governance · magenta = stuck
          {!data.live && " · demo data"}
        </div>
      </div>

      {/* queue meter + selection */}
      <div className="mt-4 grid gap-4 lg:grid-cols-3">
        <div className={`${FONT_MONO} rounded-xl border border-white/10 bg-white/[0.015] p-4 text-sm`}>
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>queue</div>
          <div className="mt-2 space-y-1.5 text-[#e9edff]/70">
            <div>ready <span style={{ color: data.health.ready > 3 ? MAGENTA : THETA }}>{data.health.ready}</span> · in flight {data.health.in_flight}</div>
            <div>oldest waiting {Math.round(data.health.oldest_ready_secs)}s</div>
            <div>archived ok {data.health.archived.ok} · failed {data.health.archived.failed}</div>
            <div>dead letters <span style={{ color: data.health.dead_letters > 0 ? MAGENTA : "inherit" }}>{data.health.dead_letters}</span></div>
          </div>
        </div>
        <div className="rounded-xl border border-white/10 bg-white/[0.015] p-4 lg:col-span-2">
          {!sel && (
            <p className={`${FONT_MONO} py-6 text-center text-sm text-[#e9edff]/35`}>
              click a pulse — its conduction record opens here
            </p>
          )}
          {sel && (
            <div className={`${FONT_MONO} text-sm`}>
              <div className="flex items-baseline justify-between">
                <span className="text-white">{sel.external_ref ?? `${sel.kind} capture`}</span>
                <span className={LABEL} style={{ color: sel.stuck ? MAGENTA : THETA }}>
                  {sel.status} · {ageLabel(sel.created_at)} ago
                </span>
              </div>
              <div className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1 text-[#e9edff]/60 sm:grid-cols-4">
                <span>team: {sel.team ?? "—"}</span>
                <span>extracted: {sel.memories}</span>
                <span>promoted: {sel.promoted}</span>
                <span>awaiting review: {sel.pending_review}</span>
              </div>
              <div className="mt-3 flex items-center gap-1.5">
                {STAGES.map((stage, i) => (
                  <span
                    key={stage}
                    className="h-1.5 flex-1 rounded-full"
                    style={{
                      background:
                        i <= sel.stage
                          ? sel.stuck && i === sel.stage
                            ? MAGENTA
                            : band("theta", 68, 0.8)
                          : "rgba(233,237,255,0.1)",
                    }}
                    title={stage}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
