/*
 * Per-row rendering helpers for the Archive.
 *
 * Search, facets, cross-filtering, sorting and paging all moved to the SERVER
 * (GET /v1/memories) — the browser holds one page, never the corpus, so the
 * in-memory aggregation this file used to carry (indexRows, buildScope,
 * sortByValid, suggest…) is gone. What remains is the constant-per-row drawing:
 * the Memory column's label (title, or a content excerpt when there is none)
 * and the validity span. Both are cheap and pure.
 *
 * Pure by policy: no React, no components, no theme — so archive-index.test.ts
 * can assert the label fallback and the span directly.
 */

import type { MemoryRow } from "@/lib/types";

import { fmtDate } from "./archive-data";

/** Longest content excerpt rendered where a title should have been. */
const FALLBACK_CHARS = 96;

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

/** The validity window as one string: "2026-01-30 → now" for an open end. */
export function spanLabel(row: Pick<MemoryRow, "valid_from" | "valid_to">): string {
  return `${fmtDate(row.valid_from)} → ${row.valid_to ? fmtDate(row.valid_to) : "now"}`;
}

/** Everything the Memory + Valid columns draw for one row, computed once. */
export interface RowView {
  label: string;
  /** False when `label` is an excerpt standing in for a missing title. */
  titled: boolean;
  /** The content excerpt shown under a title (empty when the label IS content). */
  sub: string;
  span: string;
}

export function rowView(row: MemoryRow): RowView {
  const { label, titled } = memoryLabel(row);
  // Under a title, the claim itself rides underneath. Under an excerpt, nothing
  // — the label is already the content, and echoing it would read as a bug.
  return { label, titled, sub: titled ? squash(row.content) : "", span: spanLabel(row) };
}
