import DemoBanner from "@/components/DemoBanner";
import DocWiki from "@/docs/DocWiki";
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
//
// The whole visible corpus still crosses the wire in that one trip — /v1/docs is
// unpaginated, so this fetch is O(corpus) whatever the client does with it. What
// changed is that DocWiki no longer PAINTS all of it: the tree renders one node
// per space and one page of rows, and says so. Paging pushed into /v1/docs is the
// next move (it would also let the queue tab count without the corpus in hand);
// until then the summaries are small and the transfer is not what made this
// module unusable — 40,797px of DOM was.
export default async function DocsPage() {
  const { data, live } = await withDemoFallback<DocSummary[]>(
    () => listDocs(configFromEnv()),
    DEMO_DOCS,
  );
  return (
    <>
      {!live && <DemoBanner />}
      <DocWiki docs={data} />
    </>
  );
}
