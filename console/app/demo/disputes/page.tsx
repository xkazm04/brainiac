import type { Metadata } from "next";

import DisputeBench from "../../console/(modules)/disputes/DisputeBench";
import { DEMO_DISPUTES } from "../../console/(modules)/disputes/disputes-data";

export const metadata: Metadata = { title: "Brainiac — demo · disputes" };

// DEMO_DISPUTES carries live:false, which the bench already honours by
// disabling its decision bar — a public visitor can read the contradiction and
// its evidence, but cannot resolve anything.
export default function DemoDisputesPage() {
  return <DisputeBench data={DEMO_DISPUTES} />;
}
