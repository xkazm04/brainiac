import { Suspense } from "react";
import type { Metadata } from "next";

import { DEMO_DISPUTES } from "../console/modules/disputes/disputes-data";
import { DEMO_CORTEX } from "../console/modules/graph/cortex-data";
import { DEMO_ARCHIVE } from "../console/modules/memories/archive-data";

import { DEMO_DIVERGENCES } from "@/divergence/divergence-data";
import { DEMO_HEALTH } from "@/health/health-data";
import { DEMO_OBSERVATORY } from "@/observatory/observatory-data";

import DemoConsole from "./DemoConsole";
import { DEMO_CONTRADICTIONS, DEMO_COUNTS, DEMO_PROMOTIONS } from "./demo-reviews-data";
import Loading from "./loading";

export const metadata: Metadata = {
  title: "Brainiac — the demo org",
  description:
    "Walk a governed knowledge base end to end on a synthetic org: the review gate, contradictions, the canonical graph, the archive, a knowledge-health score, and the standards board.",
};

/*
 * The tour's only route. It exists as a server component for one reason: it is
 * where the fixtures are read. DEMO_DISPUTES lives behind `import "server-only"`,
 * so it can be loaded here and handed down, but never imported by the client
 * shell — which is why DemoConsole takes its data as props.
 *
 * Every fixture carries live:false. That flag, not the absence of a session, is
 * what makes this subtree safe: each component degrades on it — synthesizing
 * drill-in detail client-side instead of calling a gated /api route, and
 * disabling its write controls. No API token is ever used under /demo.
 *
 * The Suspense boundary is required: DemoConsole reads the active module from
 * the query string on its first render (useSearchParams), so a deep link paints
 * the right module instead of flinching through the overview.
 */
export default function DemoPage() {
  return (
    <Suspense fallback={<Loading />}>
      <DemoConsole
        data={{
          observatory: DEMO_OBSERVATORY,
          promotions: DEMO_PROMOTIONS,
          contradictions: DEMO_CONTRADICTIONS,
          counts: DEMO_COUNTS,
          disputes: DEMO_DISPUTES,
          cortex: DEMO_CORTEX,
          archive: DEMO_ARCHIVE,
          health: DEMO_HEALTH,
          divergences: DEMO_DIVERGENCES,
        }}
      />
    </Suspense>
  );
}
