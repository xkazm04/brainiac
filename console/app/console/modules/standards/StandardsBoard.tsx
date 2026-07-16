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
import { buildStandardsTree, proposedOf } from "./tree";

const THETA = band("theta");

function RuleLink({
  rule,
  active,
  onSelect,
}: {
  rule: LibraryStandard;
  active: boolean;
  onSelect: (id: string) => void;
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
      {rule.lifecycle === "proposed" && (
        <span className="text-[10px] uppercase tracking-[0.14em]" style={{ color: tone }}>
          triage
        </span>
      )}
    </button>
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
  const [selectedId, setSelectedId] = useState<string | null>(
    queue[0]?.id ?? standards[0]?.id ?? null,
  );
  const detail = selectedId ? details[selectedId] : undefined;

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
        {queue.length > 0 && (
          <p className={`${FONT_MONO} text-[12px]`} style={{ color: lifecycleTone("proposed") }}>
            {queue.length} {queue.length === 1 ? "proposal" : "proposals"} waiting at the gate
          </p>
        )}
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
            {tree.map((s) => (
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
            ))}
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
