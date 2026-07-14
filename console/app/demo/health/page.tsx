import type { Metadata } from "next";

import KnowledgeHealthReport from "@/health/KnowledgeHealth";
import { DEMO_HEALTH } from "@/health/health-data";

export const metadata: Metadata = { title: "Brainiac — demo · knowledge health" };

// The health report on the fixture org. The live page pairs this with a
// SweepControl (which mutates); that control is deliberately absent here — a
// public visitor gets the read, not the write.
export default function DemoHealthPage() {
  return <KnowledgeHealthReport data={DEMO_HEALTH} />;
}
