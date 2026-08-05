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

import Button from "@/components/ui/Button";
import { band, FOCUS, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import { teamColor, type CortexData } from "../cortex-data";
import { useCanonicalDetail } from "../useCanonicalDetail";
import { AnchoredMemories, EvidenceEdges, NeighborHops, SurfaceForms } from "../DetailSections";

const GOLD = band("gamma");

export default function DepthOfFieldView({ data }: { data: CortexData }) {
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading, error } = useCanonicalDetail(selected, data);
  const { overview } = data;
  const teamIndex = (teamId: string) => overview.teams.findIndex((t) => t.id === teamId);
  const focused = selected != null;
  const selectedName = overview.canonicals.find((c) => c.id === selected)?.name;

  return (
    <div className="mx-auto max-w-6xl px-6 py-6">
      {/* focal stack breadcrumb */}
      <div className={`${FONT_MONO} flex items-center gap-2 text-xs`}>
        {/* `quiet` — a breadcrumb crumb, which must not read as a button beside
            the trail it sits in. sm matches the strip's own text-xs, and the
            variant's hover:text-white is the hover this already had. */}
        <Button
          variant="quiet"
          size="sm"
          onClick={() => setSelected(null)}
          style={{ color: undefined }}
          className={focused ? "text-[var(--ink)]/40 hover:text-white" : "text-white"}
        >
          org meridian
        </Button>
        {focused && (
          <>
            <span className="text-[var(--ink)]/25">▸</span>
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
            <div className={`${FONT_MONO} mt-0.5 text-sm text-[var(--ink)]/60`}>
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
            {/* These stay plain <button>s. A card is a BLOCK — a title row, a
                count, a team ribbon stacked down it — and Button's `row`
                variant is `flex`, which would lay those three out side by side.
                What they take from the primitives is the piece that was
                genuinely missing: FOCUS, so tabbing through the grid is visible. */}
            {overview.canonicals.map((c) => (
              <button
                key={c.id}
                onClick={() => setSelected(c.id)}
                className={`${FOCUS} group rounded-lg border p-4 text-left transition`}
                style={{
                  borderColor: c.teams === 3 ? "hsla(46,90%,68%,0.4)" : "rgba(233,237,255,0.1)",
                  background: c.teams === 3 ? "hsla(46,90%,60%,0.05)" : "rgba(255,255,255,0.015)",
                }}
              >
                <div className="flex items-baseline justify-between">
                  <span className={`${FONT_DISPLAY} text-lg font-semibold text-white group-hover:text-[var(--gold)]`}>
                    {c.name}
                  </span>
                  <span className={`${FONT_MONO} text-xs text-[var(--ink)]/35`}>{c.kind}</span>
                </div>
                <div className={`${FONT_MONO} mt-2 text-xs text-[var(--ink)]/50`}>
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
              <div className="rounded-xl border bg-[var(--surface-solid)]/95 p-6 shadow-2xl backdrop-blur" style={{ borderColor: "hsla(46,90%,68%,0.35)" }}>
                {loading && <div className={`${FONT_MONO} text-sm text-[var(--ink)]/40`}>pulling into focus…</div>}
                {error && <div className={`${FONT_MONO} text-sm text-[var(--amber)]`}>{error}</div>}
                {detail && (
                  <div className="space-y-6">
                    <div className="flex items-baseline justify-between">
                      <h2 className={`${FONT_DISPLAY} text-3xl font-semibold tracking-tight text-white`}>
                        {detail.canonical.name}
                        <span className={`${FONT_MONO} ml-3 text-sm text-[var(--ink)]/40`}>{detail.canonical.kind} · canonical</span>
                      </h2>
                      {/* `pill` — the bordered capsule this already was. px-4
                          beats the variant's px-3.5 on stylesheet order so it
                          can stay a class, but text-xs would LOSE to sm's
                          text-sm, so the type size rides in `style`. The white
                          edge pair stays in classes, hence the cleared
                          borderColor/color. */}
                      <Button
                        variant="pill"
                        size="sm"
                        onClick={() => setSelected(null)}
                        style={{ borderColor: undefined, color: undefined, fontSize: "0.75rem" }}
                        className="border-white/15 px-4 text-[var(--ink)]/60 hover:border-white/40 hover:text-white"
                      >
                        esc · defocus
                      </Button>
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
