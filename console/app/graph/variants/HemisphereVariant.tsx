"use client";

/*
 * Cortex Map variant A — "Hemisphere". Mental model: anatomy. Three team
 * lobes at the poles of a hemisphere; canonical hubs sit at the weighted
 * centroid of the teams that speak them — pure single-team knowledge hugs
 * its lobe, γ-bound knowledge migrates to the center. Click a hub to open
 * its neighborhood in the side panel; hop neighbors without leaving the
 * map. Never renders raw entities on the map — that's the panel's job.
 */

import { useMemo, useState } from "react";
import { motion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL } from "@/design/theme";

import { hash01, teamColor, type CortexData } from "../cortex-data";
import { useCanonicalDetail } from "../useCanonicalDetail";
import { AnchoredMemories, EvidenceEdges, NeighborHops, SurfaceForms } from "../DetailSections";

const GOLD = band("gamma");
const W = 760;
const H = 640;
const CX = W / 2;
const CY = H / 2 + 10;
const R = 250;

export default function HemisphereVariant({ data }: { data: CortexData }) {
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading } = useCanonicalDetail(selected, data);
  const { overview } = data;

  const teamIndex = (teamId: string) => overview.teams.findIndex((t) => t.id === teamId);

  const poles = useMemo(
    () =>
      overview.teams.map((t, i) => {
        const a = (-90 + i * 120) * (Math.PI / 180);
        return { ...t, x: CX + Math.cos(a) * R, y: CY + Math.sin(a) * R, i };
      }),
    [overview.teams],
  );

  const hubs = useMemo(
    () =>
      overview.canonicals.map((c) => {
        const members = c.team_ids
          .map((tid) => poles.find((p) => p.id === tid))
          .filter(Boolean) as typeof poles;
        const mx = members.reduce((s, p) => s + p.x, 0) / Math.max(1, members.length);
        const my = members.reduce((s, p) => s + p.y, 0) / Math.max(1, members.length);
        // pull toward center with team count; deterministic jitter breaks overlaps
        const pull = c.teams === 3 ? 0.85 : c.teams === 2 ? 0.55 : 0.28;
        const jx = (hash01(c.name) - 0.5) * 90;
        const jy = (hash01(c.name, 7) - 0.5) * 90;
        return {
          ...c,
          x: CX * pull + mx * (1 - pull) + jx * (1 - pull),
          y: CY * pull + my * (1 - pull) + jy * (1 - pull),
          r: 6 + Math.sqrt(c.memories) * 2.6,
        };
      }),
    [overview.canonicals, poles],
  );

  const maxShared = Math.max(1, ...overview.team_links.map((l) => l.shared));

  return (
    <div className="mx-auto max-w-7xl px-6 py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: GOLD }}>
            γ · cortex map · {selected ? "neighborhood" : "hemisphere"}
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            {selected
              ? (detail?.canonical.name ?? "…")
              : "Where the org's knowledge lives."}
          </h1>
        </div>
        <div className={`${FONT_MONO} text-xs text-[#e9edff]/40`}>
          {overview.canonicals.length} canonical hubs · gold = binds all teams
          {!data.live && " · demo data"}
        </div>
      </div>

      <div className="mt-4 grid gap-6 lg:grid-cols-[1.2fr_0.8fr]">
        {/* the map */}
        <div className="relative overflow-hidden rounded-xl border border-white/10 bg-white/[0.015]">
          <svg viewBox={`0 0 ${W} ${H}`} className="h-auto w-full" role="img" aria-label="Hemisphere map of canonical knowledge across teams">
            {/* hemisphere outline */}
            <circle cx={CX} cy={CY} r={R + 62} fill="none" stroke="rgba(233,237,255,0.07)" strokeDasharray="4 6" />
            {/* binding arcs between lobes */}
            {overview.team_links.map((l) => {
              const a = poles.find((p) => p.id === l.a);
              const b = poles.find((p) => p.id === l.b);
              if (!a || !b) return null;
              return (
                <path
                  key={`${l.a}${l.b}`}
                  d={`M${a.x} ${a.y} Q ${CX} ${CY} ${b.x} ${b.y}`}
                  fill="none"
                  stroke={GOLD}
                  strokeOpacity={0.1 + (l.shared / maxShared) * 0.25}
                  strokeWidth={1 + (l.shared / maxShared) * 3}
                />
              );
            })}
            {/* team lobes */}
            {poles.map((p) => (
              <g key={p.id}>
                <circle cx={p.x} cy={p.y} r={26 + Math.sqrt(p.memories) * 2} fill={teamColor(p.i, 60, 0.07)} stroke={teamColor(p.i, 68, 0.5)} strokeWidth="1.2" />
                <text x={p.x} y={p.y - 34 - Math.sqrt(p.memories) * 2} textAnchor="middle" fontSize="12" fill={teamColor(p.i, 78)} style={{ textTransform: "uppercase", letterSpacing: "0.18em" }}>
                  {p.name}
                </text>
                <text x={p.x} y={p.y + 4} textAnchor="middle" fontSize="11" fill="rgba(233,237,255,0.5)">
                  {p.memories}
                </text>
              </g>
            ))}
            {/* canonical hubs */}
            {hubs.map((c) => {
              const isSel = selected === c.id;
              const tone = c.teams === 3 ? GOLD : c.teams === 2 ? band("gamma", 68, 0.65) : "rgba(233,237,255,0.35)";
              return (
                <g key={c.id} onClick={() => setSelected(isSel ? null : c.id)} className="cursor-pointer">
                  {isSel && <circle cx={c.x} cy={c.y} r={c.r + 8} fill="none" stroke={GOLD} strokeWidth="1.4" strokeDasharray="3 4" />}
                  <motion.circle
                    cx={c.x}
                    cy={c.y}
                    r={c.r}
                    fill={c.teams === 3 ? "hsla(46,90%,60%,0.16)" : "rgba(233,237,255,0.04)"}
                    stroke={tone}
                    strokeWidth={isSel ? 2 : 1.2}
                    initial={{ scale: 0, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    transition={{ duration: 0.4, delay: hash01(c.id) * 0.4 }}
                    style={{ transformOrigin: `${c.x}px ${c.y}px` }}
                  />
                  <text x={c.x} y={c.y - c.r - 6} textAnchor="middle" fontSize="11" fill={isSel ? GOLD : "rgba(233,237,255,0.65)"}>
                    {c.name}
                  </text>
                </g>
              );
            })}
          </svg>
          <div className={`${LABEL} absolute bottom-3 left-4`} style={{ color: "rgba(233,237,255,0.3)" }}>
            click a hub · size = anchored memories · position = team pull
          </div>
        </div>

        {/* the panel */}
        <div className="min-h-[420px] rounded-xl border border-white/10 bg-white/[0.015] p-5">
          {!selected && (
            <div className="flex h-full flex-col justify-center text-center">
              <div className={`${FONT_DISPLAY} text-xl text-white/70`}>Select a canonical hub.</div>
              <p className={`${FONT_MONO} mx-auto mt-2 max-w-xs text-sm leading-relaxed text-[#e9edff]/40`}>
                The map never shows raw entities — each hub opens its neighborhood here:
                dialects, evidence edges, and the memories you're cleared to read.
              </p>
            </div>
          )}
          {selected && loading && (
            <div className={`${FONT_MONO} text-sm text-[#e9edff]/40`}>resolving neighborhood…</div>
          )}
          {selected && detail && (
            <div className="space-y-5">
              <div className="flex items-baseline justify-between">
                <span className={LABEL} style={{ color: GOLD }}>
                  {detail.canonical.kind} · canonical
                </span>
                <button onClick={() => setSelected(null)} className={`${FONT_MONO} text-xs text-[#e9edff]/40 hover:text-white`}>
                  ✕ back to hemisphere
                </button>
              </div>
              <SurfaceForms detail={detail} teamIndex={teamIndex} />
              <EvidenceEdges detail={detail} />
              <NeighborHops detail={detail} onHop={setSelected} />
              <AnchoredMemories detail={detail} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
