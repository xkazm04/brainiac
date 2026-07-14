import type { Metadata } from "next";

import Archive from "../../console/(modules)/memories/Archive";
import { DEMO_ARCHIVE } from "../../console/(modules)/memories/archive-data";

export const metadata: Metadata = { title: "Brainiac — demo · archive" };

// DEMO_ARCHIVE carries live:false, so the memory drill-in renders from the
// fixture rather than calling the gated /api/memories route. The as-of scrubber
// works: it runs client-side over the corpus already in the page.
export default function DemoMemoriesPage() {
  return <Archive data={DEMO_ARCHIVE} />;
}
