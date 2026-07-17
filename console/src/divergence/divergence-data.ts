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
      axis: "team",
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
      axis: "team",
    },
    // The PR3 class: two APPLICATIONS diverging — a per-stack rule candidate
    // for the Library, not a team conversation.
    {
      practice: "client timeout budget",
      summary:
        "payments-api calls the PSP with a 30-second timeout while checkout-web abandons the same call at 5 seconds, so one user journey gives up while the other is still waiting.",
      recommended_standard:
        "Adopt a shared 10-second end-to-end timeout budget for PSP calls, with checkout surfacing a pending state instead of abandoning.",
      impact: "medium",
      approaches: [
        { project: "payments-api", approach: "30s PSP client timeout after the incident review" },
        { project: "checkout-web", approach: "5s fetch abort to keep the checkout interactive" },
      ],
      model_ref: "qwen:qwen-max",
      detected_at: "2026-07-14T00:00:00Z",
      axis: "project",
    },
  ],
};
