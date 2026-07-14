import type { Metadata } from "next";

import PracticeDivergenceReport from "@/divergence/PracticeDivergence";
import { DEMO_DIVERGENCES } from "@/divergence/divergence-data";

export const metadata: Metadata = { title: "Brainiac — demo · standards" };

// The standardization board on the fixture org. The live page pairs this with
// a SweepControl (which mutates); that control is deliberately absent here —
// a public visitor gets the read, not the write.
export default function DemoDivergencePage() {
  return <PracticeDivergenceReport data={DEMO_DIVERGENCES} />;
}
