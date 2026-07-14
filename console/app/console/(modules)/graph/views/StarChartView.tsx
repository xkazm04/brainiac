"use client";

/*
 * Cortex Map — "Star Chart" view (relationship lens). Mental model: astronomy. Canonical
 * entities are stars; magnitude = anchored memories; γ-bound knowledge
 * burns gold. Constellation lines are drawn only for the SELECTED star —
 * to its one-hop neighbors — so the sky never becomes a hairball. Hovering
 * a team in the legend lights up its stars. Search-to-focus teleports.
 */

import { useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import { hash01, teamColor, type CortexData } from "../cortex-data";
import { useCanonicalDetail } from "../useCanonicalDetail";
import { AnchoredMemories, EvidenceEdges, NeighborHops, SurfaceForms } from "../DetailSections";

const GOLD = band("gamma");
const W = 1000;
const H = 560;

export default function StarChartView({ data }: { data: CortexData }) {
  const [selected, setSelected] = useState<string | null>(null);
  const [hoverTeam, setHoverTeam] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const { detail, loading } = useCanonicalDetail(selected, data);
  const { overview } = data;

  const teamIndex = (teamId: string) => overview.teams.findIndex((t) => t.id === teamId);

  const stars = useMemo(
    () =>
      overview.canonicals.map((c) => ({
        ...c,
        x: 60 + hash01(c.name, 3) * (W - 120),
        y: 50 + hash01(c.name, 11) * (H - 130),
        r: 2.5 + Math.sqrt(c.memories) * 1.9,
      })),
    [overview.canonicals],
  );

  const starById = useMemo(() => new Map(stars.map((s) => [s.id, s])), [stars]);
  const selectedStar = selected ? starById.get(selected) : null;
  const match = query.trim().toLowerCase();
  const matched = match ? stars.filter((s) => s.name.toLowerCase().includes(match)) : [];

  // Density mode: at large-org scale only bound/loud stars keep permanent
  // labels — the rest identify on hover (title) / selection / search.
  const dense = stars.length > 24;
  const maxTeams = Math.max(1, ...stars.map((s) => s.teams));
  const labelCutoff = useMemo(() => {
    if (!dense) return 0;
    const sorted = [...stars].sort((a, b) => b.memories - a.memories);
    return sorted[Math.min(17, sorted.length - 1)]?.memories ?? 0;
  }, [stars, dense]);

  const dimmed = (s: (typeof stars)[number]) => {
    if (hoverTeam && !s.team_ids.includes(hoverTeam)) return true;
    if (match && !s.name.toLowerCase().includes(match)) return true;
    return false;
  };

  return (
    <div className="mx-auto max-w-7xl px-6 py-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: GOLD }}>
            γ · cortex map · star chart
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            The night sky of what the org knows.
          </h1>
        </div>
        <input
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            const hit = e.target.value.trim()
              ? stars.find((s) => s.name.toLowerCase().includes(e.target.value.trim().toLowerCase()))
              : null;
            if (hit) setSelected(hit.id);
          }}
          placeholder="search the sky…"
          className={`${FONT_MONO} w-56 rounded-full border border-white/15 bg-white/[0.03] px-4 py-2 text-sm text-white placeholder:text-[#e9edff]/30 focus:border-[#f3c74f]/60 focus:outline-none`}
          aria-label="Search canonical entities"
        />
      </div>

      <div className="relative mt-4 overflow-hidden rounded-xl border border-white/10 bg-white/[0.015]">
        <svg viewBox={`0 0 ${W} ${H}`} className="h-auto w-full" role="img" aria-label="Star chart of canonical knowledge">
          {/* graticule */}
          {[0.25, 0.5, 0.75].map((f) => (
            <line key={f} x1={0} y1={H * f} x2={W} y2={H * f} stroke="rgba(233,237,255,0.04)" />
          ))}
          {/* constellation lines: only for the selected star */}
          {selectedStar &&
            detail?.neighbors.map((n) => {
              const t = starById.get(n.id);
              if (!t) return null;
              return (
                <motion.line
                  key={n.id}
                  x1={selectedStar.x}
                  y1={selectedStar.y}
                  x2={t.x}
                  y2={t.y}
                  stroke={GOLD}
                  strokeOpacity={0.35}
                  strokeWidth={1}
                  strokeDasharray="2 4"
                  initial={{ pathLength: 0 }}
                  animate={{ pathLength: 1 }}
                  transition={{ duration: 0.5 }}
                />
              );
            })}
          {/* stars */}
          {stars.map((s) => {
            const isSel = selected === s.id;
            const dim = dimmed(s);
            const bound = s.teams >= Math.max(3, maxTeams);
            const tone = bound ? GOLD : s.teams >= 2 ? band("gamma", 74, 0.8) : "rgba(233,237,255,0.55)";
            const showLabel =
              !dense || isSel || bound || s.memories >= labelCutoff || (!!match && !dim);
            return (
              <g key={s.id} onClick={() => setSelected(isSel ? null : s.id)} className="cursor-pointer" opacity={dim ? 0.18 : 1}>
                <title>{`${s.name} · ${s.memories} memories · ${s.teams} team${s.teams > 1 ? "s" : ""}`}</title>
                {bound && !dim && (
                  <circle cx={s.x} cy={s.y} r={s.r + 5} fill="none" stroke={GOLD} strokeOpacity="0.25" />
                )}
                {isSel && <circle cx={s.x} cy={s.y} r={s.r + 9} fill="none" stroke={GOLD} strokeWidth="1.2" strokeDasharray="3 4" />}
                <motion.circle
                  cx={s.x}
                  cy={s.y}
                  r={s.r}
                  fill={tone}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: dim ? 0.2 : 1 }}
                  transition={{ duration: 0.3, delay: hash01(s.id, 5) * 0.5 }}
                />
                {showLabel && (
                  <text x={s.x} y={s.y - s.r - 5} textAnchor="middle" fontSize={dense ? 10 : 10.5} fill={isSel ? GOLD : "rgba(233,237,255,0.55)"}>
                    {s.name}
                  </text>
                )}
              </g>
            );
          })}
        </svg>

        {/* legend */}
        <div className="absolute bottom-3 left-4 flex items-center gap-4">
          {overview.teams.map((t, i) => (
            <button
              key={t.id}
              onMouseEnter={() => setHoverTeam(t.id)}
              onMouseLeave={() => setHoverTeam(null)}
              className={`${LABEL} transition`}
              style={{ color: hoverTeam === t.id ? teamColor(i, 78) : teamColor(i, 68, 0.55) }}
            >
              ● {t.name}
            </button>
          ))}
          <span className={LABEL} style={{ color: "rgba(233,237,255,0.3)" }}>
            hover a team to light its stars{dense && " · faint stars name themselves on hover"}
            {!data.live && " · demo data"}
          </span>
        </div>
        {match && matched.length === 0 && (
          <div className={`${FONT_MONO} absolute right-4 top-3 text-xs`} style={{ color: "rgba(255,93,162,0.8)" }}>
            nothing by that name in this sky
          </div>
        )}
      </div>

      {/* focus card */}
      <AnimatePresence>
        {selected && (
          <motion.div
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 10 }}
            transition={{ duration: 0.25 }}
            className="mt-4 rounded-xl border p-5"
            style={{ borderColor: "hsla(46,90%,68%,0.3)", background: "hsla(46,90%,60%,0.04)" }}
          >
            {loading && <div className={`${FONT_MONO} text-sm text-[#e9edff]/40`}>focusing…</div>}
            {detail && (
              <div className="grid gap-6 lg:grid-cols-3">
                <div className="space-y-4">
                  <div className="flex items-baseline justify-between">
                    <h2 className={`${FONT_DISPLAY} text-2xl font-semibold text-white`}>
                      {detail.canonical.name}
                      <span className={`${FONT_MONO} ml-2 text-xs text-[#e9edff]/40`}>{detail.canonical.kind}</span>
                    </h2>
                    <button onClick={() => setSelected(null)} className={`${FONT_MONO} text-xs text-[#e9edff]/40 hover:text-white`}>
                      ✕
                    </button>
                  </div>
                  <SurfaceForms detail={detail} teamIndex={teamIndex} />
                  <NeighborHops detail={detail} onHop={setSelected} />
                </div>
                <EvidenceEdges detail={detail} />
                <AnchoredMemories detail={detail} />
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
