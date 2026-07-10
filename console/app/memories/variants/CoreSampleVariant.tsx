"use client";

/*
 * Archive variant C — "Core Sample". Mental model: geology. The corpus is
 * a drilled core: each quarter is a stratum, thickness = deposition volume,
 * superseded knowledge reads as compressed dark sediment under the living
 * layer. You read the org's history top-down like a geologist reads rock.
 * Strata expand in place; records open the shared inspector.
 */

import { useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import type { ArchiveData } from "../archive-data";
import MemoryInspector, { fmtDate, statusTone } from "../MemoryInspector";
import { useMemoryDetail } from "../useMemoryDetail";

const VIOLET = band("delta");

function quarterOf(iso: string | null): string {
  if (!iso) return "undated";
  const d = new Date(iso);
  return `${d.getUTCFullYear()} Q${Math.floor(d.getUTCMonth() / 3) + 1}`;
}

export default function CoreSampleVariant({ data }: { data: ArchiveData }) {
  const [openStratum, setOpenStratum] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading } = useMemoryDetail(selected, data.live);

  const strata = useMemo(() => {
    const byQ = new Map<string, typeof data.rows>();
    for (const r of data.rows) {
      const key = quarterOf(r.valid_from ?? r.created_at);
      byQ.set(key, [...(byQ.get(key) ?? []), r]);
    }
    return [...byQ.entries()]
      .sort((a, b) => b[0].localeCompare(a[0])) // newest on top, like a core
      .map(([label, rows]) => ({
        label,
        rows,
        living: rows.filter((r) => r.status === "canonical" || r.status === "candidate").length,
        compressed: rows.filter((r) => r.status === "deprecated").length,
      }));
  }, [data.rows]);

  const maxCount = Math.max(1, ...strata.map((s) => s.rows.length));

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: VIOLET }}>
            δ · archive · core sample
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            Drill the org&apos;s memory, layer by layer.
          </h1>
        </div>
        <div className={`${FONT_MONO} text-xs text-[#e9edff]/40`}>
          newest strata on top · dark bands = superseded sediment
          {!data.live && " · demo data"}
        </div>
      </div>

      <div className="mt-5 grid gap-6 lg:grid-cols-[1fr_0.9fr]">
        {/* the core */}
        <div className="overflow-hidden rounded-xl border border-white/10">
          {strata.map((s, i) => {
            const open = openStratum === s.label;
            const depth = 34 + (s.rows.length / maxCount) * 64;
            return (
              <div key={s.label} className={i > 0 ? "border-t border-white/[0.07]" : ""}>
                <button
                  onClick={() => setOpenStratum(open ? null : s.label)}
                  className="relative block w-full text-left transition hover:brightness-125"
                  style={{
                    height: open ? undefined : depth,
                    background: `linear-gradient(90deg,
                      hsla(262,85%,60%,${0.04 + (s.living / Math.max(1, s.rows.length)) * 0.1}),
                      rgba(10,9,15,0.6))`,
                  }}
                  aria-expanded={open}
                >
                  <div className="flex h-full items-center gap-4 px-5" style={{ minHeight: 34 }}>
                    <span className={`${FONT_DISPLAY} w-24 text-lg font-semibold text-white`}>{s.label}</span>
                    <span className={`${FONT_MONO} text-xs text-[#e9edff]/45`}>
                      {s.rows.length} deposited · {s.living} living
                      {s.compressed > 0 && <span className="text-[#e9edff]/30"> · {s.compressed} compressed</span>}
                    </span>
                    {/* sediment texture: one tick per record */}
                    <span className="ml-auto flex h-3 items-end gap-[2px]" aria-hidden>
                      {s.rows.slice(0, 40).map((r) => (
                        <span
                          key={r.id}
                          className="w-[3px] rounded-sm"
                          style={{
                            height: r.status === "deprecated" ? 5 : 12,
                            background: r.status === "deprecated" ? "rgba(233,237,255,0.18)" : band("delta", 68, 0.65),
                          }}
                        />
                      ))}
                    </span>
                  </div>
                </button>
                <AnimatePresence>
                  {open && (
                    <motion.div
                      initial={{ opacity: 0, height: 0 }}
                      animate={{ opacity: 1, height: "auto" }}
                      exit={{ opacity: 0, height: 0 }}
                      transition={{ duration: 0.25 }}
                      className="overflow-hidden bg-black/30"
                    >
                      <ul className="space-y-1 px-5 py-3">
                        {s.rows.map((r) => (
                          <li key={r.id}>
                            <button
                              onClick={() => setSelected(r.id === selected ? null : r.id)}
                              className={`${FONT_MONO} w-full truncate text-left text-sm transition ${
                                selected === r.id ? "text-[#c9b6ff]" : r.status === "deprecated" ? "text-[#e9edff]/35 hover:text-[#e9edff]/60" : "text-[#e9edff]/75 hover:text-white"
                              }`}
                              title={r.content}
                            >
                              <span className="mr-2 text-[10px] uppercase tracking-widest" style={{ color: statusTone(r.status) }}>
                                {r.kind}
                              </span>
                              {r.content}
                              <span className="ml-2 text-[10px] text-[#e9edff]/30">
                                {fmtDate(r.valid_from ?? r.created_at)} · {r.team}
                              </span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            );
          })}
        </div>

        {/* inspector */}
        <div className="lg:sticky lg:top-4 lg:self-start">
          <div className="min-h-[300px] rounded-xl border border-white/10 bg-white/[0.015] p-5">
            {!selected && (
              <p className={`${FONT_MONO} py-12 text-center text-sm text-[#e9edff]/35`}>
                crack a stratum open, pick a record — its full card reads here
              </p>
            )}
            {selected && loading && <p className={`${FONT_MONO} text-sm text-[#e9edff]/40`}>extracting the sample…</p>}
            {selected && detail && <MemoryInspector detail={detail} onHop={setSelected} />}
          </div>
        </div>
      </div>
    </div>
  );
}
