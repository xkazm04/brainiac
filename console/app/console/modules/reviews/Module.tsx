import ApiOffline from "@/components/ApiOffline";
import { configFromEnv } from "@/lib/api";
import {
  contradictionQueue,
  promotionQueue,
  type ContradictionStatus,
} from "@/lib/governance-api";

import ReviewsLive from "./ReviewsLive";
import { STATUS_TABS } from "./review-surface";

export const dynamic = "force-dynamic";

function asStatus(v: string | string[] | undefined): ContradictionStatus {
  const s = Array.isArray(v) ? v[0] : v;
  return STATUS_TABS.some((t) => t.key === s) ? (s as ContradictionStatus) : "open";
}

/*
 * The operator's review queue: the live queues, and the only controls in the
 * product that can make something canonical.
 *
 * The surface is ReviewWorklist (the triage rail that won the 2026-07-15
 * round), which the public tour renders too — so what a visitor evaluates at
 * /demo is this page, minus the data and the write path. The controls are built
 * inside ReviewsLive rather than passed from here: a server component cannot
 * hand a render prop to a client one.
 */
export async function ReviewsModule({
  searchParams,
}: {
  searchParams: Record<string, string | string[] | undefined>;
}) {
  const cstatus = asStatus(searchParams.cstatus);
  const cfg = configFromEnv();
  let promotions, contradictionsPage;
  // Deliberate exception to withDemoFallback (see src/lib/demo-fallback.ts):
  // reviews is a write surface (approve / reject / resolve), so it hard-stops
  // rather than showing a fabricated queue wired to real actions.
  try {
    [promotions, contradictionsPage] = await Promise.all([
      promotionQueue(cfg),
      contradictionQueue(cfg, { status: cstatus }),
    ]);
  } catch (e) {
    return <ApiOffline error={e instanceof Error ? e.message : String(e)} />;
  }

  return (
    <ReviewsLive
      promotions={promotions}
      contradictions={contradictionsPage.contradictions}
      counts={contradictionsPage.counts}
      cstatus={cstatus}
    />
  );
}

export default ReviewsModule;
