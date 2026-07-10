import { configFromEnv, getGraphOverview } from "@/lib/api";

import { DEMO_CORTEX, type CortexData } from "./cortex-data";
import CortexMap from "./CortexMap";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Cortex Map",
};

// Live multi-level graph when brainiac serve is up; demo
// shape when not, so both lenses render either way.
async function cortexData(): Promise<CortexData> {
  try {
    return { live: true, overview: await getGraphOverview(configFromEnv()) };
  } catch {
    return DEMO_CORTEX;
  }
}

export default async function GraphPage() {
  return <CortexMap data={await cortexData()} />;
}
