import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, listMemories } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

import Archive from "./Archive";
import { DEMO_ARCHIVE, type ArchiveData } from "./archive-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Archive",
};

// One server-side fetch of the visible corpus (all statuses, validity
// windows included); as-of scrubbing runs client-side over it. Demo corpus
// (behind an unconditional DemoBanner) when the server is down.
export default async function MemoriesPage() {
  const { data, live } = await withDemoFallback<ArchiveData>(async () => {
    const out = await listMemories(configFromEnv(), { limit: "200" });
    return { live: true, total: out.total, rows: out.memories };
  }, DEMO_ARCHIVE);
  return (
    <>
      {!live && <DemoBanner />}
      <Archive data={data} />
    </>
  );
}
