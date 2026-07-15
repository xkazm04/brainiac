import DemoBanner from "@/components/DemoBanner";
import PracticeDivergenceReport from "@/divergence/PracticeDivergence";
import { DEMO_DIVERGENCES } from "@/divergence/divergence-data";
import { configFromEnv, getPracticeDivergence, getSweeps } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { PracticeDivergences, SweepSchedule } from "@/lib/types";
import SweepControl from "@/ops/SweepControl";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Standardization",
};

/** The divergence sweep's schedule — best-effort; the control is hidden when
 *  the server is unreachable (a mutating control over demo data would lie). */
async function sweepFor(kind: string): Promise<SweepSchedule | null> {
  try {
    const s = await getSweeps(configFromEnv());
    return s.sweeps.find((x) => x.kind === kind) ?? null;
  } catch {
    return null;
  }
}

// Live when `brainiac serve` is reachable; the demo board behind a DemoBanner
// when not — a leader must never read fabricated divergences as their org's.
export default async function DivergencePage() {
  const { data, live } = await withDemoFallback<PracticeDivergences>(
    () => getPracticeDivergence(configFromEnv()),
    DEMO_DIVERGENCES,
  );
  const sweep = live ? await sweepFor("divergence") : null;
  return (
    <>
      {!live && <DemoBanner />}
      {sweep && (
        <div className="px-6 pt-8">
          <SweepControl schedule={sweep} title="divergence scan" revalidate="/console/divergence" />
        </div>
      )}
      <PracticeDivergenceReport data={data} />
    </>
  );
}
