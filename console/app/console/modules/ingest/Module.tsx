import DemoBanner from "@/components/DemoBanner";
import {
  configFromEnv,
  getPipelineRuns,
  getQueueHealth,
  getSourcesFeed,
} from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

import { DEMO_INGEST, type IngestData } from "./ingest-data";
import IngestMonitor from "./IngestMonitor";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Ingest Monitor",
};

// Live pipeline feed when reachable; demo shape (behind an unconditional
// DemoBanner) when not.
export default async function IngestPage() {
  const { data, live } = await withDemoFallback<IngestData>(async () => {
    const cfg = configFromEnv();
    const [sources, runs, health] = await Promise.all([
      getSourcesFeed(cfg, 30),
      getPipelineRuns(cfg, 40),
      getQueueHealth(cfg),
    ]);
    return { live: true, sources, runs, health };
  }, DEMO_INGEST);
  return (
    <>
      {!live && <DemoBanner />}
      <IngestMonitor data={data} />
    </>
  );
}
