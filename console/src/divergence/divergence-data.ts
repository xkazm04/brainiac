/*
 * Demo shape for the practice-divergence report, rendered only behind
 * <DemoBanner /> when `brainiac serve` is unreachable — so a leader never
 * mistakes it for their org's real standardization board.
 *
 * The first card is the actual finding from the Meridian fixture sweep (the
 * retry-policy divergence); the second is a second realistic shape so the demo
 * shows a populated board rather than a lone card. Deliberately conservative in
 * count — the real detector flags few, and a demo overflowing with divergences
 * would teach the reader the surface cries wolf.
 */

import type { PracticeDivergences } from "@/lib/types";

export const DEMO_DIVERGENCES: PracticeDivergences = {
  divergences: [
    {
      practice: "service retry policy",
      summary:
        "The platform team and the payments team run different retry caps for service calls, so the same failure is handled two ways depending on who owns the code.",
      recommended_standard:
        "Adopt a standard retry cap of 2 seconds with 3 attempts for all internal service calls, including the refund-worker — unless a specific use case is identified and approved.",
      impact: "high",
      approaches: [
        { team: "platform", approach: "retry cap of 2 seconds, 3 attempts, for all internal service calls" },
        { team: "payments", approach: "retry cap of 30 seconds with jitter for refund-worker, aligned to std-retry policy" },
      ],
      model_ref: "qwen:qwen-max",
      detected_at: "2026-07-14T00:00:00Z",
    },
    {
      practice: "idempotency key TTL",
      summary:
        "Payments and platform expire idempotency keys on different clocks, so a retried request that is safe for one service is a duplicate for the other.",
      recommended_standard:
        "Standardize idempotency-key retention at 24 hours across services, with a documented exception path for long-running settlement flows.",
      impact: "medium",
      approaches: [
        { team: "payments", approach: "idempotency keys retained 7 days to cover settlement reconciliation" },
        { team: "platform", approach: "idempotency keys expire after 1 hour to bound Redis memory" },
      ],
      model_ref: "qwen:qwen-max",
      detected_at: "2026-07-14T00:00:00Z",
    },
  ],
};
