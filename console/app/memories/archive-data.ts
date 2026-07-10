// Shared substrate for the Archive variants. The list endpoint returns all
// statuses with validity windows, so as-of scrubbing is computed CLIENT-side
// over the fetched corpus (instant, no re-query). Demo corpus mirrors the
// fixtures' supersession chains so time travel demos offline.

import type { MemoryDetail, MemoryRow } from "@/lib/types";

export interface ArchiveData {
  live: boolean;
  total: number;
  rows: MemoryRow[];
}

/** Validity check for as-of scrubbing (null bounds = open interval). */
export function validAt(row: MemoryRow, at: Date): boolean {
  const from = row.valid_from ? new Date(row.valid_from) : null;
  const to = row.valid_to ? new Date(row.valid_to) : null;
  if (from && from > at) return false;
  if (to && to <= at) return false;
  return true;
}

export function timeBounds(rows: MemoryRow[]): { min: Date; max: Date } {
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const r of rows) {
    for (const t of [r.valid_from, r.valid_to, r.created_at]) {
      if (!t) continue;
      const ms = new Date(t).getTime();
      if (Number.isFinite(ms)) {
        min = Math.min(min, ms);
        max = Math.max(max, ms);
      }
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max) || min >= max) {
    return { min: new Date("2025-06-01T00:00:00Z"), max: new Date("2026-07-10T00:00:00Z") };
  }
  return { min: new Date(min), max: new Date(max) };
}

// ── demo corpus (chains mirror the Meridian fixtures) ───────────────────

const row = (
  id: string,
  content: string,
  kind: string,
  team: string,
  opts: Partial<MemoryRow> = {},
): MemoryRow => ({
  id: `dm-${id}`,
  content,
  kind,
  status: "canonical",
  visibility: "team",
  team,
  team_id: `t-${team}`,
  valid_from: null,
  valid_to: null,
  superseded_by: null,
  created_at: "2026-06-20T10:00:00Z",
  confidence: 0.9,
  ...opts,
});

