"use client";

/*
 * Ingest Monitor — "Airlock" view (spatial lens). Mental model: staged chambers. Knowledge
 * must pass through pressurized rooms — Intake, Extraction, Adjudication,
 * Canon — and you see exactly what's sitting in each chamber right now.
 * Failures divert to the dead-letter chamber at the end. Spatial columns
 * instead of a timeline: the question this answers is "where is
 * everything?", not "when did it happen?".
 */

import { motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import { ageLabel, stageOf, type IngestData } from "../ingest-data";
import SubmitBox from "../SubmitBox";
import { useIngestFeed } from "../useIngestFeed";

const THETA = band("theta");

const CHAMBERS = [
  { key: "intake", name: "Intake", hint: "queued for the worker" },
  { key: "extraction", name: "Extraction", hint: "distilled, resolving entities" },
  { key: "adjudication", name: "Adjudication", hint: "awaiting a human" },
  { key: "canon", name: "Canon", hint: "promoted, distributed" },
] as const;

function chamberOf(stage: number, stuck: boolean, pending: number, promoted: number): number {
  if (stuck || stage <= 1) return 0;
  if (pending > 0) return 2;
  if (promoted > 0) return 3;
  return 1;
}

export default function AirlockView({ data: initial }: { data: IngestData }) {
  const { data, refresh } = useIngestFeed(initial);

  const chambers = CHAMBERS.map((c) => ({ ...c, items: [] as (typeof data.sources[number] & { stuck: boolean })[] }));
  for (const s of data.sources) {
    const { stage, stuck } = stageOf(s);
    chambers[chamberOf(stage, stuck, s.pending_review, s.promoted)].items.push({ ...s, stuck });
  }

  return (
    <div className="mx-auto max-w-7xl px-6 py-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: THETA }}>
            θ · ingest monitor · airlock
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            Nothing reaches canon without passing the chambers.
          </h1>
        </div>
        <SubmitBox live={data.live} onSubmitted={refresh} />
      </div>

      <div className="mt-4 grid gap-3 lg:grid-cols-[repeat(4,1fr)_180px]">
        {chambers.map((c, ci) => {
          // Density mode: past ~10 occupants a chamber compresses to
          // single-line cards and scrolls internally instead of growing.
          const dense = c.items.length > 10;
          return (
            <div key={c.key} className="flex min-h-[380px] flex-col rounded-xl border border-white/10 bg-white/[0.015]">
              <div className="border-b border-white/10 px-4 py-3">
                <div className="flex items-baseline justify-between">
                  <span className={`${FONT_DISPLAY} font-semibold text-white`}>{c.name}</span>
                  <span className={`${FONT_MONO} text-xs`} style={{ color: c.items.some((s) => s.stuck) ? MAGENTA : THETA }}>
                    {c.items.length}
                  </span>
                </div>
                <div className={`${LABEL} mt-0.5`} style={{ color: "rgba(233,237,255,0.3)" }}>{c.hint}</div>
              </div>
              <div className={`max-h-[460px] flex-1 overflow-y-auto p-3 ${dense ? "space-y-1" : "space-y-2"}`}>
                {c.items.map((s, i) => (
                  <motion.div
                    key={s.id}
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.25, delay: Math.min(0.3, i * 0.02) }}
                    className={`rounded-lg border ${dense ? "px-2.5 py-1.5" : "p-2.5"}`}
                    style={{
                      borderColor: s.stuck ? "rgba(255,93,162,0.4)" : "rgba(233,237,255,0.1)",
                      background: ci === 3 ? "hsla(46,90%,60%,0.04)" : "rgba(255,255,255,0.015)",
                    }}
                    title={`${s.external_ref ?? s.kind} · ${s.team ?? "—"} · ${s.status} · ${s.memories} memories`}
                  >
                    {dense ? (
                      <div className={`${FONT_MONO} flex items-baseline gap-2 text-xs`}>
                        <span className="truncate text-[#e9edff]/75">{s.external_ref ?? `${s.kind} capture`}</span>
                        <span className="ml-auto shrink-0 text-[10px] text-[#e9edff]/35">
                          {s.stuck ? <span style={{ color: MAGENTA }}>{s.status}</span> : ageLabel(s.created_at)}
                          {s.memories > 0 && <span style={{ color: ci === 3 ? band("gamma") : THETA }}> · {s.memories}</span>}
                        </span>
                      </div>
                    ) : (
                      <>
                        <div className={`${FONT_MONO} truncate text-xs text-[#e9edff]/80`}>
                          {s.external_ref ?? `${s.kind} capture`}
                        </div>
                        <div className={`${FONT_MONO} mt-1 text-[10px] uppercase tracking-widest text-[#e9edff]/35`}>
                          {s.team ?? "—"} · {ageLabel(s.created_at)}
                          {s.stuck && <span style={{ color: MAGENTA }}> · {s.status}</span>}
                          {s.memories > 0 && <span style={{ color: ci === 3 ? band("gamma") : THETA }}> · {s.memories} mem</span>}
                        </div>
                      </>
                    )}
                  </motion.div>
                ))}
                {c.items.length === 0 && (
                  <p className={`${FONT_MONO} pt-8 text-center text-xs text-[#e9edff]/25`}>chamber empty</p>
                )}
              </div>
            </div>
          );
        })}

        {/* dead-letter chamber */}
        <div
          className="flex min-h-[380px] flex-col rounded-xl border p-4"
          style={{ borderColor: data.health.dead_letters > 0 ? "rgba(255,93,162,0.45)" : "rgba(233,237,255,0.1)" }}
        >
          <div className={LABEL} style={{ color: data.health.dead_letters > 0 ? MAGENTA : "rgba(233,237,255,0.35)" }}>
            dead letters
          </div>
          <div className={`${FONT_DISPLAY} mt-1 text-4xl font-semibold`} style={{ color: data.health.dead_letters > 0 ? MAGENTA : "rgba(233,237,255,0.4)" }}>
            {data.health.dead_letters}
          </div>
          <div className={`${FONT_MONO} mt-3 space-y-1.5 text-xs text-[#e9edff]/50`}>
            <div>queue ready: {data.health.ready}</div>
            <div>in flight: {data.health.in_flight}</div>
            <div>oldest wait: {Math.round(data.health.oldest_ready_secs)}s</div>
            <div>archived ok: {data.health.archived.ok}</div>
            <div>archived failed: {data.health.archived.failed}</div>
          </div>
          <p className={`${FONT_MONO} mt-auto text-[10px] leading-relaxed text-[#e9edff]/30`}>
            requeue via <code>POST /v1/queue/dead-letters/:id/requeue</code>
            {!data.live && " · demo data"}
          </p>
        </div>
      </div>
    </div>
  );
}
