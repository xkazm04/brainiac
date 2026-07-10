import Home, { type LiveStats } from "@/home/Home";
import { configFromEnv, getAnalytics, pendingPromotions } from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Console",
};

// The home hero runs on demo physics regardless; the stats strip goes live
// when the brainiac server is reachable, and degrades silently when not.
async function liveStats(): Promise<LiveStats | null> {
  try {
    const cfg = configFromEnv();
    const [analytics, pending] = await Promise.all([
      getAnalytics(cfg),
      pendingPromotions(cfg),
    ]);
    return {
      pendingPromotions: pending.length,
      openContradictions: analytics.reviews.open_contradictions,
      canonicalCount:
        analytics.memories_by_status.find((r) => r.status === "canonical")?.count ?? 0,
      embeddingModel: analytics.embedding_model,
    };
  } catch {
    return null;
  }
}

export default async function Page() {
  return <Home live={await liveStats()} />;
}
