/*
 * Fixtures for the PUBLIC review queue.
 *
 * The operator /console/reviews page deliberately has no demo fallback: it is a
 * write surface, and a fabricated queue wired to real approve/reject actions
 * would be dangerous (see src/lib/demo-fallback.ts). So the showcase brings its
 * own read-only fixtures rather than borrowing that page's DATA path.
 *
 * It does share that page's SURFACE — both render ReviewQueue — because the two
 * looking identical is the entire value of the tour. What the showcase never
 * gets is the write path: app/demo/DemoReviews.tsx passes inert stamps where the
 * operator passes the real controls, so the server actions are never linked into
 * a public bundle at all.
 *
 * The content tracks the Meridian fixture org used everywhere else — the same
 * payments/platform/data teams, the same refund-worker and PSP-timeout story the
 * eval harness and the UAT trial run on — so the demo tells one coherent story.
 * DEMO_COUNTS mirrors the status tallies the live queue endpoint returns for the
 * filter tabs; it must stay consistent with DEMO_CONTRADICTIONS above.
 */

import type { ContradictionQueueItem, PromotionQueueItem } from "@/lib/governance-api";

export const DEMO_PROMOTIONS: PromotionQueueItem[] = [
  {
    id: "prm-0001",
    memory_id: "mem-pay-0043",
    from_status: "candidate",
    to_status: "canonical",
    policy_rule: "cross_team_requires_maintainer",
    age_secs: 5 * 3600 + 12 * 60,
    memory: {
      content:
        "The refund worker retries a failed refund at most 3 times with a 30s cap. Any consumer deduplicating refund events must use a window of at least 30s, or it will double-count a retried refund.",
      kind: "decision",
      status: "candidate",
      confidence: 0.93,
      team: "payments",
    },
    provenance: {
      actor_kind: "pipeline",
      actor_id: "extract-worker",
      model_ref: "qwen:qwen-max",
      source_kind: "session_transcript",
      source_ref: "pay-incident-011",
    },
  },
  {
    id: "prm-0002",
    memory_id: "mem-pay-0051",
    from_status: "raw",
    to_status: "candidate",
    policy_rule: "pitfall_high_conf_auto_candidate",
    age_secs: 41 * 60,
    memory: {
      content:
        "Never process a chargeback and a refund for the same transaction concurrently — the ledger writes both and the account is debited twice.",
      kind: "pitfall",
      status: "raw",
      confidence: 1.0,
      team: "payments",
    },
    provenance: {
      actor_kind: "pipeline",
      actor_id: "extract-worker",
      model_ref: "qwen:qwen-max",
      source_kind: "session_transcript",
      source_ref: "pay-incident-007",
    },
  },
  {
    id: "prm-0003",
    memory_id: "mem-plat-0018",
    from_status: "candidate",
    to_status: "canonical",
    policy_rule: "cross_team_requires_maintainer",
    age_secs: 2 * 24 * 3600 + 3 * 3600,
    memory: {
      content:
        "The PSP timeout was raised from 15s to 30s on 2026-04-01. Any client-side abort shorter than the PSP timeout can abandon an in-flight charge that later succeeds — producing a silent double charge.",
      kind: "decision",
      status: "candidate",
      confidence: 0.88,
      team: "platform",
    },
    provenance: {
      actor_kind: "human",
      actor_id: "platlead@meridian.example",
      model_ref: null,
      source_kind: "manual",
      source_ref: "arch-review-2026-04-01",
    },
  },
  {
    // The one that shows RLS is real: a maintainer sees the promotion exists,
    // but not the content, because the memory is out of their scope.
    id: "prm-0004",
    memory_id: "mem-data-0072",
    from_status: "candidate",
    to_status: "canonical",
    policy_rule: "team_maintainer",
    age_secs: 9 * 3600,
    memory: null,
    provenance: null,
  },
];

export const DEMO_CONTRADICTIONS: ContradictionQueueItem[] = [
  {
    id: "ctr-0001",
    memory_a: {
      id: "mem-pay-0043",
      content:
        "The refund worker retries at most 3 times with a 30s cap; dedup windows must be at least 30s.",
    },
    memory_b: {
      id: "mem-plat-0009",
      content:
        "Refund dedup must match the standard retry policy of 2s — the 30s override was reverted.",
    },
    detected_by: "qwen:qwen-max",
    status: "open",
    suggested_resolution: "supersede",
    resolved_by: null,
    resolved_at: null,
    created_at: "2026-07-11T09:20:00Z",
    age_secs: 3 * 24 * 3600 + 40 * 60,
  },
  {
    id: "ctr-0002",
    memory_a: {
      id: "mem-plat-0018",
      content: "The PSP timeout is 30s as of 2026-04-01.",
    },
    memory_b: {
      id: "mem-web-0004",
      content: "The PSP times out at 15s; a 15s client abort is therefore safe.",
    },
    detected_by: "qwen:qwen-max",
    status: "resolved_supersede",
    suggested_resolution: "supersede",
    resolved_by: "paylead@meridian.example",
    resolved_at: "2026-07-12T14:05:00Z",
    created_at: "2026-07-12T09:30:00Z",
    age_secs: 2 * 24 * 3600,
  },
];

export const DEMO_COUNTS = [
  { status: "open", count: 1 },
  { status: "resolved_supersede", count: 1 },
  { status: "resolved_coexist", count: 0 },
  { status: "dismissed", count: 0 },
];
