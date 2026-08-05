"use client";

/*
 * Shared L2/L3 content blocks for the Cortex Map variants (hoisted per the
 * /prototype skill — every variant frames these differently but renders the
 * same facts): surface forms per team, evidence edges, anchored memories,
 * neighbor hops.
 */

import type { CSSProperties } from "react";

import Button from "@/components/ui/Button";
import type { CanonicalDetail } from "@/lib/types";
import { band, FONT_MONO, LABEL, withAlpha } from "@/design/theme";

import { teamColor } from "./cortex-data";

const GOLD = band("gamma");

/*
 * The neighbor hop's hover gold.
 *
 * It was two raw `#f3c74f` literals — a hand-mixed twin of the gamma band that
 * had drifted a shade off the real token, so the hop chips lit a different gold
 * than everything else on the map. It cannot ride in `style` (it only applies on
 * :hover, and an inline colour would win in BOTH states, killing the hover
 * outright), so it rides as a custom property instead — the same trick Archive
 * uses for --vio — and GOLD is now the single source for both.
 */
const HOP_VARS = { "--hop": GOLD, "--hop-edge": withAlpha(GOLD, 0.6) } as CSSProperties;

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
          <li key={i} className={`${FONT_MONO} text-sm text-[var(--ink)]/75`}>
            <span className="text-white">{e.src_name}</span>{" "}
            <em style={{ color: GOLD }}>{e.relation}</em>{" "}
            <span className="text-white">{e.dst_name}</span>
            {e.evidence && (
              <div className="mt-0.5 truncate pl-3 text-xs text-[var(--ink)]/40" title={e.evidence}>
                ▸ {e.evidence}
              </div>
            )}
          </li>
        ))}
        {detail.edges.length === 0 && (
          <li className={`${FONT_MONO} text-sm text-[var(--ink)]/35`}>no edges recorded yet</li>
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
            <p className={`${FONT_MONO} mt-1 text-sm leading-snug text-[var(--ink)]/80`}>{m.content}</p>
          </li>
        ))}
        {detail.memories.length === 0 && (
          <li className={`${FONT_MONO} text-sm text-[var(--ink)]/35`}>
            {"nothing you're cleared to read anchors here"}
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
          /* `pill` — the bordered capsule this already was. Its sm bucket is a
             touch roomier than the original (px-3.5/py-1.5 vs px-3/py-1), so the
             gutters ride in `style`, which is the only deterministic override:
             two padding utilities resolve by stylesheet order, not className
             order. borderColor/color are cleared for the same reason the hover
             gold is a variable — an inline colour would outrank both hovers. */
          <Button
            key={n.id}
            variant="pill"
            size="sm"
            onClick={() => onHop(n.id)}
            style={{
              ...HOP_VARS,
              borderColor: undefined,
              color: undefined,
              paddingLeft: "0.75rem",
              paddingRight: "0.75rem",
              paddingTop: "0.25rem",
              paddingBottom: "0.25rem",
            }}
            className="border-white/15 text-[var(--ink)]/70 hover:border-[var(--hop-edge)] hover:text-[var(--hop)]"
          >
            {n.name} <span className="text-[var(--ink)]/35">×{n.shared_edges}</span>
          </Button>
        ))}
      </div>
    </div>
  );
}
