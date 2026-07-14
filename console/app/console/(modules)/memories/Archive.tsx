"use client";

/*
 * Archive — consolidated from the 2026-07-10 prototype round ("Time
 * Scrubber" won over Card Catalog and Core Sample). Mental model: the
 * corpus as a playhead. One hero control — the as-of timeline — and the
 * whole archive re-renders to what the org KNEW at that instant:
 * superseded memories come back to life, newer decisions vanish. The
 * unique power of the temporal schema, made physical. Client-side
 * filtering over one fetched corpus = zero-latency scrubbing.
 */

import { useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import { timeBounds, validAt, type ArchiveData } from "./archive-data";
import MemoryInspector, { fmtDate, statusTone } from "./MemoryInspector";
import { useMemoryDetail } from "./useMemoryDetail";

const VIOLET = band("delta");
const VIOLET_GLOW = band("delta", 60, 0.35);

export default function Archive({ data }: { data: ArchiveData }) {
  const { min, max } = useMemo(() => timeBounds(data.rows), [data.rows]);
  const [frac, setFrac] = useState(1); // 1 = now
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading } = useMemoryDetail(selected, data.live);

  const at = useMemo(
    () => new Date(min.getTime() + (max.getTime() - min.getTime() + 86400000) * frac),
    [frac, min, max],
  );
  const atNow = frac >= 0.999;

  const visible = useMemo(
    () => data.rows.filter((r) => validAt(r, at) && r.status !== "rejected"),
    [data.rows, at],
  );
  const resurrected = visible.filter((r) => r.status === "deprecated").length;

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: VIOLET }}>
            δ · archive · time scrubber
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            What did the org know on{" "}
            <span style={{ color: VIOLET, textShadow: `0 0 28px ${VIOLET_GLOW}` }}>
              {fmtDate(at.toISOString())}
            </span>
            ?
          </h1>
        </div>
        <div className={`${FONT_MONO} text-xs text-[#e9edff]/40`}>
          {visible.length} memories true then
          {resurrected > 0 && <> · {resurrected} since superseded</>}
          {!data.live && " · demo data"}
        </div>
      </div>

      {/* the scrubber */}
      <div className="mt-5 rounded-xl border border-white/10 bg-white/[0.015] p-5">
        <input
          type="range"
          min={0}
          max={1000}
          value={Math.round(frac * 1000)}
          onChange={(e) => setFrac(Number(e.target.value) / 1000)}
          className="w-full accent-[#b9a5f5]"
          aria-label="As-of date"
        />
        <div className={`${FONT_MONO} mt-1 flex items-center justify-between text-[11px] uppercase tracking-widest text-[#e9edff]/35`}>
          <span>{fmtDate(min.toISOString())} · the beginning of record</span>
          <button
            onClick={() => setFrac(1)}
            className={`rounded-full border px-3 py-0.5 transition ${atNow ? "border-transparent text-[#e9edff]/35" : "border-white/25 text-white hover:border-white/60"}`}
            disabled={atNow}
          >
            {atNow ? "● now" : "→ jump to now"}
          </button>
        </div>
      </div>

      {/* the corpus at that instant */}
      <div className="mt-5 grid gap-6 lg:grid-cols-[1fr_0.9fr]">
        <div className="space-y-2">
          <AnimatePresence initial={false}>
            {visible.slice(0, 40).map((r) => (
              <motion.button
                key={r.id}
                layout
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.22 }}
                onClick={() => setSelected(r.id === selected ? null : r.id)}
                className={`block w-full rounded-lg border p-3.5 text-left transition ${
                  selected === r.id ? "border-[#b9a5f5]/60 bg-[#b9a5f5]/[0.06]" : "border-white/10 bg-white/[0.015] hover:border-white/25"
                }`}
              >
                <div className={`${FONT_MONO} flex items-center gap-2 text-[10px] uppercase tracking-widest`}>
                  <span style={{ color: statusTone(r.status) }}>
                    {r.status === "deprecated" ? "was true then" : r.status}
                  </span>
                  <span className="text-[#e9edff]/35">· {r.kind} · {r.team}</span>
                  <span className="ml-auto text-[#e9edff]/30">
                    {fmtDate(r.valid_from)} → {r.valid_to ? fmtDate(r.valid_to) : "now"}
                  </span>
                </div>
                <p className={`${FONT_MONO} mt-1.5 text-sm leading-snug text-[#e9edff]/80`}>{r.content}</p>
              </motion.button>
            ))}
          </AnimatePresence>
          {visible.length === 0 && (
            <p className={`${FONT_MONO} py-10 text-center text-sm text-[#e9edff]/35`}>
              nothing was known yet — scrub forward
            </p>
          )}
        </div>

        {/* inspector */}
        <div className="lg:sticky lg:top-4 lg:self-start">
          <div className="min-h-[300px] rounded-xl border border-white/10 bg-white/[0.015] p-5">
            {!selected && (
              <p className={`${FONT_MONO} py-12 text-center text-sm text-[#e9edff]/35`}>
                select a memory — its lineage, provenance and ledger open here
              </p>
            )}
            {selected && loading && <p className={`${FONT_MONO} text-sm text-[#e9edff]/40`}>opening the record…</p>}
            {selected && detail && <MemoryInspector detail={detail} onHop={setSelected} />}
          </div>
        </div>
      </div>
    </div>
  );
}
