"use client";

/*
 * The standards board — the Library's rule shelf.
 *
 * Left: the tree the flat rules compile into (stack ▸ category ▸ rule), the
 * triage queue floated to the top of every branch it lives in. Right: the
 * selected rule in full (RuleDetail), with the gate's controls when the board
 * is live. Selection is client-local: the whole library (rules + details)
 * arrives with the page, so switching rules is instant — the same trade the
 * demo tour makes, which is what lets this one component serve both.
 */

import { useState } from "react";

import {
  band,
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  PANEL,
  withAlpha,
} from "@/design/theme";
import type { LibraryStandard, StandardDetail } from "@/lib/types";

import RuleDetail, { lifecycleTone } from "./RuleDetail";
import TriageControls from "./TriageControls";
import { buildStandardsTree, dormantOf, proposedOf, rankByPressure } from "./tree";

const THETA = band("theta");

function RuleLink({
  rule,
  active,
  onSelect,
  showPulse,
}: {
  rule: LibraryStandard;
  active: boolean;
  onSelect: (id: string) => void;
  /** In pressure order the number IS the ordering, so it is always shown;
   *  on the shelf it would be noise on every row. */
  showPulse?: boolean;
}) {
  const tone = lifecycleTone(rule.lifecycle);
  return (
    <button
      onClick={() => onSelect(rule.id)}
      className={`${FONT_MONO} flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-[12px] transition`}
      style={{
        color: active ? INK : INK_DIM,
        background: active ? withAlpha(THETA, 0.1) : "transparent",
      }}
      aria-current={active ? "true" : undefined}
    >
      <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: tone }} />
      <span className="min-w-0 flex-1 truncate">{rule.slug}</span>
      {rule.dormant && (
        <span
          className="shrink-0 text-[10px] uppercase tracking-[0.14em]"
          style={{ color: INK_FAINT }}
          title="adopted, and nobody has used it in the dormancy window"
        >
          dormant
        </span>
      )}
      {showPulse && rule.uses > 0 && (
        <span className="shrink-0 text-[10px] tabular-nums" style={{ color: THETA }}>
          {rule.uses}×
        </span>
      )}
      {rule.lifecycle === "proposed" && (
        <span className="shrink-0 text-[10px] uppercase tracking-[0.14em]" style={{ color: tone }}>
          triage
        </span>
      )}
    </button>
  );
}

type Order = "shelf" | "pressure";

function OrderToggle({ order, onChange }: { order: Order; onChange: (o: Order) => void }) {
  return (
    <div className="flex items-center gap-1" role="group" aria-label="Order">
      {(["shelf", "pressure"] as const).map((o) => (
        <button
          key={o}
          onClick={() => onChange(o)}
          aria-pressed={order === o}
          className={`${FONT_MONO} rounded-full px-2.5 py-1 text-[10px] uppercase tracking-[0.14em] transition`}
          style={{
            color: order === o ? INK : INK_FAINT,
            background: order === o ? withAlpha(THETA, 0.14) : "transparent",
          }}
        >
          {o === "shelf" ? "shelf" : "pressure"}
        </button>
      ))}
    </div>
  );
}

