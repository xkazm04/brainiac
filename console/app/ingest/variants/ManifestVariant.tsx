"use client";

/*
 * Ingest variant B — "Manifest". Mental model: parcel tracking. Every
 * source is a tracked shipment with stage checkpoints; the manifest is a
 * dense table an operator can scan top to bottom. The worker log rides
 * along as the depot's activity feed.
 */

import { motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import { ageLabel, STAGES, stageOf, type IngestData } from "../ingest-data";
import SubmitBox from "../SubmitBox";
import { useIngestFeed } from "../useIngestFeed";

const THETA = band("theta");

export default function ManifestVariant({ data: initial }: { data: IngestData }) {
  const { data, refresh } = useIngestFeed(initial);

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: THETA }}>
            θ · ingest monitor · manifest
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            Every capture, tracked like a shipment.
          </h1>
        </div>
        <SubmitBox live={data.live} onSubmitted={refresh} />
      </div>

      {/* manifest table */}
      <div className={`${FONT_MONO} mt-4 overflow-hidden rounded-xl border border-white/10`}>
        <div className={`${LABEL} grid grid-cols-[1fr_110px_190px_140px] gap-3 border-b border-white/10 bg-white/[0.02] px-4 py-2.5`} style={{ color: "rgba(233,237,255,0.4)" }}>
          <span>shipment</span>
          <span>status</span>
          <span>checkpoints</span>
          <span className="text-right">yield</span>
        </div>
        {data.sources.map((s, i) => {
          const { stage, stuck } = stageOf(s);
          return (
            <motion.div
              key={s.id}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.25, delay: Math.min(0.4, i * 0.03) }}
              className="grid grid-cols-[1fr_110px_190px_140px] items-center gap-3 border-b border-white/[0.05] px-4 py-2.5 text-sm transition hover:bg-white/[0.02] max-sm:grid-cols-1"
            >
              <div className="min-w-0">
                <div className="truncate text-[#e9edff]/85">{s.external_ref ?? `${s.kind} capture`}</div>
                <div className="text-[10px] uppercase tracking-widest text-[#e9edff]/30">
                  {s.team ?? "—"} · {ageLabel(s.created_at)} ago
                  {(s.attempts ?? 0) > 0 && <span style={{ color: MAGENTA }}> · attempt {s.attempts}</span>}
                </div>
              </div>
              <span
                className="text-[10px] uppercase tracking-widest"
                style={{ color: stuck ? MAGENTA : s.status === "processed" ? band("gamma") : THETA }}
              >
                {s.status}
              </span>
              <div className="flex items-center gap-1">
                {STAGES.map((st, k) => (
                  <span key={st} title={st} className="flex items-center gap-1">
                    <span
                      className="grid h-4 w-4 place-items-center rounded-full border text-[9px]"
                      style={{
                        borderColor: k <= stage ? (stuck && k === stage ? MAGENTA : band("theta", 68, 0.7)) : "rgba(233,237,255,0.15)",
                        color: k <= stage ? (stuck && k === stage ? MAGENTA : band("theta", 78)) : "rgba(233,237,255,0.2)",
                        background: k < stage ? band("theta", 60, 0.12) : "transparent",
                      }}
                    >
                      {k < stage ? "✓" : stuck && k === stage ? "!" : ""}
                    </span>
                    {k < STAGES.length - 1 && <span className="h-px w-1.5 bg-white/10" />}
                  </span>
                ))}
              </div>
              <span className="text-right text-xs text-[#e9edff]/55">
                {s.memories} mem · {s.promoted}↑{s.pending_review > 0 && <span style={{ color: THETA }}> · {s.pending_review} review</span>}
              </span>
            </motion.div>
          );
        })}
      </div>

      {/* depot activity */}
      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <div className="rounded-xl border border-white/10 bg-white/[0.015] p-4">
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>depot log · pipeline runs</div>
          <ul className={`${FONT_MONO} mt-2 space-y-1.5 text-xs`}>
            {data.runs.slice(0, 8).map((r) => (
              <li key={r.id} className="flex items-baseline gap-2 text-[#e9edff]/60">
                <span style={{ color: r.status === "ok" ? THETA : MAGENTA }}>{r.status === "ok" ? "●" : "✕"}</span>
                <span className="text-[#e9edff]/80">{r.stage}</span>
                <span className="truncate">{r.detail ?? ""}</span>
                <span className="ml-auto shrink-0 text-[#e9edff]/30">{r.duration_secs}s · {ageLabel(r.started_at)}</span>
              </li>
            ))}
            {data.runs.length === 0 && <li className="text-[#e9edff]/35">no runs recorded — is the worker running?</li>}
          </ul>
        </div>
        <div className={`${FONT_MONO} rounded-xl border border-white/10 bg-white/[0.015] p-4 text-sm`}>
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>dock status</div>
          <div className="mt-2 grid grid-cols-2 gap-y-1.5 text-[#e9edff]/70">
            <span>waiting at dock</span><span style={{ color: data.health.ready > 3 ? MAGENTA : THETA }}>{data.health.ready}</span>
            <span>being unloaded</span><span>{data.health.in_flight}</span>
            <span>delivered ok</span><span>{data.health.archived.ok}</span>
            <span>lost parcels</span><span style={{ color: data.health.dead_letters > 0 ? MAGENTA : "inherit" }}>{data.health.dead_letters}</span>
          </div>
          {!data.live && <div className={`${LABEL} mt-3`} style={{ color: "rgba(233,237,255,0.3)" }}>demo data</div>}
        </div>
      </div>
    </div>
  );
}
