import DemoBanner from "@/components/DemoBanner";
import KnowledgeHealthReport from "@/health/KnowledgeHealth";
import { DEMO_HEALTH } from "@/health/health-data";
import { configFromEnv, getKnowledgeHealth } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { KnowledgeHealth } from "@/lib/types";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Knowledge Health",
};

// Live when `brainiac serve` is reachable; the demo org behind an unconditional
// DemoBanner when not — a leader must never read a fabricated score as theirs.
export default async function HealthPage() {
  const { data, live } = await withDemoFallback<KnowledgeHealth>(
    () => getKnowledgeHealth(configFromEnv()),
    DEMO_HEALTH,
  );
  return (
    <>
      {!live && <DemoBanner />}
      <KnowledgeHealthReport data={data} />
    </>
  );
}