export const DEMO_ROWS: MemoryRow[] = [
  row("psp-10s", "psp-gateway client timeout is 10 seconds", "fact", "payments", {
    status: "deprecated",
    visibility: "org",
    valid_from: "2025-09-01T00:00:00Z",
    valid_to: "2026-05-01T00:00:00Z",
    superseded_by: "dm-psp-30s",
    created_at: "2025-09-01T10:00:00Z",
  }),
  row("psp-30s", "psp-gateway client timeout raised to 30 seconds after the PSP incident review", "decision", "payments", {
    visibility: "org",
    valid_from: "2026-05-01T00:00:00Z",
    created_at: "2026-05-01T09:00:00Z",
  }),
  row("ckv1", "checkout v1 is the live checkout flow for all merchants", "fact", "payments", {
    status: "deprecated",
    visibility: "org",
    valid_from: "2025-06-01T00:00:00Z",
    valid_to: "2026-02-01T00:00:00Z",
    superseded_by: "dm-ckv2",
    created_at: "2025-06-01T08:00:00Z",
  }),
  row("ckv2", "checkout v2 replaced checkout v1 as the live checkout flow; v1 endpoints are frozen", "decision", "payments", {
    visibility: "org",
    valid_from: "2026-02-01T00:00:00Z",
    created_at: "2026-02-01T08:00:00Z",
  }),
  row("jenkins", "production deploys go through the Jenkins pipelines in deploy-tools", "fact", "platform", {
    status: "deprecated",
    visibility: "org",
    valid_from: "2025-01-01T00:00:00Z",
    valid_to: "2026-03-01T00:00:00Z",
    superseded_by: "dm-argocd",
    created_at: "2025-01-05T08:00:00Z",
  }),
  row("argocd", "ArgoCD is the only supported production deploy path since March 2026", "decision", "platform", {
    visibility: "org",
    valid_from: "2026-03-01T00:00:00Z",
    created_at: "2026-03-01T08:00:00Z",
  }),
  row("feast-100", "the feast online serving p99 target is 100ms", "fact", "data", {
    status: "deprecated",
    valid_from: "2025-08-01T00:00:00Z",
    valid_to: "2026-03-01T00:00:00Z",
    superseded_by: "dm-feast-50",
    created_at: "2025-08-01T08:00:00Z",
  }),
  row("feast-50", "the feast online serving p99 target tightened to 50ms after the fraud latency review", "decision", "data", {
    valid_from: "2026-03-01T00:00:00Z",
    created_at: "2026-03-02T08:00:00Z",
  }),
  row("decline", "decline code 05 spikes are issuer-side; retrying burns PSP quota and reads as fraud velocity", "pitfall", "payments", {
    created_at: "2026-06-12T14:00:00Z",
  }),
  row("recon", "reconcile PSP settlement files against ledger-service with the deploy CLI recon command", "howto", "payments", {
    created_at: "2026-05-18T09:00:00Z",
  }),
  row("minor-units", "all monetary amounts in the feature store are integer minor units by contract", "decision", "data", {
    created_at: "2026-06-25T11:00:00Z",
  }),
  row("backfill", "backfill DAG must not run concurrently with the hourly ingest — partition locks deadlock", "pitfall", "data", {
    created_at: "2026-07-01T16:00:00Z",
  }),
  row("opa-exc", "request a deploy exception via an override PR into infra-live/policies; OPA needs two maintainer approvals", "howto", "platform", {
    visibility: "org",
    created_at: "2026-06-05T10:00:00Z",
  }),
  row("msk-disk", "MSK broker storage autoscaling is not enabled — disk expansion is a manual infra-live change", "fact", "platform", {
    created_at: "2026-06-28T13:00:00Z",
  }),
  row("raw-1", "raw candidate: settlement recon runs at 07:00 daily", "fact", "payments", {
    status: "raw",
    created_at: "2026-07-09T07:30:00Z",
  }),
  row("cand-1", "candidate: browser autofill fires duplicate tokenization on new card forms", "pitfall", "payments", {
    status: "candidate",
    created_at: "2026-07-08T15:00:00Z",
  }),
];

export const DEMO_ARCHIVE: ArchiveData = {
  live: false,
  total: DEMO_ROWS.length,
  rows: DEMO_ROWS,
};

export function demoDetail(id: string): MemoryDetail {
  const m = DEMO_ROWS.find((r) => r.id === id) ?? DEMO_ROWS[0];
  const successor = m.superseded_by ? DEMO_ROWS.find((r) => r.id === m.superseded_by) : null;
  const predecessor = DEMO_ROWS.find((r) => r.superseded_by === m.id);
  const link = (r: MemoryRow, depth: number) => ({
    id: r.id,
    content: r.content,
    status: r.status,
    valid_from: r.valid_from,
    valid_to: r.valid_to,
    depth,
  });
  return {
    memory: m,
    provenance: {
      actor_kind: "pipeline",
      actor_id: "extract-worker",
      model_ref: "qwen:qwen-max",
      source_kind: "session_transcript",
      source_ref: "demo-session-114",
    },
    entities: [
      { name: "psp-gateway", kind: "service", team: m.team },
      { name: "retry backoff rules", kind: "concept", team: m.team },
    ],
    promotions: [
      {
        from_status: "raw",
        to_status: "candidate",
        policy_decision: "auto_approved",
        policy_rule: `${m.kind}.high_confidence`,
        reviewed_at: null,
        created_at: m.created_at,
      },
      {
        from_status: "candidate",
        to_status: "canonical",
        policy_decision: "approved",
        policy_rule: "human.maintainer",
        reviewed_at: m.created_at,
        created_at: m.created_at,
      },
    ],
    chain: {
      predecessors: predecessor ? [link(predecessor, -1)] : [],
      successors: successor ? [link(successor, 1)] : [],
    },
  };
}
