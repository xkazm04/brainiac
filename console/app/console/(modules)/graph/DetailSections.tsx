"use client";

/*
 * Shared L2/L3 content blocks for the Cortex Map variants (hoisted per the
 * /prototype skill — every variant frames these differently but renders the
 * same facts): surface forms per team, evidence edges, anchored memories,
 * neighbor hops.
 */

import type { CanonicalDetail } from "@/lib/types";
import { band, FONT_MONO, LABEL } from "@/design/theme";

import { teamColor } from "./cortex-data";

const GOLD = band("gamma");

export function SurfaceForms({
  detail,
  teamIndex,
}: {
  detail: CanonicalDetail;
  teamIndex: (teamId: string) => number;
}) {
  return (
    <div>
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
        known as · {detail.surface_forms.length} dialects
      </div>
      <div className="mt-2 flex flex-wrap gap-2">
        {detail.surface_forms.map((f) => (
          <span
            key={f.entity_id}
            className={`${FONT_MONO} rounded-full border px-3 py-1 text-sm`}
            style={{
              borderColor: teamColor(teamIndex(f.team_id), 68, 0.4),
              color: teamColor(teamIndex(f.team_id), 78),
            }}
            title={`${f.method ?? "linked"} · confidence ${f.confidence ?? "—"}`}
          >
            {f.team}: “{f.name}”
          </span>
        ))}
      </div>
    </div>
  );
}

export function EvidenceEdges({ detail }: { detail: CanonicalDetail }) {
  const shown = detail.edges.slice(0, 5);
  return (
    <div>
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
        relationships · {detail.edges.length} evidence edges
      </div>
      <ul className="mt-2 space-y-1.5">
        {shown.map((e, i) => (
          <li key={i} className={`${FONT_MONO} text-sm text-[#e9edff]/75`}>
            <span className="text-white">{e.src_name}</span>{" "}
            <em style={{ color: GOLD }}>{e.relation}</em>{" "}
            <span className="text-white">{e.dst_name}</span>
            {e.evidence && (
              <div className="mt-0.5 truncate pl-3 text-xs text-[#e9edff]/40" title={e.evidence}>
                ▸ {e.evidence}
              </div>
            )}
          </li>
        ))}
        {detail.edges.length === 0 && (
          <li className={`${FONT_MONO} text-sm text-[#e9edff]/35`}>no edges recorded yet</li>
        )}
      </ul>
    </div>
  );
}

export function AnchoredMemories({ detail }: { detail: CanonicalDetail }) {
  return (
    <div>
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
        evidence memories · your clearance
      </div>
      <ul className="mt-2 space-y-2">
        {detail.memories.slice(0, 4).map((m) => (
          <li key={m.id} className="rounded-lg border border-white/10 bg-white/[0.02] p-3">
            <div className={LABEL} style={{ color: GOLD }}>
              {m.kind} · {m.team} · {m.status}
            </div>
            <p className={`${FONT_MONO} mt-1 text-sm leading-snug text-[#e9edff]/80`}>{m.content}</p>
          </li>
        ))}
        {detail.memories.length === 0 && (
          <li className={`${FONT_MONO} text-sm text-[#e9edff]/35`}>
            nothing you're cleared to read anchors here
          </li>
        )}
      </ul>
    </div>
  );
}

export function NeighborHops({
  detail,
  onHop,
}: {
  detail: CanonicalDetail;
  onHop: (id: string) => void;
}) {
  if (detail.neighbors.length === 0) return null;
  return (
    <div>
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
        one hop away
      </div>
      <div className="mt-2 flex flex-wrap gap-2">
        {detail.neighbors.map((n) => (
          <button
            key={n.id}
            onClick={() => onHop(n.id)}
            className={`${FONT_MONO} rounded-full border border-white/15 px-3 py-1 text-sm text-[#e9edff]/70 transition hover:border-[#f3c74f]/60 hover:text-[#f3c74f]`}
          >
            {n.name} <span className="text-[#e9edff]/35">×{n.shared_edges}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
