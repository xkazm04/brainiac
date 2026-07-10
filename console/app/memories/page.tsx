import { configFromEnv, listMemories } from "@/lib/api";

import { DEMO_ARCHIVE, type ArchiveData } from "./archive-data";
import ArchiveLab from "./ArchiveLab";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Archive",
};

// One server-side fetch of the visible corpus (all statuses, validity
// windows included); as-of scrubbing and filtering run client-side over it.
async function archiveData(): Promise<ArchiveData> {
  try {
    const out = await listMemories(configFromEnv(), { limit: "200" });
    return { live: true, total: out.total, rows: out.memories };
  } catch {
    return DEMO_ARCHIVE;
  }
}

export default async function MemoriesPage() {
  return <ArchiveLab data={await archiveData()} />;
}
