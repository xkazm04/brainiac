import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, getGraphOverview } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

import { DEMO_CORTEX, type CortexData } from "./cortex-data";
import CortexMap from "./CortexMap";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Cortex Map",
};

// Live multi-level graph when brainiac serve is up; the demo shape (behind an
// unconditional DemoBanner) when not, so both lenses render either way.
export default async function GraphPage() {
  const { data, live } = await withDemoFallback<CortexData>(
    async () => ({ live: true, overview: await getGraphOverview(configFromEnv()) }),
    DEMO_CORTEX,
  );
  return (
    <>
      {!live && <DemoBanner />}
      <CortexMap data={data} />
    </>
  );
}
