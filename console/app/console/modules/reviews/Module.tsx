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

/**
 * The page window, at the server's ceiling for both queues (`limit` clamps to
 * 200). Deliberately the maximum: the rail's facets, its stale tally and its
 * select-all are computed client-side over whatever is in the page, so the page
 * is the working set, and a bigger one is a better one right up to the cap.
 * Past the cap the rail pages and says so — see scopeNote in ./review-surface.
 */
const PAGE = 200;

/** Read `?poffset=` into a page offset. Junk and negatives fall back to page one. */
function asOffset(v: string | string[] | undefined): number {
  const raw = Array.isArray(v) ? v[0] : v;
  const n = Number(raw);
  return Number.isSafeInteger(n) && n > 0 ? n : 0;
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
  const poffset = asOffset(searchParams.poffset);
  const cfg = configFromEnv();
  let promotionsPage, contradictionsPage;
  // Deliberate exception to withDemoFallback (see src/lib/demo-fallback.ts):
  // reviews is a write surface (approve / reject / resolve), so it hard-stops
  // rather than showing a fabricated queue wired to real actions.
  try {
    [promotionsPage, contradictionsPage] = await Promise.all([
      promotionQueue(cfg, { limit: PAGE, offset: poffset }),
      contradictionQueue(cfg, { status: cstatus, limit: PAGE }),
    ]);
  } catch (e) {
    return <ApiOffline error={e instanceof Error ? e.message : String(e)} />;
  }

  return (
    <ReviewsLive
      promotions={promotionsPage.promotions}
      promotionsTotal={promotionsPage.total}
      promotionsOffset={poffset}
      contradictions={contradictionsPage.contradictions}
      contradictionsTotal={contradictionsPage.total}
      counts={contradictionsPage.counts}
      cstatus={cstatus}
    />
  );
}

export default ReviewsModule;
