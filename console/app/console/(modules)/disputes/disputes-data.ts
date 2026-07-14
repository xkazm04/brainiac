// Substrate for the Disputes bench: the claim model, severity ordering,
// decay math, and the demo queue.
//
// Client-safe by construction — it must NOT import the server-only API
// client, so the shape is declared here (structurally identical to
// governance-api's FlaggedMemory, which page.tsx passes straight through).

export interface DisputedMemory {
  memory_id: string;
  content: string;
  kind: string;
  status: string;
  team_id: string | null;
  /** End of the memory's validity window (TTL), if it has one. */
  valid_to: string | null;
  claims: { wrong: number; outdated: number };
  /** What the reporters said, newest first. */
  notes: string[];
  /** How long the oldest unanswered claim has stood. */
  oldest_claim_secs: number;
}

export interface DisputeData {
  live: boolean;
  flagged: DisputedMemory[];
}

export type Resolution = "reverified" | "deprecated" | "dismissed";

/** The three answers, in the order a reviewer weighs them. */
export const DECISIONS: { id: Resolution; verb: string; gloss: string }[] = [
  { id: "reverified", verb: "still true", gloss: "checked it — extend its validity window" },
  { id: "deprecated", verb: "they're right", gloss: "end it now — drop it out of retrieval" },
  { id: "dismissed", verb: "noise", gloss: "the reports are wrong — the memory stands" },
];

/**
 * Destructive weight of the claims against a memory. `wrong` (this was never
 * true) outranks `outdated` (this stopped being true) — one is a defect, the
 * other is decay.
 */
export function severity(m: DisputedMemory): number {
  return m.claims.wrong * 2 + m.claims.outdated;
}

export function claimCount(m: DisputedMemory): number {
  return m.claims.wrong + m.claims.outdated;
}

/** Days until the validity window closes; negative = already expired. */
export function daysLeft(m: DisputedMemory): number | null {
  if (!m.valid_to) return null;
  return (new Date(m.valid_to).getTime() - Date.now()) / 86_400_000;
}

/** Compact age: 12m / 3.4h / 2d. */
export function ageLabel(secs: number): string {
  if (secs < 90) return `${Math.round(secs)}s`;
  if (secs < 5400) return `${Math.round(secs / 60)}m`;
  if (secs < 129600) return `${Math.round(secs / 3600)}h`;
  return `${Math.round(secs / 86400)}d`;
}

/** Most-disputed first, then longest-standing — the server's own ordering. */
export function triageOrder(rows: DisputedMemory[]): DisputedMemory[] {
  return [...rows].sort(
    (a, b) =>
      severity(b) - severity(a) ||
      claimCount(b) - claimCount(a) ||
      b.oldest_claim_secs - a.oldest_claim_secs,
  );
}

// ── demo queue (server unreachable) ─────────────────────────────────────

const d = (days: number) => new Date(Date.now() + days * 86_400_000).toISOString();

export const DEMO_DISPUTES: DisputeData = {
  live: false,
  flagged: [
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000001",
      content:
        "The PSP webhook signing secret rotates every 90 days; the rotation is announced in #payments-oncall a week ahead.",
      kind: "fact",
      status: "canonical",
      team_id: "payments",
      valid_to: d(41),
      claims: { wrong: 2, outdated: 1 },
      notes: [
        "rotation moved to 30 days after the Q2 incident — this misled me twice",
        "no announcement channel anymore, it's in the vault changelog",
      ],
      oldest_claim_secs: 9 * 86400,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000002",
      content:
        "Refund workers read from the primary Kafka cluster; the replica is analytics-only and must never be used for money movement.",
      kind: "decision",
      status: "canonical",
      team_id: "payments",
      valid_to: d(-3),
      claims: { wrong: 0, outdated: 2 },
      notes: ["we migrated refunds to the replica in the MSK cutover"],
      oldest_claim_secs: 4 * 86400,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000003",
      content:
        "To backfill the event lake, run the nightly job with --from and --to; it is idempotent and safe to re-run.",
      kind: "howto",
      status: "canonical",
      team_id: "data",
      valid_to: d(12),
      claims: { wrong: 1, outdated: 0 },
      notes: ["re-running double-counted a partition — it is NOT idempotent"],
      oldest_claim_secs: 31 * 3600,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000004",
      content:
        "OPA policy bundles are pulled every 5 minutes; a bad bundle fails closed and blocks all checkout traffic.",
      kind: "pitfall",
      status: "candidate",
      team_id: "platform",
      valid_to: d(120),
      claims: { wrong: 0, outdated: 1 },
      notes: [],
      oldest_claim_secs: 6 * 3600,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000005",
      content:
        "Fraud scoring runs synchronously in the checkout path with a 200ms budget.",
      kind: "fact",
      status: "canonical",
      team_id: "data",
      valid_to: d(74),
      claims: { wrong: 1, outdated: 1 },
      notes: ["it's async since the scoring service split"],
      oldest_claim_secs: 20 * 86400,
    },
  ],
};
