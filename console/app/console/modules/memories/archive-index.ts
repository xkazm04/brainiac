/*
 * The corpus, indexed once — the Archive's whole performance budget.
 *
 * Every keystroke re-tests every row, so that loop is what this file exists to
 * make cheap. Asking archive-data's validAt() there would construct two Date
 * objects per row per character (~1.3k allocations a stroke at org scale);
 * parsed to epoch ms up front it is two number compares. The predicate is
 * validAt unchanged — from <= at < to — with the open bounds carried as
 * infinities instead of nulls. Labels and validity spans are pre-rendered for
 * the same reason: they are constant per row, so formatting them per paint
 * would only move the cost downstream.
 *
 * Pure by policy: no React, no components, no theme. That is what lets
 * archive-index.test.ts assert the ranking and the title fallback directly.
 */

import type { MemoryRow } from "@/lib/types";

import { fmtDate } from "./archive-data";

/** Longest content excerpt rendered where a title should have been. */
const FALLBACK_CHARS = 96;

export interface Indexed {
  row: MemoryRow;
  /** Epoch ms; ±Infinity for an open bound. */
  from: number;
  to: number;
  /** What the Memory column shows: the title, or a content excerpt. */
  label: string;
  /** False when `label` is an excerpt standing in for a missing title. */
  titled: boolean;
  /** The content excerpt shown under a title (empty when the label IS content). */
  sub: string;
  /** Lowercased title + content + team + kind — one string, one includes(). */
  hay: string;
  span: string;
  /** Entity/service-shaped names lifted out of the content, for suggestions. */
  terms: string[];
}

const msOr = (iso: string | null | undefined, fallback: number): number => {
  if (!iso) return fallback;
  const t = new Date(iso).getTime();
  return Number.isFinite(t) ? t : fallback;
};

const squash = (s: string) => s.trim().replace(/\s+/g, " ");

const clip = (s: string, n: number) =>
  s.length > n ? `${s.slice(0, n - 1).trimEnd()}…` : s;

/**
 * The label for a row, and whether it is a real one.
 *
 * `memories.title` is nullable FOREVER: everything captured before migration
 * 0023 has none, and the extractor only started emitting one. So every reader
 * falls back to the content — and the caller is told which it got, because a
 * label and a claim should not look alike.
 */
export function memoryLabel(row: MemoryRow): { label: string; titled: boolean } {
  const t = row.title?.trim();
  if (t) return { label: t, titled: true };
  return { label: clip(squash(row.content), FALLBACK_CHARS), titled: false };
}

// Names worth suggesting: kebab services (psp-gateway), CamelCase products
// (ArgoCD), and shouted acronyms (PSP, DAG, MSK). Bare capitalised words are
// deliberately not matched — every sentence starts with one.
const SHAPES = [
  /[a-z][a-z0-9]*(?:-[a-z0-9]+)+/g,
  /\b[A-Z][a-z0-9]+(?:[A-Z][a-zA-Z0-9]+)+\b/g,
  /\b[A-Z]{2,}[a-z0-9]*\b/g,
];

const TERMS_PER_ROW = 8;

export function extractTerms(content: string): string[] {
  const out = new Set<string>();
  for (const re of SHAPES) {
    for (const m of content.matchAll(re)) {
      const t = m[0].toLowerCase();
      if (t.length >= 3 && t.length <= 40) out.add(t);
      if (out.size >= TERMS_PER_ROW) return [...out];
    }
  }
  return [...out];
}

export function indexRows(rows: MemoryRow[]): Indexed[] {
  return rows.map((row) => {
    const { label, titled } = memoryLabel(row);
    const content = squash(row.content);
    return {
      row,
      from: msOr(row.valid_from, Number.NEGATIVE_INFINITY),
      to: msOr(row.valid_to, Number.POSITIVE_INFINITY),
      label,
      titled,
      // Under a title, the claim itself. Under an excerpt, nothing — the label
      // is already the content, and echoing it would read as a rendering bug.
      sub: titled ? content : "",
      hay: `${row.title ?? ""} ${content} ${row.team} ${row.kind}`.toLowerCase(),
      span: `${fmtDate(row.valid_from)} → ${row.valid_to ? fmtDate(row.valid_to) : "now"}`,
      terms: extractTerms(row.content),
    };
  });
}

export const liveAt = (ix: Indexed, at: number) => ix.from <= at && at < ix.to;

// ── sorting ─────────────────────────────────────────────────────────────

export type SortDir = "off" | "asc" | "desc";

/**
 * Order by validity: when a claim started being true, then when it stopped.
 * Stable — ties keep the server's order (created_at DESC), because Array#sort
 * has been stable by spec since ES2019 and a comparator that returns 0 for a
 * tie therefore preserves input order.
 */
