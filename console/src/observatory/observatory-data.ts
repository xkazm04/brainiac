// Normalized shape every Observatory variant consumes, plus the demo
// fallback used when the brainiac server is down or the corpus is too young
// for a meaningful trend (fresh fixtures land in a single ISO week).

import { INGESTION_WEEKS } from "@/design/demo-data";
import type { ObservatoryPayload } from "@/lib/types";

export interface WeekPoint {
  week: string;
  captured: number;
  promoted: number;
}

export interface ObservatoryData {
  live: boolean;
  /** True when the weekly trend is demo-shaped (corpus younger than 3 weeks). */
  weeklyIsDemo: boolean;
  totals: Record<string, number>;
  weekly: WeekPoint[];
  byKind: { kind: string; team: string; count: number }[];
  /** The axis-swap twin: kind×project, with "org-shared" as its own column
   *  (PROJECT-PLAN PR3). */
  byProject: { kind: string; project: string; count: number }[];
  topEntities: { name: string; kind: string; memories: number; teams: number }[];
  review: {
    pending: number;
    oldestSecs: number;
    reviewed: number;
    avgLatencySecs: number;
    autoPromoted: number;
  };
  contradictions: Record<string, number>;
  queueDepth: number;
  embeddingModel: string;
}

const DEMO_WEEKLY: WeekPoint[] = INGESTION_WEEKS.map((w) => ({ ...w }));

export function normalizeObservatory(p: ObservatoryPayload): ObservatoryData {
  const weeks = new Map<string, WeekPoint>();
  for (const c of p.weekly.captured) {
    weeks.set(c.week, { week: c.week, captured: c.count, promoted: 0 });
  }
  for (const pr of p.weekly.promoted) {
    const w = weeks.get(pr.week) ?? { week: pr.week, captured: 0, promoted: 0 };
    w.promoted = pr.count;
    weeks.set(pr.week, w);
  }
  const weekly = [...weeks.values()].sort((a, b) => a.week.localeCompare(b.week));
  const weeklyIsDemo = weekly.length < 3;
  return {
    live: true,
    weeklyIsDemo,
    totals: Object.fromEntries(p.totals.map((t) => [t.status, t.count])),
    weekly: weeklyIsDemo ? DEMO_WEEKLY : weekly,
    byKind: p.by_kind,
    byProject: p.by_project,
    topEntities: p.top_entities,
    review: {
      pending: p.review.pending,
      oldestSecs: p.review.oldest_pending_secs,
      reviewed: p.review.reviewed,
      avgLatencySecs: p.review.avg_latency_secs,
      autoPromoted: p.review.auto_promoted,
    },
    contradictions: Object.fromEntries(p.contradictions.map((c) => [c.status, c.count])),
    queueDepth: p.queue.ingest_depth,
    embeddingModel: p.embedding_model,
  };
}

export const DEMO_OBSERVATORY: ObservatoryData = {
  live: false,
  weeklyIsDemo: true,
  totals: { canonical: 81, candidate: 7, raw: 12, deprecated: 6, rejected: 3 },
  weekly: DEMO_WEEKLY,
  byKind: [
    { kind: "fact", team: "payments", count: 14 },
    { kind: "fact", team: "platform", count: 9 },
    { kind: "fact", team: "data", count: 8 },
    { kind: "decision", team: "payments", count: 6 },
    { kind: "decision", team: "platform", count: 4 },
    { kind: "decision", team: "data", count: 5 },
    { kind: "pitfall", team: "payments", count: 5 },
    { kind: "pitfall", team: "platform", count: 3 },
    { kind: "pitfall", team: "data", count: 2 },
    { kind: "howto", team: "payments", count: 4 },
    { kind: "howto", team: "platform", count: 4 },
    { kind: "howto", team: "data", count: 3 },
  ],
  // Application-shaped, deliberately not the team names — and org-shared is
  // its own column, not a gap (PR3).
  byProject: [
    { kind: "fact", project: "payments-api", count: 12 },
    { kind: "fact", project: "checkout-web", count: 8 },
    { kind: "fact", project: "feature-store", count: 6 },
    { kind: "fact", project: "org-shared", count: 5 },
    { kind: "decision", project: "payments-api", count: 5 },
    { kind: "decision", project: "checkout-web", count: 4 },
    { kind: "decision", project: "org-shared", count: 6 },
    { kind: "pitfall", project: "payments-api", count: 4 },
    { kind: "pitfall", project: "feature-store", count: 3 },
    { kind: "pitfall", project: "org-shared", count: 3 },
    { kind: "howto", project: "payments-api", count: 3 },
    { kind: "howto", project: "feature-store", count: 3 },
    { kind: "howto", project: "org-shared", count: 5 },
  ],
  topEntities: [
    { name: "kafka", kind: "tech", memories: 14, teams: 3 },
    { name: "psp-gateway", kind: "service", memories: 11, teams: 2 },
    { name: "checkout-feature", kind: "feature", memories: 9, teams: 3 },
    { name: "argocd", kind: "tech", memories: 8, teams: 2 },
    { name: "refund-worker", kind: "service", memories: 7, teams: 2 },
    { name: "fraud scoring", kind: "concept", memories: 6, teams: 2 },
    { name: "event-lake", kind: "repo", memories: 6, teams: 1 },
    { name: "opa", kind: "tech", memories: 5, teams: 2 },
  ],
  review: { pending: 3, oldestSecs: 11520, reviewed: 24, avgLatencySecs: 11520, autoPromoted: 9 },
  contradictions: { open: 1, resolved_supersede: 4, resolved_coexist: 2, dismissed: 3 },
  queueDepth: 0,
  embeddingModel: "qwen:text-embedding-v4",
};
