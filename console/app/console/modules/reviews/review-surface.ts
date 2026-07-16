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
 * The id of the contradictions heading, and the fragment the status tabs scroll
 * to. Shared rather than written twice: the tabs are LINKS (the filter is a
 * server round trip), so a fragment that does not match the rendered heading is
 * a link that round-trips and then scrolls nowhere — which is what
 * `#contradictions-h` had quietly become against a heading named `wl-contra-h`.
 * One constant, imported by both sides, cannot drift.
 */
export const CONTRA_HEADING_ID = "wl-contra-h";

/** Anything the rail can put a cursor on. */
export interface FocusableRow {
  id: string;
}

/**
 * Resolve the rail's focus cursor to an index in the CURRENT list.
 *
 * The cursor is an id, not an index, and that distinction is a safety property
 * rather than a preference. The rail re-renders whenever the queue is
 * revalidated, and a decided row leaving the list shifts every row after it up
 * one. Under an index cursor the pane would then silently re-point at the
 * neighbouring claim while the operator believed they were still looking at the
 * one they had read — and `a`/`r` would sign it. Under an id cursor the pane
 * follows the claim, wherever the refresh moved it to.
 *
 * When the focused id is gone (decided here, or by someone else) the cursor
 * goes to the FRONT of the rail rather than to whatever now sits at the old
 * index. That is deliberate: any "nearby" fallback is exactly the silent
 * re-point this cursor exists to prevent, and the rail is oldest-first, so the
 * front is the honest place to resume. The common case never reaches this —
 * a decision advances the cursor to the next id explicitly, before the row
 * disappears.
 *
 * @returns the index of the focused row, or -1 when there is nothing to focus.
 */
export function resolveFocusIndex(
  rows: readonly FocusableRow[],
  focusId: string | null,
): number {
  if (rows.length === 0) return -1;
  if (focusId === null) return 0;
  const i = rows.findIndex((r) => r.id === focusId);
  return i === -1 ? 0 : i;
}

/**
 * The id the cursor should land on after `delta` steps from `focusId`.
 * Clamped at both ends — walking off the rail parks at its edge.
 */
export function stepFocus(
  rows: readonly FocusableRow[],
  focusId: string | null,
  delta: number,
): string | null {
  const i = resolveFocusIndex(rows, focusId);
  if (i === -1) return null;
  const next = Math.min(Math.max(i + delta, 0), rows.length - 1);
  return rows[next].id;
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
