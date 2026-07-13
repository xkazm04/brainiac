import Home, { type LiveStats } from "@/home/Home";
import {
  configFromEnv,
  getAnalytics,
  getGraphOverview,
  pendingPromotions,
} from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Console",
};

// The home hero runs on demo physics regardless; the stats strip goes live
// when the brainiac server is reachable, and degrades silently when not.
async function liveStats(): Promise<LiveStats | null> {
  try {
    const cfg = configFromEnv();
    const [analytics, pending, overview] = await Promise.all([
      getAnalytics(cfg),
      pendingPromotions(cfg),
      getGraphOverview(cfg),
    ]);
    const teams = overview.teams
      .map((t) => ({ name: t.name, memories: t.memories }))
      .sort((a, b) => b.memories - a.memories);
    // Most-bound canonical (widest team span, then largest) — the strongest
    // real example of "constructive" binding for the third story station.
    const top = [...overview.canonicals].sort(
      (a, b) => b.teams - a.teams || b.memories - a.memories,
    )[0];
    return {
      pendingPromotions: pending.length,
      openContradictions: analytics.reviews.open_contradictions,
      canonicalCount:
        analytics.memories_by_status.find((r) => r.status === "canonical")?.count ?? 0,
      embeddingModel: analytics.embedding_model,
      teams,
      topCanonical: top ? { name: top.name, teams: top.teams } : null,
      totalMemories: teams.reduce((sum, t) => sum + t.memories, 0),
    };
  } catch {
    return null;
  }
}

export default async function Page() {
  return <Home live={await liveStats()} />;
}
