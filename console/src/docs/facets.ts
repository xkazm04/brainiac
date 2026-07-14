/*
 * Boundary narrowing for the document layer's enum-shaped strings.
 *
 * The Rust enums serialize as plain strings, so the generated types give the
 * console `string` for `lifecycle` and `policy_decision`. The UI keeps its
 * closed unions (a `Record<MemoryLifecycle, …>` is how the reader guarantees
 * every lifecycle HAS a colour and a caption) — so every incoming string is
 * narrowed here, once, at the edge.
 *
 * The defaults are chosen for safety, not convenience:
 *
 *  - An unknown **lifecycle** degrades to `shipped`. That mirrors the pipeline's
 *    own facet firewall (KB0), which coerces rather than drops.
 *  - An unknown **policy** degrades to `needs_review`, never `auto_published`.
 *    A future enum value must never make this UI claim a revision published
 *    itself when a human is in fact still owed a decision.
 */

import type { MemoryLifecycle, RevisionPolicy } from "@/lib/types";

const LIFECYCLES: readonly MemoryLifecycle[] = ["shipped", "in_flight", "proposed"];
const POLICIES: readonly RevisionPolicy[] = ["auto_published", "needs_review", "rejected"];

export function asLifecycle(s: string): MemoryLifecycle {
  return (LIFECYCLES as readonly string[]).includes(s) ? (s as MemoryLifecycle) : "shipped";
}

export function asPolicy(s: string): RevisionPolicy {
  return (POLICIES as readonly string[]).includes(s) ? (s as RevisionPolicy) : "needs_review";
}
