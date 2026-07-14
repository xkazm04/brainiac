import DemoBanner from "@/components/DemoBanner";
import KnowledgeHealthReport from "@/health/KnowledgeHealth";
import { DEMO_HEALTH } from "@/health/health-data";
import { configFromEnv, getKnowledgeHealth, getSweeps } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { KnowledgeHealth, SweepSchedule } from "@/lib/types";
import SweepControl from "@/ops/SweepControl";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Knowledge Health",
};

/** The health-snapshot sweep's schedule — best-effort; hidden offline, where a
 *  mutating control over demo data would mislead. Scheduling it is what fills
 *  the trend line over weeks without a manual snapshot click. */
async function sweepFor(kind: string): Promise<SweepSchedule | null> {
  try {
    const s = await getSweeps(configFromEnv());
    return s.sweeps.find((x) => x.kind === kind) ?? null;
  } catch {
    return null;
  }
}

// Live when `brainiac serve` is reachable; the demo org behind an unconditional
// DemoBanner when not — a leader must never read a fabricated score as theirs.
export default async function HealthPage() {
  const { data, live } = await withDemoFallback<KnowledgeHealth>(
    () => getKnowledgeHealth(configFromEnv()),
    DEMO_HEALTH,
  );
  const sweep = live ? await sweepFor("health_snapshot") : null;
  return (
    <>
      {!live && <DemoBanner />}
      {sweep && (
        <div className="px-6 pt-8">
          <SweepControl schedule={sweep} title="health snapshot" revalidate="/console/health" />
        </div>
      )}
      <KnowledgeHealthReport data={data} />
    </>
  );
}
