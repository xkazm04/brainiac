import Home, { type LiveStats } from "@/home/Home";
import {
  configFromEnv,
  getAnalytics,
  getGraphOverview,
  getKnowledgeHealth,
  getPracticeDivergence,
  listDocs,
  pendingPromotions,
} from "@/lib/api";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Console",
};

// The operator home. Real org data — reachable only behind the console gate
// (see middleware.ts). The public root ("/") is the pitch, and it never calls
// the API at all.
//
// The hero runs on demo physics regardless; the stats strip goes live when the
// brainiac server is reachable, and degrades silently when not.
async function liveStats(): Promise<LiveStats | null> {
  try {
    const cfg = configFromEnv();
    // The core three decide live-vs-demo (any failure → null → demo field).
    // The second-movement three degrade individually: a health/divergence/docs
    // hiccup costs that one station its live artifact, never the whole page.
    const [analytics, pending, overview, health, divergences, docs] = await Promise.all([
      getAnalytics(cfg),
      pendingPromotions(cfg),
      getGraphOverview(cfg),
      getKnowledgeHealth(cfg).catch(() => null),
      getPracticeDivergence(cfg).catch(() => null),
      listDocs(cfg).catch(() => null),
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
      health: health
        ? {
            score: health.score,
            grade: health.grade,
            crossTeamContradictions: health.signals.cross_team_contradictions,
          }
        : null,
      divergence: divergences
        ? {
            count: divergences.divergences.length,
            // The server orders by impact, so [0] is the headline finding.
            top: divergences.divergences[0]
              ? {
                  practice: divergences.divergences[0].practice,
                  impact: divergences.divergences[0].impact,
                }
              : null,
          }
        : null,
      docsPages: docs ? docs.length : null,
    };
  } catch {
    return null;
  }
}

export default async function ConsoleHomePage() {
  return <Home live={await liveStats()} />;
}
