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
  /** ONE PAGE of the promotion backlog, oldest first — not the backlog. */
  promotions: PromotionQueueItem[];
  /**
   * Every promotion awaiting review. Required, and deliberately not defaulted to
   * `promotions.length`: that default is the bug. The rail renders a page (the
   * server caps it at 200) and the two numbers diverge silently the moment a
   * real org's backlog passes it.
   */
  promotionsTotal: number;
  /** Where this page starts in the backlog. */
  promotionsOffset: number;
  /** Link to another page of the promotion backlog. Absent ⇒ no pager. */
  promotionsPageHref?: (offset: number) => string;
  /** ONE PAGE of the contradiction queue under `cstatus`. */
  contradictions: ContradictionQueueItem[];
  /** Contradictions matching `cstatus`, ignoring the page window. */
  contradictionsTotal: number;
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
   * Sign a whole selection at once. Absent when the surface cannot write — the
   * public tour passes nothing here, which is what keeps it structurally inert.
   *
   * A queue of 480 is unworkable one card at a time. But bulk approval is also
   * how a review gate degrades into a rubber stamp, so the rail pairs it with a
   * confirmation step for any batch of more than one, and the analytics module's
   * `rubber_stamp_rate` is the number that would catch it if that is not enough.
   *
   * Returns a per-ROW outcome rather than a single ok/failed, because a mixed
   * batch is the normal case: the server authorizes every item separately
   * against its own team, so a selection can legitimately come back part
   * approved, part 403, part 409. Answering "some of them" with "no" is what a
   * single boolean would do.
   */
  onBulk?: BulkHandler;
}

export type BulkHandler = (
  ids: string[],
  action: "approve" | "reject",
) => Promise<BulkOutcome>;

/** What became of one id in a batch. */
export interface BulkRowOutcome {
  id: string;
  ok: boolean;
  message: string;
}

export interface BulkOutcome {
  /** True only when EVERY row landed. */
  ok: boolean;
  message: string;
  decided: number;
  failed: number;
  rows: BulkRowOutcome[];
}

/**
 * Whether a batch has to be confirmed before it is signed.
 *
 * One item does not: the pane is showing that exact claim, so `a` is a decision
 * about something the operator is looking at — that is keyboard triage, and
 * making it ask twice would remove the only reason it exists. More than one
 * item does, because nothing on screen shows all of them at once, and "approve
 * 200" is precisely the click this gate exists to slow down.
 */
export const needsConfirm = (count: number): boolean => count > 1;

/**
 * The id of the contradictions heading, and the fragment the status tabs scroll
 * to. Shared rather than written twice: the tabs are LINKS (the filter is a
 * server round trip), so a fragment that does not match the rendered heading is
 * a link that round-trips and then scrolls nowhere — which is what
 * `#contradictions-h` had quietly become against a heading named `wl-contra-h`.
 * One constant, imported by both sides, cannot drift.
 */
export const CONTRA_HEADING_ID = "wl-contra-h";

/** Where a rendered page sits inside the backlog behind it. */
export interface PageScope {
  /** True when this page IS the whole backlog — nothing is being hidden. */
  whole: boolean;
  /** 1-based first row on this page (0 when the page is empty). */
  from: number;
  /** 1-based last row on this page (0 when the page is empty). */
  to: number;
  /** The whole backlog. */
  total: number;
}

export function pageScope(pageLength: number, offset: number, total: number): PageScope {
  const empty = pageLength === 0;
  return {
    whole: offset === 0 && pageLength >= total,
    from: empty ? 0 : offset + 1,
    to: empty ? 0 : offset + pageLength,
    total,
  };
}

/**
 * The sentence that keeps the rail's filters honest, or null when the page is
 * the whole backlog and there is nothing to disclaim.
 *
 * The rail's facet chips, its stale tally and its "select all" are all computed
 * over the rows CURRENTLY IN THE PAGE, because that is the only corpus the
 * client has. That is a fine trade — 200 rows is a working set, and the counts
 * are still computed against every other active filter, so a chip means "what
 * this would leave me with" rather than "what exists". What is NOT fine is
 * letting it read as the whole org: a team filter that silently searches 200 of
 * 5000 rows reports zero pending for a team whose work sits on page two, and
 * looks exactly like good news. So when a page is a window, it says so.
 */
export function scopeNote(s: PageScope): string | null {
  if (s.whole) return null;
  return `Filters, counts and select-all below cover rows ${s.from}–${s.to} of ${s.total} — this page, not the whole backlog.`;
}

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
