/*
 * The contract the review surface renders against.
 *
 * Two callers, one component (ReviewWorklist): the operator's console passes
 * live queues and the real server actions; the public tour passes fixtures and
 * inert stamps. Everything that differs between them is a prop, which is what
 * keeps the tour honest — a visitor is looking at the operator's surface, not a
 * lookalike built beside it.
 */

import type { ReactNode } from "react";

import type {
  ContradictionQueueItem,
  ContradictionStatus,
  PromotionQueueItem,
} from "@/lib/governance-api";

export interface ReviewSurfaceProps {
  promotions: PromotionQueueItem[];
  contradictions: ContradictionQueueItem[];
  counts: { status: string; count: number }[];
  cstatus: ContradictionStatus;
  /** Live console: filtering is a server round trip, so tabs are links. */
  statusHref?: (s: ContradictionStatus) => string;
  /** Scale mode: the corpus is already in the page, so tabs are buttons. */
  onStatusChange?: (s: ContradictionStatus) => void;
  /** The per-item decision controls — real buttons, or an inert stamp. */
  promotionControls: (p: PromotionQueueItem) => ReactNode;
  contradictionControls: (c: ContradictionQueueItem) => ReactNode;
  /**
   * Sign a whole selection at once. Absent when the surface cannot write.
   * A queue of 480 is unworkable one card at a time — but bulk approval is also
   * how a review gate degrades into a rubber stamp, so a variant that offers it
   * must make what is being signed legible first. The analytics module already
   * tracks `rubber_stamp_rate`; this is the surface that would move it.
   */
  onBulk?: (ids: string[], action: "approve" | "reject") => void;
}

/**
 * The contradiction filter's tabs, and the allow-list the route validates
 * `?cstatus=` against. Shared rather than private to the rail because the server
 * page has to reject a junk status before it queries with it.
 */
export const STATUS_TABS: { key: ContradictionStatus; label: string }[] = [
  { key: "open", label: "open" },
  { key: "resolved_supersede", label: "superseded" },
  { key: "resolved_coexist", label: "coexist" },
  { key: "dismissed", label: "dismissed" },
  { key: "all", label: "all" },
];
