"use client";

/*
 * Cortex Map — "Depth of Field" view (catalog lens). Mental model: focal planes.
 * Levels are physical depth: the org summary strip (L0) sits above a grid
 * of canonical cards (L1); focusing one pushes the grid back — blurred,
 * receded — while the neighborhood (L2/L3) comes forward in focus. The
 * breadcrumb is literally the focal stack. No map metaphor at all: this is
 * the librarian's version, dense and legible.
 */

import { useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import { teamColor, type CortexData } from "../cortex-data";
import { useCanonicalDetail } from "../useCanonicalDetail";
import { AnchoredMemories, EvidenceEdges, NeighborHops, SurfaceForms } from "../DetailSections";

const GOLD = band("gamma");

export default function DepthOfFieldView({ data }: { data: CortexData }) {
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading } = useCanonicalDetail(selected, data);
  const { overview } = data;
  const teamIndex = (teamId: string) => overview.teams.findIndex((t) => t.id === teamId);
  const focused = selected != null;
  const selectedName = overview.canonicals.find((c) => c.id === selected)?.name;

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      {/* focal stack breadcrumb */}
      <div className={`${FONT_MONO} flex items-center gap-2 text-xs`}>
        <button
          onClick={() => setSelected(null)}
          className={focused ? "text-[#e9edff]/40 hover:text-white" : "text-white"}
        >
          org meridian
        </button>
        {focused && (
          <>
            <span className="text-[#e9edff]/25">▸</span>
            <span style={{ color: GOLD }}>{selectedName ?? "…"}</span>
          </>
        )}
        <span className={`${LABEL} ml-auto`} style={{ color: "rgba(233,237,255,0.3)" }}>
          γ · cortex map · depth of field{!data.live && " · demo data"}
        </span>
      </div>

      {/* L0 — the org strip (always sharp) */}
      <div className="mt-3 grid grid-cols-3 gap-3">
        {overview.teams.map((t, i) => (
          <div key={t.id} className="rounded-lg border border-white/10 bg-white/[0.02] px-4 py-3">
            <div className={LABEL} style={{ color: teamColor(i, 74) }}>
              {t.name}
            </div>
            <div className={`${FONT_MONO} mt-0.5 text-sm text-[#e9edff]/60`}>
              {t.memories} memories · {t.entities} entities
            </div>
          </div>
        ))}
      </div>

      <div className="relative mt-4">
        {/* L1 — canonical grid; recedes when a card is focused */}
        <motion.div
          animate={
            focused
              ? { scale: 0.965, opacity: 0.35, filter: "blur(3px)" }
              : { scale: 1, opacity: 1, filter: "blur(0px)" }
          }
          transition={{ duration: 0.35 }}
          className={focused ? "pointer-events-none" : ""}
          aria-hidden={focused}
        >
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            {overview.canonicals.map((c) => (
              <button
                key={c.id}
                onClick={() => setSelected(c.id)}
                className="group rounded-lg border p-4 text-left transition"
                style={{
                  borderColor: c.teams === 3 ? "hsla(46,90%,68%,0.4)" : "rgba(233,237,255,0.1)",
                  background: c.teams === 3 ? "hsla(46,90%,60%,0.05)" : "rgba(255,255,255,0.015)",
                }}
              >
                <div className="flex items-baseline justify-between">
                  <span className={`${FONT_DISPLAY} text-lg font-semibold text-white group-hover:text-[#f3c74f]`}>
                    {c.name}
                  </span>
                  <span className={`${FONT_MONO} text-xs text-[#e9edff]/35`}>{c.kind}</span>
                </div>
                <div className={`${FONT_MONO} mt-2 text-xs text-[#e9edff]/50`}>
                  {c.memories} memories
                </div>
                <div className="mt-2 flex gap-1.5">
                  {c.team_ids.map((tid) => {
                    const i = teamIndex(tid);
                    return (
                      <span
                        key={tid}
                        className="h-1.5 flex-1 rounded-full"
                        style={{ background: teamColor(i, 68, 0.7) }}
                        title={overview.teams[i]?.name}
                      />
                    );
                  })}
                </div>
              </button>
            ))}
          </div>
        </motion.div>

        {/* L2/L3 — the focused plane */}
        <AnimatePresence>
          {focused && (
            <motion.div
              initial={{ opacity: 0, scale: 1.04, y: 8 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 1.03 }}
              transition={{ duration: 0.3 }}
              className="absolute inset-x-0 top-0 z-10"
            >
              <div className="rounded-xl border bg-[#0a090f]/95 p-6 shadow-2xl backdrop-blur" style={{ borderColor: "hsla(46,90%,68%,0.35)" }}>
                {loading && <div className={`${FONT_MONO} text-sm text-[#e9edff]/40`}>pulling into focus…</div>}
                {detail && (
                  <div className="space-y-6">
                    <div className="flex items-baseline justify-between">
                      <h2 className={`${FONT_DISPLAY} text-3xl font-semibold tracking-tight text-white`}>
                        {detail.canonical.name}
                        <span className={`${FONT_MONO} ml-3 text-sm text-[#e9edff]/40`}>{detail.canonical.kind} · canonical</span>
                      </h2>
                      <button
                        onClick={() => setSelected(null)}
                        className={`${FONT_MONO} rounded-full border border-white/15 px-4 py-1.5 text-xs text-[#e9edff]/60 transition hover:border-white/40 hover:text-white`}
                      >
                        esc · defocus
                      </button>
                    </div>
                    <div className="grid gap-6 lg:grid-cols-2">
                      <div className="space-y-5">
                        <SurfaceForms detail={detail} teamIndex={teamIndex} />
                        <EvidenceEdges detail={detail} />
                        <NeighborHops detail={detail} onHop={setSelected} />
                      </div>
                      <AnchoredMemories detail={detail} />
                    </div>
                  </div>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
