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

/** The triage queue: every proposed rule, across all stacks, oldest slug-first. */
export function proposedOf(rules: LibraryStandard[]): LibraryStandard[] {
  return rules
    .filter((r) => r.lifecycle === "proposed")
    .sort((a, b) => a.slug.localeCompare(b.slug));
}
