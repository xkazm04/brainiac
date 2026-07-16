import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, listMemories, type ApiConfig } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import type { MemoryRow } from "@/lib/types";

import Archive from "./Archive";
import { DEMO_ARCHIVE, type ArchiveData } from "./archive-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Archive",
};

/**
 * The visible corpus, fetched server-side and scrubbed client-side.
 *
 * This used to be one `limit: "200"` call, which was a lie with a number on it:
 * the handler clamps limit to 1..200, so at 660 memories the archive dropped
 * 70% of the org on the floor and then rendered confident counts over what was
 * left — "88 canonical" meaning "88 of the first 200". No client-side view can
 * fix that; the rows are simply not there. So it pages.
 */

/** The server's own ceiling (`limit.clamp(1, 200)` in console.rs) — asking for
 *  more just gets 200 back, which would read as a short page and stop early. */
const PAGE = 200;

/**
 * The safety cap, in rows. A very large org should not stall this server
 * component behind 50 sequential fetches, and the browser should not be handed
 * a corpus it cannot filter on a keystroke.
 *
 * When it bites, `capped` is set and the Archive says so ON SCREEN. That is the
 * entire difference between this and the bug it replaces: a cap is honest
 * engineering, silence about one is not.
 */
const MAX_ROWS = 5000;

async function fetchCorpus(cfg: ApiConfig): Promise<ArchiveData> {
  const rows: MemoryRow[] = [];
  let total = 0;
  for (let offset = 0; offset < MAX_ROWS; offset += PAGE) {
    const page = await listMemories(cfg, {
      limit: String(Math.min(PAGE, MAX_ROWS - offset)),
      offset: String(offset),
    });
    total = page.total;
    rows.push(...page.memories);
    // Three independent stops, because a paging loop that trusts only one of
    // them is a hang waiting for a server that disagrees: a short page, the
    // total reached, and the `offset < MAX_ROWS` bound above.
    if (page.memories.length < PAGE || rows.length >= total) break;
  }
  return { live: true, total, rows, capped: rows.length < total };
}

export default async function MemoriesPage() {
  const { data, live } = await withDemoFallback<ArchiveData>(
    () => fetchCorpus(configFromEnv()),
    DEMO_ARCHIVE,
  );
  return (
    <>
      {!live && <DemoBanner />}
      <Archive data={data} />
    </>
  );
}
