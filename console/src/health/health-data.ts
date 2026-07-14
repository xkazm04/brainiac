/*
 * Demo shape for the Knowledge Health report (KB-PLAN KB0).
 *
 * Rendered only behind <DemoBanner /> when `brainiac serve` is unreachable, so
 * a leader never mistakes it for their org. Deliberately NOT a flattering org:
 * the demo shows an org at "Watch" with a live cross-team contradiction,
 * because a health report whose demo reads 100/Healthy teaches the reader that
 * the number is decoration. The point of the surface is that it can go red.
 */

import type { KnowledgeHealth } from "@/lib/types";

export const DEMO_HEALTH: KnowledgeHealth = {
  score: 61,
  grade: "Watch",
  pillars: { consistency: 56, currency: 88, liquidity: 41, governance: 62 },
  signals: {
    total_memories: 412,
    canonical_entities: 63,
    cross_team_entities: 26,
    open_contradictions: 3,
    cross_team_contradictions: 1,
    stale_beliefs: 12,
    org_wide: 138,
    team_only: 231,
    siloed_private: 43,
    liquidity_pct: 33,
    review_backlog: 9,
    oldest_review_secs: 232_000,
  },
  attention: [
    {
      severity: "critical",
      kind: "contradiction",
      headline: "payments and platform disagree — and neither can see it",
      detail:
        'payments: "refund-worker retry cap is 30s with jitter"  vs  platform: "std-retry caps all consumer retries at 10s"',
    },
    {
      severity: "warning",
      kind: "staleness",
      headline: "12 expired beliefs are still being served as truth",
      detail:
        'Oldest: "checkout v1 is the live checkout flow for all merchants" — expired 2026-02-01, never re-verified.',
    },
    {
      severity: "warning",
      kind: "governance",
      headline: "Review SLO breached — oldest promotion has waited 2d 16h",
      detail:
        "9 promotions pending against a 48h median-review SLO. The capture side is outrunning the review side.",
    },
  ],
  trend: [
    {
      captured_at: "2026-06-16T00:00:00Z",
      score: 74,
      consistency: 78,
      currency: 91,
      liquidity: 38,
      governance: 84,
    },
    {
      captured_at: "2026-06-23T00:00:00Z",
      score: 71,
      consistency: 74,
      currency: 90,
      liquidity: 39,
      governance: 78,
    },
    {
      captured_at: "2026-06-30T00:00:00Z",
      score: 68,
      consistency: 70,
      currency: 89,
      liquidity: 40,
      governance: 71,
    },
    {
      captured_at: "2026-07-07T00:00:00Z",
      score: 61,
      consistency: 56,
      currency: 88,
      liquidity: 41,
      governance: 62,
    },
  ],
  embedding_model: "demo-deterministic",
};
