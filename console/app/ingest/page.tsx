import {
  configFromEnv,
  getPipelineRuns,
  getQueueHealth,
  getSourcesFeed,
} from "@/lib/api";

import { DEMO_INGEST, type IngestData } from "./ingest-data";
import IngestLab from "./IngestLab";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Ingest Monitor",
};

async function ingestData(): Promise<IngestData> {
  try {
    const cfg = configFromEnv();
    const [sources, runs, health] = await Promise.all([
      getSourcesFeed(cfg, 30),
      getPipelineRuns(cfg, 40),
      getQueueHealth(cfg),
    ]);
    return { live: true, sources, runs, health };
  } catch {
    return DEMO_INGEST;
  }
}

export default async function IngestPage() {
  return <IngestLab data={await ingestData()} />;
}
