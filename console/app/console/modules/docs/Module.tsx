import DemoBanner from "@/components/DemoBanner";
import DocWiki from "@/docs/DocWiki";
import { DEMO_DOCS } from "@/docs/docs-demo";
import { WIKI_PAGE_SIZE, demoWiki, parseWiki, type WikiData } from "@/docs/wiki-data";
import { configFromEnv, listDocs } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Pages",
};

type Params = Record<string, string | string[] | undefined>;

// Read surface: live when `brainiac serve` is reachable, the Meridian fixture
// org behind an unconditional DemoBanner when not. Never 500s.
//
// The wiki is now SERVER-driven, the URL its single source of truth. The whole
// corpus no longer crosses the wire: two bounded round trips do the work the
// browser used to do over 436 summaries.
//
//  1. The facet menu — GET /v1/docs?facets=1&limit=1. Cheap and always fetched:
//     it carries the space directory (the tree) and the tab counts for the whole
//     wiki, so the rail costs one node per SPACE, not one per page.
//  2. The page — GET /v1/docs?space=&needs_review=|stale=&q=&limit=25&offset=.
//     The active tab/space/search, ordered and windowed by the server. Never the
//     whole corpus; never a client `.slice`.
export default async function DocsPage({ searchParams }: { searchParams: Params }) {
  const { filter, page } = parseWiki(searchParams);
  const offset = page * WIKI_PAGE_SIZE;

  const { data, live } = await withDemoFallback<WikiData>(async () => {
    const cfg = configFromEnv();
    const [menu, pageOut] = await Promise.all([
      // Whole-wiki facet menu: the directory + stable tab counts.
      listDocs(cfg, { facets: true, limit: 1 }),
      // The active page.
      listDocs(cfg, {
        space: filter.space,
        q: filter.q,
        needsReview: filter.tab === "review" ? true : undefined,
        stale: filter.tab === "dirty" ? true : undefined,
        limit: WIKI_PAGE_SIZE,
        offset,
      }),
    ]);
    const f = menu.facets;
    return {
      live: true,
      documents: pageOut.documents,
      total: pageOut.total,
      spaces: f?.spaces ?? [],
      tabCounts: {
        all: menu.total,
        review: f?.needs_review ?? 0,
        dirty: f?.dirty ?? 0,
      },
      filter,
      page,
      pageSize: WIKI_PAGE_SIZE,
    } satisfies WikiData;
  }, demoWiki(DEMO_DOCS, filter, page, WIKI_PAGE_SIZE));

  return (
    <>
      {!live && <DemoBanner />}
      <DocWiki data={data} />
    </>
  );
}
