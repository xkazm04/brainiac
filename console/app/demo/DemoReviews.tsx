"use client";

/*
 * The review queue on the fixture org — the operator’s ReviewWorklist, rendered
 * with fixtures instead of live queues and with the decision controls replaced
 * by an inert stamp.
 *
 * This file is the entire difference between the public tour's reviews module
 * and the operator's. It used to be a 200-line lookalike (ReviewGate) that had
 * already drifted: no status filter, no counts, no empty states, different
 * headline. Now the drift has nowhere to live — a visitor sees the triage rail
 * an operator actually works, keyboard legend and all.
 *
 * The console filters contradictions on the server (a query param it re-queries
 * on). There is no server here — the fixture is already in the page — so the
 * same filter runs over it in client state, and the tab bar behaves identically.
 */

import { useMemo, useState } from "react";

import ReviewWorklist from "../console/modules/reviews/ReviewWorklist";

import { FONT_MONO } from "@/design/theme";
import type { ContradictionQueueItem, ContradictionStatus, PromotionQueueItem } from "@/lib/governance-api";

/**
 * The read-only twin of the operator's decision controls.
 *
 * Not disabled real buttons — there is no action wired behind these at all, and
 * the module that owns the real ones is never imported into this bundle. The
 * public showcase must be structurally incapable of mutating anything, not
 * merely discouraged from it.
 */
function InertControls({ labels }: { labels: string[] }) {
  return (
    <span
      className={`${FONT_MONO} flex flex-wrap items-center gap-2 text-[11px]`}
      style={{ color: "rgba(233,237,255,0.35)" }}
    >
      {labels.map((l) => (
        <span
          key={l}
          className="rounded-full border px-3 py-1"
          style={{ borderColor: "rgba(233,237,255,0.16)", color: "rgba(233,237,255,0.45)" }}
        >
          {l}
        </span>
      ))}
      <span className="ml-1">read-only in the demo</span>
    </span>
  );
}

export default function DemoReviews({
  promotions,
  contradictions,
  counts,
}: {
  promotions: PromotionQueueItem[];
  contradictions: ContradictionQueueItem[];
  counts: { status: string; count: number }[];
}) {
  const [cstatus, setCstatus] = useState<ContradictionStatus>("open");

  const shown = useMemo(
    () => (cstatus === "all" ? contradictions : contradictions.filter((c) => c.status === cstatus)),
    [contradictions, cstatus],
  );

  return (
    <ReviewWorklist
      promotions={promotions}
      contradictions={shown}
      counts={counts}
      cstatus={cstatus}
      onStatusChange={setCstatus}
      promotionControls={() => <InertControls labels={["approve", "reject"]} />}
      contradictionControls={() => (
        <InertControls labels={["A wins", "B wins", "coexist", "dismiss"]} />
      )}
    />
  );
}