export default function StandardsBoard({
  standards,
  details,
  live,
}: {
  standards: LibraryStandard[];
  /** Detail per rule id — prefetched (live) or fixture (demo). */
  details: Record<string, StandardDetail>;
  /** Only a live board mounts the gate; a demo must never offer a
   *  working-looking adopt over fabricated rules. */
  live: boolean;
}) {
  const tree = buildStandardsTree(standards);
  const queue = proposedOf(standards);
  const ranked = rankByPressure(standards);
  const dormant = dormantOf(standards);
  const [order, setOrder] = useState<Order>("shelf");
  const [selectedId, setSelectedId] = useState<string | null>(
    queue[0]?.id ?? standards[0]?.id ?? null,
  );
  const detail = selectedId ? details[selectedId] : undefined;
  // A proposal the org is ALREADY following: the cheapest adoption on the
  // board, and invisible in every ordering that sorts by name.
  const followedAlready = ranked.filter((r) => r.lifecycle === "proposed" && r.uses > 0);

  return (
    <main className="mx-auto flex max-w-6xl flex-col gap-8 px-6 py-12">
      <header className="flex flex-col gap-3">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          the library · standards
        </span>
        <h1 className={`${FONT_DISPLAY} text-3xl`} style={{ color: INK }}>
          The org&rsquo;s ratified judgment, one rule at a time
        </h1>
        <p className="max-w-2xl text-[15px] leading-snug" style={{ color: INK_DIM }}>
          Every rule here is individually addressed, carries the evidence behind it, and shows
          whether practice actually follows it. Proposals wait at the gate until a named human
          adopts them — agents are only ever served what passed.
        </p>
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
          {queue.length > 0 && (
            <p className={`${FONT_MONO} text-[12px]`} style={{ color: lifecycleTone("proposed") }}>
              {queue.length} {queue.length === 1 ? "proposal" : "proposals"} waiting at the gate
            </p>
          )}
          {followedAlready.length > 0 && (
            <p className={`${FONT_MONO} text-[12px]`} style={{ color: THETA }}>
              {followedAlready.length}{" "}
              {followedAlready.length === 1 ? "is one the org" : "are ones the org"} already
              follows — order by pressure
            </p>
          )}
          {dormant.length > 0 && (
            <p className={`${FONT_MONO} text-[12px]`} style={{ color: INK_FAINT }}>
              {dormant.length} adopted {dormant.length === 1 ? "rule has" : "rules have"} gone
              dormant
            </p>
          )}
        </div>
      </header>

      {standards.length === 0 ? (
        <p
          className="rounded-xl p-6 text-[14px]"
          style={{ background: PANEL, border: `1px solid ${BORDER}`, color: INK_DIM }}
        >
          The library is empty. Rules arrive two ways: ratify a divergence from the drift board,
          or wait for the mining sweep (roadmap) to propose candidates from what the org already
          learned.
        </p>
      ) : (
        <div className="grid gap-6 lg:grid-cols-[260px_1fr]">
          {/* the tree rail */}
          <nav aria-label="Standards" className="flex flex-col gap-4">
            <OrderToggle order={order} onChange={setOrder} />
            {order === "pressure" ? (
              <div className="flex flex-col gap-0.5">
                <span
                  className={`${FONT_MONO} text-[10px] uppercase tracking-[0.16em]`}
                  style={{ color: INK_FAINT }}
                >
                  by the pressure it is under
                </span>
                {ranked.map((r) => (
                  <RuleLink
                    key={r.id}
                    rule={r}
                    active={r.id === selectedId}
                    onSelect={setSelectedId}
                    showPulse
                  />
                ))}
              </div>
            ) : (
              tree.map((s) => (
              <div key={s.stack} className="flex flex-col gap-1.5">
                <div className="flex items-baseline justify-between">
                  <span className={LABEL} style={{ color: THETA }}>
                    {s.stack}
                  </span>
                  <span className={`${FONT_MONO} text-[10px]`} style={{ color: INK_FAINT }}>
                    {s.count}
                    {s.proposed > 0 ? ` · ${s.proposed} at the gate` : ""}
                  </span>
                </div>
                {s.categories.map((c) => (
                  <div key={c.category} className="flex flex-col gap-0.5 pl-1">
                    <span className={`${FONT_MONO} text-[10px] uppercase tracking-[0.16em]`} style={{ color: INK_FAINT }}>
                      {c.category}
                    </span>
                    {c.rules.map((r) => (
                      <RuleLink key={r.id} rule={r} active={r.id === selectedId} onSelect={setSelectedId} />
                    ))}
                  </div>
                ))}
              </div>
              ))
            )}
          </nav>

          {/* the selected rule */}
          {detail ? (
            <RuleDetail detail={detail} gate={live ? <TriageControls detail={detail} /> : undefined} />
          ) : (
            <p
              className="rounded-xl p-6 text-[14px]"
              style={{ background: PANEL, border: `1px solid ${BORDER}`, color: INK_DIM }}
            >
              Select a rule from the shelf.
            </p>
          )}
        </div>
      )}
    </main>
  );
}
