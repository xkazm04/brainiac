"use client";

/*
 * Shared L2 content for the Archive variants (hoisted per the /prototype
 * skill): the full record of one memory — validity, supersession lineage,
 * provenance, anchored entities, promotion history. Variants frame it
 * differently; the facts are identical.
 */

import type { MemoryDetail } from "@/lib/types";
import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

const VIOLET = band("delta");

// Nullable-and-optional: the generated API types mark Option<T> fields as
// optional (utoipa's default), though the server always emits them as null.
export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toISOString().slice(0, 10);
}

export function statusTone(status: string): string {
  if (status === "canonical") return band("gamma");
  if (status === "deprecated") return "rgba(233,237,255,0.35)";
  if (status === "rejected") return MAGENTA;
  return VIOLET;
}

export default function MemoryInspector({
  detail,
  onHop,
}: {
  detail: MemoryDetail;
  onHop: (id: string) => void;
}) {
  const m = detail.memory;
  const chain = [
    ...detail.chain.predecessors,
    { id: m.id, content: m.content, status: m.status, valid_from: m.valid_from, valid_to: m.valid_to, depth: 0 },
    ...detail.chain.successors,
  ].sort((a, b) => a.depth - b.depth);

  return (
    <div className="space-y-5">
      {/* header facts */}
      <div className={`${FONT_MONO} flex flex-wrap items-center gap-2 text-[11px] uppercase tracking-[0.16em]`}>
        <span style={{ color: statusTone(m.status) }}>{m.status}</span>
        <span className="text-[#e9edff]/30">·</span>
        <span style={{ color: VIOLET }}>{m.kind}</span>
        <span className="text-[#e9edff]/30">·</span>
        <span className="text-[#e9edff]/55">{m.team} / {m.visibility}</span>
        {m.confidence != null && (
          <>
            <span className="text-[#e9edff]/30">·</span>
            <span className="text-[#e9edff]/55">conf {m.confidence.toFixed(2)}</span>
          </>
        )}
      </div>
      <p className={`${FONT_DISPLAY} text-xl leading-snug text-white`}>{m.content}</p>
      <div className={`${FONT_MONO} text-xs text-[#e9edff]/45`}>
        valid {fmtDate(m.valid_from)} → {m.valid_to ? fmtDate(m.valid_to) : "now"} · recorded {fmtDate(m.created_at)}
      </div>

      {/* lineage */}
      {chain.length > 1 && (
        <div>
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
            lineage · what this superseded / what superseded it
          </div>
          <ol className="mt-2 space-y-0">
            {chain.map((c) => {
              const current = c.depth === 0;
              return (
                <li key={c.id} className="relative pl-5">
                  <span
                    className="absolute left-0 top-2.5 h-2 w-2 rounded-full"
                    style={{ background: current ? VIOLET : "rgba(233,237,255,0.25)" }}
                  />
                  {chain[chain.length - 1] !== c && (
                    <span className="absolute left-[3.5px] top-5 h-full w-px bg-[#e9edff]/15" />
                  )}
                  <button
                    disabled={current}
                    onClick={() => onHop(c.id)}
                    className={`${FONT_MONO} pb-3 text-left text-sm leading-snug ${
                      current ? "text-white" : "text-[#e9edff]/50 transition hover:text-[#c9b6ff]"
                    }`}
                  >
                    {c.content}
                    <span className="ml-2 text-[10px] uppercase tracking-widest" style={{ color: statusTone(c.status) }}>
                      {c.status} · {fmtDate(c.valid_from)}–{c.valid_to ? fmtDate(c.valid_to) : "now"}
                    </span>
                  </button>
                </li>
              );
            })}
          </ol>
        </div>
      )}

      {/* provenance */}
      <div>
        <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
          provenance
        </div>
        {detail.provenance ? (
          <p className={`${FONT_MONO} mt-1.5 text-sm text-[#e9edff]/70`}>
            {detail.provenance.actor_kind}:{detail.provenance.actor_id}
            {detail.provenance.model_ref && <> · {detail.provenance.model_ref}</>}
            {detail.provenance.source_kind && (
              <> · from {detail.provenance.source_kind}{detail.provenance.source_ref ? ` (${detail.provenance.source_ref})` : ""}</>
            )}
          </p>
        ) : (
          <p className={`${FONT_MONO} mt-1.5 text-sm text-[#e9edff]/35`}>seeded directly — no session provenance</p>
        )}
      </div>

      {/* entities + promotions */}
      <div className="grid gap-5 sm:grid-cols-2">
        <div>
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
            anchored entities
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {detail.entities.map((e) => (
              <span key={`${e.team}-${e.name}`} className={`${FONT_MONO} rounded-full border border-white/12 px-2.5 py-0.5 text-xs text-[#e9edff]/70`}>
                {e.name} <span className="text-[#e9edff]/35">· {e.team}</span>
              </span>
            ))}
            {detail.entities.length === 0 && (
              <span className={`${FONT_MONO} text-xs text-[#e9edff]/35`}>none</span>
            )}
          </div>
        </div>
        <div>
          <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
            promotion history
          </div>
          <ul className={`${FONT_MONO} mt-2 space-y-1 text-xs text-[#e9edff]/60`}>
            {detail.promotions.map((p, i) => (
              <li key={i}>
                {fmtDate(p.created_at)} · {p.from_status} → <span style={{ color: statusTone(p.to_status) }}>{p.to_status}</span>{" "}
                <span className="text-[#e9edff]/35">({p.policy_decision}{p.policy_rule ? ` · ${p.policy_rule}` : ""})</span>
              </li>
            ))}
            {detail.promotions.length === 0 && <li className="text-[#e9edff]/35">no ledger entries yet</li>}
          </ul>
        </div>
      </div>
    </div>
  );
}
