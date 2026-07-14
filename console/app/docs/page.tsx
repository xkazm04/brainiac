import DemoBanner from "@/components/DemoBanner";
import DocIndex from "@/docs/DocIndex";
import { DEMO_DOCS } from "@/docs/docs-demo";
import { configFromEnv, listDocs } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { DocSummary } from "@/lib/types";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Pages",
};

// Read surface: live when `brainiac serve` is reachable, the Meridian fixture
// org behind an unconditional DemoBanner when not. Each row carries its own
// `pending_review` / `dirty` flags, so the index is one round trip.
export default async function DocsPage() {
  const { data, live } = await withDemoFallback<DocSummary[]>(
    () => listDocs(configFromEnv()),
    DEMO_DOCS,
  );
  return (
    <>
      {!live && <DemoBanner />}
      <DocIndex docs={data} />
    </>
  );
}
