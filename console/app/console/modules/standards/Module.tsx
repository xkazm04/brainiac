import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, getStandard, getSweeps, listStandards } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { LibraryStandard, StandardDetail, SweepSchedule } from "@/lib/types";
import SweepControl from "@/ops/SweepControl";

import { DEMO_STANDARD_DETAILS, DEMO_STANDARDS } from "./standards-data";
import StandardsBoard from "./StandardsBoard";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Standards",
};

/**
 * Fetch every rule's detail up front (parallel, bounded). The board's whole
 * interaction is hopping between rules, and an org's library is tens of rules,
 * not thousands — prefetching buys instant selection for one burst of small
 * queries. If a library ever outgrows the cap, the tail renders from the list
 * row alone and the fix is a detail endpoint per click, not a bigger burst.
 */
const DETAIL_PREFETCH_CAP = 100;

async function fetchLive(): Promise<{
  standards: LibraryStandard[];
  details: Record<string, StandardDetail>;
}> {
  const cfg = configFromEnv();
  const standards = await listStandards(cfg, "all");
  const details: Record<string, StandardDetail> = {};
  const fetched = await Promise.all(
    standards.slice(0, DETAIL_PREFETCH_CAP).map((s) => getStandard(cfg, s.id)),
  );
  for (const d of fetched) details[d.id] = d;
  return { standards, details };
}

/** The mining sweep's schedule — best-effort; the control is hidden when the
 *  server is unreachable (a mutating control over demo data would lie). */
async function librarySweep(): Promise<SweepSchedule | null> {
  try {
    const s = await getSweeps(configFromEnv());
    return s.sweeps.find((x) => x.kind === "library") ?? null;
  } catch {
    return null;
  }
}

export default async function StandardsModule() {
  const { data, live } = await withDemoFallback(fetchLive, {
    standards: DEMO_STANDARDS,
    details: DEMO_STANDARD_DETAILS,
  });
  const sweep = live ? await librarySweep() : null;
  return (
    <>
      {!live && <DemoBanner />}
      {sweep && (
        <div className="px-6 pt-8">
          <SweepControl schedule={sweep} title="library mining" revalidate="/console" />
        </div>
      )}
      <StandardsBoard standards={data.standards} details={data.details} live={live} />
    </>
  );
}
