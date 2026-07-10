import { configFromEnv, listMemories } from "@/lib/api";

import Archive from "./Archive";
import { DEMO_ARCHIVE, type ArchiveData } from "./archive-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Archive",
};

// One server-side fetch of the visible corpus (all statuses, validity
// windows included); as-of scrubbing runs client-side over it. Demo corpus
// (labeled) when the server is down.
async function archiveData(): Promise<ArchiveData> {
  try {
    const out = await listMemories(configFromEnv(), { limit: "200" });
    return { live: true, total: out.total, rows: out.memories };
  } catch {
    return DEMO_ARCHIVE;
  }
}

export default async function MemoriesPage() {
  return <Archive data={await archiveData()} />;
}
