// Substrate for the Disputes bench: the claim model, severity ordering,
// decay math, and the demo queue.
//
// Client-safe by construction — it must NOT import the server-only API
// client, so the shape is declared here (structurally identical to
// governance-api's FlaggedMemory, which page.tsx passes straight through).

/** One open claim, attributed and dated — see console.rs `FeedbackReport`. */
export interface ClaimReport {
  verdict: "wrong" | "outdated";
  note: string | null;
  reporter_id: string;
  /** Null when the org holds no email for the reporter. */
  reporter_email: string | null;
  /** The reporter sits on the memory's owning team. */
  reporter_on_owning_team: boolean;
  /** How long ago this claim was filed. */
  age_secs: number;
}

export interface DisputedMemory {
  memory_id: string;
  title: string | null;
  content: string;
  kind: string;
  status: string;
  team_id: string | null;
  /** The owning team's NAME. Null for org-wide memories. */
  team: string | null;
  /** How sure the corpus was when it accepted this. */
  confidence: number | null;
  /** End of the memory's validity window (TTL), if it has one. */
  valid_to: string | null;
  /** Who put this in the corpus — human, doc, or an LLM extraction. */
  provenance: { actor_kind: string; actor_id: string; model_ref: string | null } | null;
  claims: { wrong: number; outdated: number };
  /** DISTINCT reporters behind the open claims. */
  reporters: number;
  /** The open claims, newest first, capped server-side. */
  reports: ClaimReport[];
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

/**
 * How to name a reporter on screen. `users.email` is nullable — agent
 * principals routinely have none — so the id is the honest fallback rather
 * than an empty pair of quotation marks.
 */
export function reporterLabel(r: ClaimReport): string {
  return r.reporter_email ?? `user ${r.reporter_id.slice(0, 8)}`;
}

/**
 * The validity budget a re-verification buys. The API has accepted `days`
 * (clamped 1..3650) since the endpoint existed and no UI ever sent it, so a
 * maintainer who knew a fact was good for exactly 30 days had no way to say so
 * and silently got the kind's default instead.
 */
export const EXTEND_CHOICES: { days: number | null; label: string }[] = [
  { days: null, label: "kind default" },
  { days: 30, label: "30d" },
  { days: 90, label: "90d" },
  { days: 180, label: "180d" },
  { days: 365, label: "1y" },
];

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

/** Stable fake reporters, so the fixture can show the reporter question. */
const R = {
  lead: "aa000000-0000-4000-8000-00000000000a",
  dev: "aa000000-0000-4000-8000-00000000000b",
  bot: "aa000000-0000-4000-8000-00000000000c",
};

const rep = (
  who: keyof typeof R,
  verdict: "wrong" | "outdated",
  note: string | null,
  ageSecs: number,
  onTeam = true,
): ClaimReport => ({
  verdict,
  note,
  reporter_id: R[who],
  reporter_email: who === "bot" ? null : `${who}@example.com`,
  reporter_on_owning_team: onTeam,
  age_secs: ageSecs,
});

export const DEMO_DISPUTES: DisputeData = {
  live: false,
  flagged: [
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000001",
      title: "PSP webhook secret rotation",
      content:
        "The PSP webhook signing secret rotates every 90 days; the rotation is announced in #payments-oncall a week ahead.",
      kind: "fact",
      status: "canonical",
      team_id: "3f2a9c1e-0000-4000-8000-000000000001",
      team: "payments",
      confidence: 0.62,
      valid_to: d(41),
      provenance: { actor_kind: "agent", actor_id: "extractor-7", model_ref: "claude-sonnet-4" },
      claims: { wrong: 2, outdated: 1 },
      reporters: 2,
      reports: [
        rep("lead", "wrong", "rotation moved to 30 days after the Q2 incident — this misled me twice", 2 * 86400),
        rep("dev", "outdated", "no announcement channel anymore, it's in the vault changelog", 5 * 86400),
        rep("lead", "wrong", null, 9 * 86400),
      ],
      oldest_claim_secs: 9 * 86400,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000002",
      title: "Refund workers read the primary",
      content:
        "Refund workers read from the primary Kafka cluster; the replica is analytics-only and must never be used for money movement.",
      kind: "decision",
      status: "canonical",
      team_id: "3f2a9c1e-0000-4000-8000-000000000001",
      team: "payments",
      confidence: 0.91,
      valid_to: d(-3),
      provenance: { actor_kind: "human", actor_id: "lead@example.com", model_ref: null },
      claims: { wrong: 0, outdated: 2 },
      reporters: 2,
      reports: [
        rep("dev", "outdated", "we migrated refunds to the replica in the MSK cutover", 2 * 86400),
        rep("lead", "outdated", null, 4 * 86400),
      ],
      oldest_claim_secs: 4 * 86400,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000003",
      title: "Event lake backfill",
      content:
        "To backfill the event lake, run the nightly job with --from and --to; it is idempotent and safe to re-run.",
      kind: "howto",
      status: "canonical",
      team_id: "3f2a9c1e-0000-4000-8000-000000000002",
      team: "data",
      confidence: 0.44,
      valid_to: d(12),
      provenance: { actor_kind: "agent", actor_id: "extractor-7", model_ref: "claude-sonnet-4" },
      claims: { wrong: 1, outdated: 0 },
      reporters: 1,
      reports: [rep("dev", "wrong", "re-running double-counted a partition — it is NOT idempotent", 31 * 3600)],
      oldest_claim_secs: 31 * 3600,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000004",
      title: null,
      content:
        "OPA policy bundles are pulled every 5 minutes; a bad bundle fails closed and blocks all checkout traffic.",
      kind: "pitfall",
      status: "candidate",
      team_id: null,
      team: null,
      confidence: null,
      valid_to: d(120),
      provenance: null,
      claims: { wrong: 0, outdated: 1 },
      reporters: 1,
      // The case the old payload could not tell apart from a human's report.
      reports: [rep("bot", "outdated", null, 6 * 3600, false)],
      oldest_claim_secs: 6 * 3600,
    },
    {
      memory_id: "d1e0a5c2-0000-4000-8000-000000000005",
      title: "Fraud scoring is synchronous",
      content:
        "Fraud scoring runs synchronously in the checkout path with a 200ms budget.",
      kind: "fact",
      status: "canonical",
      team_id: "3f2a9c1e-0000-4000-8000-000000000002",
      team: "data",
      confidence: 0.71,
      valid_to: d(74),
      provenance: { actor_kind: "agent", actor_id: "extractor-7", model_ref: "claude-sonnet-4" },
      claims: { wrong: 1, outdated: 1 },
      // Two claims, ONE reporter — the distinction the tally alone hides.
      reporters: 1,
      reports: [
        rep("bot", "wrong", "it's async since the scoring service split", 4 * 86400, false),
        rep("bot", "outdated", null, 20 * 86400, false),
      ],
      oldest_claim_secs: 20 * 86400,
    },
  ],
};
