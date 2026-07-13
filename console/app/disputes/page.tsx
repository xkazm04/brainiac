import { configFromEnv } from "@/lib/api";
import { feedbackQueue } from "@/lib/governance-api";

import DisputeLab from "./DisputeLab";
import { DEMO_DISPUTES, type DisputeData } from "./disputes-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Disputes",
};

// Live queue when the server is reachable; the demo shape (clearly labeled,
// actions disabled) when it isn't — the page never 500s.
async function disputeData(): Promise<DisputeData> {
  try {
    return { live: true, flagged: await feedbackQueue(configFromEnv(), 50) };
  } catch {
    return DEMO_DISPUTES;
  }
}

export default async function DisputesPage() {
  return <DisputeLab data={await disputeData()} />;
}
