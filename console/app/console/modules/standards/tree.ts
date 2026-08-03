/*
 * The standards tree — pure. The API serves rules flat (a rule's identity is
 * stack → category → slug); the board renders them as a two-level tree with
 * counts. Pure and tested because the grouping IS the navigation: a rule
 * filed under the wrong branch is a rule nobody finds.
 */

import type { LibraryStandard } from "@/lib/types";

export interface CategoryNode {
  category: string;
  rules: LibraryStandard[];
}

export interface StackNode {
  stack: string;
  /** Total rules under the stack, all lifecycles. */
  count: number;
  /** Rules awaiting the gate — what the triage badge shows. */
  proposed: number;
  categories: CategoryNode[];
}

/** Lifecycle sort weight: what a maintainer acts on first. Rejected sinks to
 *  the bottom — kept visible (it is the dedup memory), never in the way. */
const LIFECYCLE_WEIGHT: Record<string, number> = {
  proposed: 0,
  adopted: 1,
  deprecated: 2,
  rejected: 3,
};

/**
 * Group flat rules into stack ▸ category ▸ rule. Stacks and categories sort
 * alphabetically (predictable from the name, like the nav); rules sort
 * proposed → adopted → deprecated, then by slug — the queue floats to the top
 * of every branch it lives in.
 */
export function buildStandardsTree(rules: LibraryStandard[]): StackNode[] {
  const byStack = new Map<string, Map<string, LibraryStandard[]>>();
  for (const r of rules) {
    const cats = byStack.get(r.stack) ?? new Map<string, LibraryStandard[]>();
    const list = cats.get(r.category) ?? [];
    list.push(r);
    cats.set(r.category, list);
    byStack.set(r.stack, cats);
  }

  return [...byStack.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([stack, cats]) => {
      const categories = [...cats.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([category, list]) => ({
          category,
          rules: [...list].sort(
            (a, b) =>
              (LIFECYCLE_WEIGHT[a.lifecycle] ?? 9) - (LIFECYCLE_WEIGHT[b.lifecycle] ?? 9) ||
              a.slug.localeCompare(b.slug),
          ),
        }));
      const all = categories.flatMap((c) => c.rules);
      return {
        stack,
        count: all.length,
        proposed: all.filter((r) => r.lifecycle === "proposed").length,
        categories,
      };
    });
}

/**
 * ORDER BY THE PRESSURE A RULE IS ACTUALLY UNDER — the board's second reading.
 *
 * The shelf (`buildStandardsTree`) answers "where does this rule live"; it
 * cannot answer "which of these matters most this week". The library has been
 * recording per-team usage since LB1, and the two questions that signal
 * answers are the two a maintainer actually has:
 *
 *   - Which PROPOSALS is the org already following? A candidate with a pulse
 *     is a rule people reach for before anyone ratified it — the cheapest
 *     adoption on the board, because the behaviour change is already done.
 *   - Which ADOPTED rules has nobody touched? Those sink to the bottom with
 *     the dormancy flag on them, which is the honest place for a rule the org
 *     stopped following.
 *
 * Ties break toward the actionable: a proposal outranks an adopted rule at the
 * same pulse, because one of them still needs a decision. Slug last, so the
 * order is total and the list never reshuffles between renders.
 */
export function rankByPressure(rules: LibraryStandard[]): LibraryStandard[] {
  return [...rules].sort(
    (a, b) =>
      b.uses - a.uses ||
      (LIFECYCLE_WEIGHT[a.lifecycle] ?? 9) - (LIFECYCLE_WEIGHT[b.lifecycle] ?? 9) ||
      a.slug.localeCompare(b.slug),
  );
}

/** Adopted rules the org has stopped following — the retirement shortlist. */
export function dormantOf(rules: LibraryStandard[]): LibraryStandard[] {
  return rules.filter((r) => r.dormant);
}

/** The triage queue: every proposed rule, across all stacks, oldest slug-first. */
export function proposedOf(rules: LibraryStandard[]): LibraryStandard[] {
  return rules
    .filter((r) => r.lifecycle === "proposed")
    .sort((a, b) => a.slug.localeCompare(b.slug));
}
