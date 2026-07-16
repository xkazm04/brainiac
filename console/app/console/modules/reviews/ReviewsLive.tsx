"use client";

/*
 * The operator's review surface, wired to the real decision path.
 *
 * This file exists for one structural reason: ReviewWorklist is a client
 * component (it owns keyboard triage and a focus cursor), and a server
 * component cannot hand a render prop to a client one. So the decision buttons
 * are imported HERE rather than passed down from page.tsx.
 *
 * That is safe in this direction and only this direction: /console is gated, and
 * nothing under /demo imports this file — which is what keeps the server actions
 * out of the public bundle entirely (see app/demo/DemoReviews.tsx for the other
 * half of that boundary).
 */

import ReviewWorklist from "./ReviewWorklist";
import { ContradictionButtons, PromotionButtons } from "./review-buttons";
import { CONTRA_HEADING_ID } from "./review-surface";
import type {
  ContradictionQueueItem,
  ContradictionStatus,
  PromotionQueueItem,
} from "@/lib/governance-api";

export default function ReviewsLive({
  promotions,
  promotionsTotal,
  promotionsOffset,
  contradictions,
  contradictionsTotal,
  counts,
  cstatus,
}: {
  promotions: PromotionQueueItem[];
  promotionsTotal: number;
  promotionsOffset: number;
  contradictions: ContradictionQueueItem[];
  contradictionsTotal: number;
  counts: { status: string; count: number }[];
  cstatus: ContradictionStatus;
}) {
  return (
    <ReviewWorklist
      promotions={promotions}
      promotionsTotal={promotionsTotal}
      promotionsOffset={promotionsOffset}
      contradictions={contradictions}
      contradictionsTotal={contradictionsTotal}
      counts={counts}
      cstatus={cstatus}
      // Paging is a server round trip for the same reason the status tabs are:
      // the page is the window the server was asked for, so it lives in the URL.
      promotionsPageHref={(o) =>
        `/console?m=reviews&cstatus=${cstatus}${o > 0 ? `&poffset=${o}` : ""}`
      }
      promotionControls={(p) => <PromotionButtons promotionId={p.id} />}
      contradictionControls={(c) => (
        <ContradictionButtons
          contradictionId={c.id}
          memoryAId={c.memory_a.id}
          memoryBId={c.memory_b.id}
        />
      )}
      // Filtering is a server round trip: the status is a query param the page
      // re-queries on, so the tabs are links rather than callbacks. The fragment
      // must match the heading ReviewWorklist actually renders (CONTRA_HEADING_ID)
      // — it pointed at a `#contradictions-h` that no longer exists, so every tab
      // click round-tripped and then scrolled nowhere.
      statusHref={(s) => `/console?m=reviews&cstatus=${s}#${CONTRA_HEADING_ID}`}
      // No onBulk: there is no bulk endpoint, and looping N single approvals
      // behind one button is not a decision this surface has earned. The rail
      // says so rather than offering a control that cannot fire.
    />
  );
}
