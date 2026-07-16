// Pure substrate for the Audit ledger: kind/outcome labels, paging/filter
// parsing, the honest actor copy, and the demo fixture.
//
// Client-safe by construction — no server-only import — even though the
// module itself renders fully on the server (see Module.tsx). Kept separate
// so the display logic is unit-testable without a DOM, matching the
// disputes/archive convention.

import type { AuditEvent, AuditKind } from "@/lib/governance-api";

export interface AuditData {
  live: boolean;
  /** The real feed length under the current filter — never `events.length`.
   *  See governance-api's AuditPage and console.rs's own comment: a client
   *  that renders the page size as the backlog understates it the moment the
   *  feed passes `limit`. */
  total: number;
  events: AuditEvent[];
}

/** One screenful of the ledger. Matches the server's own default. */
export const PAGE = 50;

/** The filter tabs, `all` first — mirrors reviews' STATUS_TABS shape so the
 *  two surfaces read as the same idiom. */
export const AUDIT_KIND_TABS: { key: AuditKind | "all"; label: string }[] = [
  { key: "all", label: "all" },
  { key: "promotion_review", label: "promotions" },
  { key: "contradiction_resolution", label: "contradictions" },
  { key: "feedback_resolution", label: "disputes" },
];

/** Validate `?kind=` against the server's own allow-list (console.rs
 *  AUDIT_KINDS) rather than trusting it — an unknown value falls back to no
 *  filter instead of getting forwarded to a 400. */
export function parseKind(raw: string | string[] | undefined): AuditKind | undefined {
  const v = Array.isArray(raw) ? raw[0] : raw;
  const hit = AUDIT_KIND_TABS.find((t) => t.key === v);
  return hit && hit.key !== "all" ? hit.key : undefined;
}

/** `?offset=` — a non-negative integer, or 0 for anything junk/negative. */
export function parseOffset(raw: string | string[] | undefined): number {
  const v = Array.isArray(raw) ? raw[0] : raw;
  const n = Number(v);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0;
}

/** Row label for a governance action kind. */
export function kindLabel(kind: string): string {
  switch (kind) {
    case "promotion_review":
      return "promotion";
    case "contradiction_resolution":
      return "contradiction";
    case "feedback_resolution":
      return "dispute";
    default:
      return kind;
  }
}

export type OutcomeTone = "good" | "bad" | "neutral";

/**
 * Best-effort tone for an outcome string. The three action kinds each have
 * their own outcome vocabulary (approved/rejected, supersede/coexist/dismiss,
 * reverified/deprecated/dismissed) so this is a display classification, not a
 * schema — an unrecognized outcome reads as neutral rather than guessing.
 */
export function outcomeTone(outcome: string): OutcomeTone {
  const o = outcome.toLowerCase();
  if (/approve|reverif|supersede/.test(o)) return "good";
  if (/reject|deprecat|deny/.test(o)) return "bad";
  return "neutral";
}

/** Compact age: 12s / 3m / 2h / 4d ago. Matches the disputes bench's
 *  ageLabel, just measured from an ISO timestamp instead of a seconds count. */
export function ageLabel(iso: string): string {
  const secs = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (secs < 90) return `${Math.round(secs)}s ago`;
  if (secs < 5400) return `${Math.round(secs / 60)}m ago`;
  if (secs < 129600) return `${Math.round(secs / 3600)}h ago`;
  return `${Math.round(secs / 86400)}d ago`;
}

/**
 * The honest actor label — the one piece of copy this module exists to get
 * right. `src/lib/auth.ts` is explicit: the console gate is ONE shared
 * passcode per deployment, so the server stamps every human decision with the
 * SAME principal. `actor_id` therefore names which token decided, never which
 * person — do not render it as though it were a name.
 */
export function actorLabel(actorId: string | null): string {
  if (!actorId) return "policy (auto)";
  return `org token · ${actorId.slice(0, 8)}`;
}

/** Every audit event names a memory, and the archive is the one surface that
 *  shows a memory regardless of its current status — a resolved item has
 *  already left the reviews queue or the disputes bench, so those cannot be
 *  the deep-link target. */
export const MEMORY_HREF = "/console?m=memories";

const demoEvent = (e: Partial<AuditEvent> & Pick<AuditEvent, "kind" | "id" | "memory_id" | "outcome" | "at">): AuditEvent => ({
  memory_b: null,
  detail: null,
  actor_id: null,
  ...e,
});

/** Fixture ledger for the Meridian demo org — used only behind DemoBanner. */
export const DEMO_AUDIT: AuditData = {
  live: false,
  total: 6,
  events: [
    demoEvent({
      kind: "feedback_resolution",
      id: "d1111111-0000-0000-0000-000000000001",
      memory_id: "a1111111-0000-0000-0000-000000000001",
      outcome: "deprecated",
      detail: "confirmed stale — the payments retry policy changed last sprint",
      actor_id: "f00dbabe-0000-0000-0000-000000000001",
      at: new Date(Date.now() - 3 * 3_600_000).toISOString(),
    }),
    demoEvent({
      kind: "contradiction_resolution",
      id: "c1111111-0000-0000-0000-000000000002",
      memory_id: "a1111111-0000-0000-0000-000000000002",
      memory_b: "a1111111-0000-0000-0000-000000000003",
      outcome: "resolved_supersede",
      detail: "the newer rollout note supersedes the retired one",
      actor_id: "f00dbabe-0000-0000-0000-000000000001",
      at: new Date(Date.now() - 26 * 3_600_000).toISOString(),
    }),
    demoEvent({
      kind: "promotion_review",
      id: "p1111111-0000-0000-0000-000000000004",
      memory_id: "a1111111-0000-0000-0000-000000000004",
      outcome: "approved",
      detail: "reviewed against the runbook — matches current practice",
      actor_id: "f00dbabe-0000-0000-0000-000000000001",
      at: new Date(Date.now() - 30 * 3_600_000).toISOString(),
    }),
    demoEvent({
      kind: "promotion_review",
      id: "p1111111-0000-0000-0000-000000000005",
      memory_id: "a1111111-0000-0000-0000-000000000005",
      outcome: "auto_approved",
      detail: "policy: high-confidence + corroborated by 2 sources",
      actor_id: null,
      at: new Date(Date.now() - 48 * 3_600_000).toISOString(),
    }),
    demoEvent({
      kind: "feedback_resolution",
      id: "d1111111-0000-0000-0000-000000000006",
      memory_id: "a1111111-0000-0000-0000-000000000006",
      outcome: "reverified",
      detail: "still accurate — extended the validity window 90 days",
      actor_id: "f00dbabe-0000-0000-0000-000000000001",
      at: new Date(Date.now() - 72 * 3_600_000).toISOString(),
    }),
    demoEvent({
      kind: "contradiction_resolution",
      id: "c1111111-0000-0000-0000-000000000007",
      memory_id: "a1111111-0000-0000-0000-000000000007",
      memory_b: "a1111111-0000-0000-0000-000000000008",
      outcome: "resolved_coexist",
      detail: "both hold — different regions, not actually in conflict",
      actor_id: "f00dbabe-0000-0000-0000-000000000001",
      at: new Date(Date.now() - 96 * 3_600_000).toISOString(),
    }),
  ],
};