export function sortByValid(list: Indexed[], dir: SortDir): Indexed[] {
  if (dir === "off") return list;
  const sign = dir === "asc" ? 1 : -1;
  const cmp = (a: Indexed, b: Indexed) => {
    if (a.from !== b.from) return a.from < b.from ? -sign : sign;
    if (a.to !== b.to) return a.to < b.to ? -sign : sign;
    return 0;
  };
  return [...list].sort(cmp);
}

// ── suggestions ─────────────────────────────────────────────────────────

export type SuggestKind = "memory" | "term" | "team" | "kind" | "status";

export interface Suggestion {
  /** Stable across renders — the listbox option id and the React key. */
  id: string;
  kind: SuggestKind;
  /** The memory id, the search term, or the facet value to switch on. */
  value: string;
  label: string;
  /** What picking it DOES, spelled out before it happens. */
  action: string;
  /** How many memories it lands on, in the current scope. 0 = not counted. */
  count: number;
  /** Secondary line — a memory's team/kind, so two similar titles are telling apart. */
  detail: string;
}

/**
 * The current scope, tallied.
 *
 * Built from the as-of + facet-filtered corpus but NOT the query, so counts
 * answer "what would this suggestion get me, here" rather than "how big is the
 * archive" — the same cross-filtering property the facet rail's meters have.
 * Excluding the query is what keeps the tally memo off the keystroke path: it
 * only rebuilds when the scrubber or a facet moves.
 */
export interface Scope {
  rows: Indexed[];
  terms: Map<string, number>;
  team: Map<string, number>;
  kind: Map<string, number>;
  status: Map<string, number>;
}

const bump = (m: Map<string, number>, k: string) => m.set(k, (m.get(k) ?? 0) + 1);

export function buildScope(rows: Indexed[]): Scope {
  const scope: Scope = {
    rows,
    terms: new Map(),
    team: new Map(),
    kind: new Map(),
    status: new Map(),
  };
  for (const ix of rows) {
    for (const t of ix.terms) bump(scope.terms, t);
    bump(scope.team, ix.row.team);
    bump(scope.kind, ix.row.kind);
    bump(scope.status, ix.row.status);
  }
  return scope;
}

const MEMORY_SUGGESTIONS = 4;

/** Prefix beats substring; then the bigger hit; then alphabetical for stability. */
const rank = (q: string) => (a: [string, number], b: [string, number]) => {
  const pa = a[0].startsWith(q) ? 0 : 1;
  const pb = b[0].startsWith(q) ? 0 : 1;
  return pa - pb || b[1] - a[1] || a[0].localeCompare(b[0]);
};

const facetSuggestions = (
  scope: Scope,
  kind: "team" | "kind" | "status",
  q: string,
): Suggestion[] =>
  [...scope[kind].entries()]
    .filter(([v, n]) => n > 0 && v.toLowerCase().includes(q))
    .sort(rank(q))
    .map(([v, n]) => ({
      id: `${kind}:${v}`,
      kind,
      value: v,
      label: v,
      action: `filter ${kind}`,
      count: n,
      detail: "",
    }));

/**
 * What to offer for a partial query.
 *
 * Ordered by decisiveness, not by score: a facet is one click to a whole shelf,
 * a term narrows the text search to a name the corpus actually uses, and a
 * memory is the row you already knew existed. Typing alone already filters the
 * table, so nothing here duplicates plain search — every option does something
 * plain search cannot.
 */
export function suggest(scope: Scope, query: string, limit = 8): Suggestion[] {
  const q = query.trim().toLowerCase();
  if (q.length < 2) return [];

  const out: Suggestion[] = [
    ...facetSuggestions(scope, "team", q),
    ...facetSuggestions(scope, "kind", q),
    ...facetSuggestions(scope, "status", q),
  ];

  for (const [t, n] of [...scope.terms.entries()]
    .filter(([t]) => t.includes(q) && t !== q)
    .sort(rank(q))
    .slice(0, limit)) {
    out.push({
      id: `term:${t}`,
      kind: "term",
      value: t,
      label: t,
      action: "search for",
      count: n,
      detail: "",
    });
  }

  const hits = scope.rows
    .filter((ix) => ix.label.toLowerCase().includes(q))
    .sort((a, b) => {
      const pa = a.label.toLowerCase().startsWith(q) ? 0 : 1;
      const pb = b.label.toLowerCase().startsWith(q) ? 0 : 1;
      return pa - pb || a.label.length - b.label.length;
    })
    .slice(0, MEMORY_SUGGESTIONS);
  for (const ix of hits) {
    out.push({
      id: `memory:${ix.row.id}`,
      kind: "memory",
      value: ix.row.id,
      label: ix.label,
      action: "open record",
      count: 0,
      detail: `${ix.row.team} · ${ix.row.kind}`,
    });
  }

  return out.slice(0, limit);
}
