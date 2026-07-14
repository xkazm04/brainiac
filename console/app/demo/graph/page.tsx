import type { Metadata } from "next";

import CortexMap from "../../console/(modules)/graph/CortexMap";
import { DEMO_CORTEX } from "../../console/(modules)/graph/cortex-data";

export const metadata: Metadata = { title: "Brainiac — demo · cortex map" };

// The real graph component on the fixture overview. DEMO_CORTEX carries
// live:false, so the drill-in hook synthesizes canonical detail locally instead
// of calling the gated /api/graph/canonical route.
export default function DemoGraphPage() {
  return <CortexMap data={DEMO_CORTEX} />;
}
