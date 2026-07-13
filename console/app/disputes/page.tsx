import DemoBanner from "@/components/DemoBanner";
import { configFromEnv } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import { feedbackQueue } from "@/lib/governance-api";

import DisputeBench from "./DisputeBench";
import { DEMO_DISPUTES, type DisputeData } from "./disputes-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Disputes",
};

// Live queue when the server is reachable; the demo shape (behind an
// unconditional DemoBanner, actions disabled) when it isn't — never 500s.
export default async function DisputesPage() {
  const { data, live } = await withDemoFallback<DisputeData>(
    async () => ({ live: true, flagged: await feedbackQueue(configFromEnv(), 50) }),
    DEMO_DISPUTES,
  );
  return (
    <>
      {!live && <DemoBanner />}
      <DisputeBench data={data} />
    </>
  );
}
