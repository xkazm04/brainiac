/*
 * Substrate for the Pages wiki (KB-PLAN KB2), server-driven edition.
 *
 * WHAT CHANGED, and why it is not a redesign. The wiki used to fetch the WHOLE
 * visible corpus in one trip and build the tree, the tab counts, the search and
 * the pagination in the browser over that array. At the bank corpus that is 436
 * summaries crossing the wire so the client can throw most of them away — an
 * O(corpus) cost that grows by one row per page the org writes. Now the server
 * pages, facets and groups (GET /v1/docs?facets=1&space=…&needs_review=1&q=…),
 * and this module only shapes the URL into a query and the envelope into props.
 * The client never holds the corpus: the rail is `facets.spaces` (the server's
 * space directory, cross-filtered so it never shrinks below all spaces) and the
 * pane is one page of `documents`.
 *
 * Client-safe by construction — it must NOT import the server-only API client,
 * so the demo mirror (offline parity) lives here too, and it is the executable
 * spec for what the server contract does.
 */

import type { DocFacet, DocSummary } from "@/lib/types";

import { spaceKey } from "./tree";

/** The console's page size. The server clamps 1..200; this is the legible list
 *  length the wiki pages through. */
export const WIKI_PAGE_SIZE = 25;

/** The three views over the corpus. `all` is the front door; `review` and
 *  `dirty` are the two boolean queues, each carrying its count. */
export type WikiTab = "all" | "review" | "dirty";

/** The narrowing a reader applied — mirrors the server query params. The URL is
 *  the single source of truth, so a filtered view is shareable and survives a
 *  refresh. `space === undefined` means no space is open (the directory); an
 *  EMPTY string is the real un-namespaced space, not "no space". */
export interface WikiFilter {
  space?: string;
  tab: WikiTab;
  q?: string;
}

export interface WikiData {
  live: boolean;
  /** The current PAGE — filtered, ordered and windowed by the server. */
  documents: DocSummary[];
  /** Filtered depth for the current space+tab+q — what matches, ignoring the
   *  page window. Drives "showing N of M" and the pager. */
  total: number;
  /** The space directory, from the server's cross-filtered facet menu. The rail
   *  and the front-door cards render from THIS, never from scanning documents. */
  spaces: DocFacet[];
  /** The tab counts over the whole wiki, from the facet menu — stable as the
   *  reader opens a space or searches, the same as the old client totals. */
  tabCounts: { all: number; review: number; dirty: number };
  /** The active filter, echoed back so the wiki can render/clear it. */
  filter: WikiFilter;
  /** Zero-based page index. */
  page: number;
  pageSize: number;
}

type Params = Record<string, string | string[] | undefined>;

const one = (v: string | string[] | undefined): string | undefined =>
  Array.isArray(v) ? v[0] : v;

/**
 * Parse the URL into the wiki's filter + page. Everything is clamped HERE, once,
 * then handed to both the live fetch and the demo mirror.
 *
 * `space` is present-vs-absent, not truthy-vs-falsy: `?space=` selects the
 * un-namespaced bucket (a real, browsable space), which is NOT the same as no
 * `space` key at all (the directory).
 */
export function parseWiki(params: Params): { filter: WikiFilter; page: number } {
  const tabRaw = one(params.tab);
  const tab: WikiTab = tabRaw === "review" || tabRaw === "dirty" ? tabRaw : "all";
  const space = "space" in params && params.space !== undefined ? (one(params.space) ?? "") : undefined;
  const q = one(params.q)?.trim() || undefined;
  const page = Math.max(0, Math.floor(Number(one(params.page)) || 0));
  return { filter: { space, tab, q }, page };
}

/**
 * Build a full WikiData page from a fixture — the offline/demo mirror, and the
 * unit-tested statement of what the server contract must do: facet the WHOLE
 * corpus (so the directory and tab counts never shrink), then filter, then
 * window. `page` clamps so an out-of-range page lands on the last real one.
 */
export function demoWiki(
  all: DocSummary[],
  filter: WikiFilter,
  page: number,
  pageSize: number,
): WikiData {
  // Space directory + tab counts over the whole corpus — the cross-filtered
  // facet menu, reproduced. The space facet ignores its own filter, so opening
  // a space never drops it (or any sibling) from the rail.
  const bySpace = new Map<string, number>();
  for (const d of all) bySpace.set(spaceKey(d.slug), (bySpace.get(spaceKey(d.slug)) ?? 0) + 1);
  const spaces: DocFacet[] = [...bySpace.entries()]
    .map(([value, count]) => ({ value, label: value, count }))
    .sort((a, b) => b.count - a.count || a.value.localeCompare(b.value));

  const tabCounts = {
    all: all.length,
    review: all.filter((d) => d.pending_review).length,
    dirty: all.filter((d) => d.dirty).length,
  };

  let matched = all;
  if (filter.space !== undefined) matched = matched.filter((d) => spaceKey(d.slug) === filter.space);
  if (filter.tab === "review") matched = matched.filter((d) => d.pending_review);
  else if (filter.tab === "dirty") matched = matched.filter((d) => d.dirty);
  if (filter.q) {
    const q = filter.q.toLowerCase();
    matched = matched.filter((d) => `${d.title} ${d.slug}`.toLowerCase().includes(q));
  }

  const total = matched.length;
  const pages = Math.max(1, Math.ceil(total / pageSize));
  const safePage = Math.min(Math.max(0, page), pages - 1);
  const start = safePage * pageSize;
  return {
    live: false,
    documents: matched.slice(start, start + pageSize),
    total,
    spaces,
    tabCounts,
    filter,
    page: safePage,
    pageSize,
  };
}
