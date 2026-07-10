// Shared substrate for the Cortex Map variants: deterministic layout
// helpers and demo fallbacks so every variant renders identically with or
// without a live server. (The client drill-down hook lives in
// useCanonicalDetail.ts — this module stays server-importable.)

import type { CanonicalDetail, GraphOverview } from "@/lib/types";

export interface CortexData {
  live: boolean;
  overview: GraphOverview;
}

/** Stable team palette (index by sorted team order): alpha/delta/beta hues. */
export const TEAM_HUES = [190, 262, 158] as const;
export const teamColor = (i: number, l = 68, a = 1) =>
  `hsla(${TEAM_HUES[i % TEAM_HUES.length]}, 85%, ${l}%, ${a})`;

/** FNV-ish string hash → stable 0..1 (deterministic layouts, no Math.random). */
export function hash01(s: string, salt = 0): number {
  let h = 2166136261 ^ salt;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return ((h >>> 0) % 10000) / 10000;
}

// ── demo shapes ─────────────────────────────────────────────────────────

const T = {
  payments: "t-payments",
  platform: "t-platform",
  data: "t-data",
} as const;

export const DEMO_CORTEX: CortexData = {
  live: false,
  overview: {
    teams: [
      { id: T.data, name: "data", memories: 18, entities: 13 },
      { id: T.payments, name: "payments", memories: 31, entities: 15 },
      { id: T.platform, name: "platform", memories: 26, entities: 14 },
    ],
    canonicals: [
      { id: "c-kafka", name: "kafka", kind: "tech", memories: 14, teams: 3, team_ids: [T.payments, T.platform, T.data] },
      { id: "c-psp", name: "psp-gateway", kind: "service", memories: 11, teams: 2, team_ids: [T.payments, T.platform] },
      { id: "c-checkout", name: "checkout-feature", kind: "feature", memories: 9, teams: 3, team_ids: [T.payments, T.platform, T.data] },
      { id: "c-argocd", name: "argocd", kind: "tech", memories: 8, teams: 2, team_ids: [T.platform, T.payments] },
      { id: "c-refund", name: "refund-worker", kind: "service", memories: 7, teams: 2, team_ids: [T.payments, T.platform] },
      { id: "c-fraud", name: "fraud scoring", kind: "concept", memories: 6, teams: 2, team_ids: [T.data, T.payments] },
      { id: "c-lake", name: "event-lake", kind: "repo", memories: 6, teams: 1, team_ids: [T.data] },
      { id: "c-opa", name: "opa", kind: "tech", memories: 5, teams: 2, team_ids: [T.platform, T.data] },
      { id: "c-retry", name: "std-retry policy", kind: "concept", memories: 5, teams: 2, team_ids: [T.platform, T.payments] },
      { id: "c-ledger", name: "ledger-service", kind: "service", memories: 4, teams: 2, team_ids: [T.payments, T.data] },
      { id: "c-feast", name: "feast", kind: "service", memories: 3, teams: 1, team_ids: [T.data] },
      { id: "c-grafana", name: "grafana", kind: "tech", memories: 3, teams: 1, team_ids: [T.platform] },
    ],
    team_links: [
      { a: T.payments, b: T.platform, shared: 6 },
      { a: T.payments, b: T.data, shared: 4 },
      { a: T.platform, b: T.data, shared: 3 },
    ],
  },
};

export function demoDetail(id: string, overview: GraphOverview): CanonicalDetail {
  const c = overview.canonicals.find((x) => x.id === id) ?? overview.canonicals[0];
  const teamName = (tid: string) => overview.teams.find((t) => t.id === tid)?.name ?? "team";
  const dialects: Record<string, string[]> = {
    kafka: ["Kafka", "MSK cluster", "the event bus"],
    "psp-gateway": ["psp-gateway", "psp gateway egress"],
    "checkout-feature": ["checkout v2", "payments API", "checkout funnel"],
  };
  const names = dialects[c.name] ?? c.team_ids.map((tid, i) => (i === 0 ? c.name : `the ${c.name}`));
  return {
    canonical: { id: c.id, name: c.name, kind: c.kind, summary: null },
    surface_forms: c.team_ids.map((tid, i) => ({
      entity_id: `${c.id}-e${i}`,
      name: names[i] ?? c.name,
      kind: c.kind,
      team_id: tid,
      team: teamName(tid),
      confidence: i === 0 ? 1 : 0.9,
      method: i === 0 ? "human" : "llm_adjudicated",
    })),
    edges: [
      {
        src: `${c.id}-e0`,
        src_name: names[0] ?? c.name,
        dst: "e-other",
        dst_name: "payment-service",
        relation: "depends_on",
        memory_id: "m-demo",
        evidence: `payment-service consumes ${c.name} events from checkout.events.v2`,
      },
    ],
    neighbors: overview.canonicals
      .filter((n) => n.id !== c.id)
      .slice(0, 4)
      .map((n) => ({ id: n.id, name: n.name, kind: n.kind, shared_edges: 1 + (n.memories % 3) })),
    memories: [
      {
        id: "m1",
        content: `use of ${c.name} is governed org-wide; the canonical node binds ${c.teams} team dialect${c.teams > 1 ? "s" : ""}`,
        kind: "fact",
        status: "canonical",
        team: teamName(c.team_ids[0]),
      },
      {
        id: "m2",
        content: `pitfall: never bypass ${c.name} conventions during incidents — the runbook lives with the owning team`,
        kind: "pitfall",
        status: "canonical",
        team: teamName(c.team_ids[c.team_ids.length - 1]),
      },
    ],
  };
}
