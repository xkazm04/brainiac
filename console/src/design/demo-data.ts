// Shared demo substrate for the design-lab variants. Numbers are real where
// we have them (the 2026-07-10 eval baselines); the rest is plausible
// Meridian-shaped fiction so every variant renders identical content and the
// comparison is purely about art direction.

export const INGESTION_WEEKS = [
  { week: "W23", captured: 34, promoted: 9 },
  { week: "W24", captured: 51, promoted: 14 },
  { week: "W25", captured: 47, promoted: 18 },
  { week: "W26", captured: 78, promoted: 24 },
  { week: "W27", captured: 92, promoted: 31 },
  { week: "W28", captured: 121, promoted: 45 },
] as const;

// Real numbers: text-embedding-v4 vs deterministic baseline, NDCG@10.
export const STRATA = [
  { name: "exact id", qwen: 0.965, baseline: 0.927 },
  { name: "cross-team", qwen: 0.926, baseline: 0.772 },
  { name: "temporal", qwen: 0.905, baseline: 0.938 },
  { name: "semantic", qwen: 0.811, baseline: 0.422 },
  { name: "czech", qwen: 0.785, baseline: 0.56 },
] as const;

export const KPIS = [
  { label: "canonical memories", value: "81", delta: "+40 this week" },
  { label: "retrieval NDCG@10", value: "0.876", delta: "+0.191 vs baseline" },
  { label: "RLS leaks", value: "0", delta: "hard gate" },
  { label: "median review", value: "3.2h", delta: "SLO < 48h" },
] as const;

export const QUEUE = [
  {
    id: "p1",
    kind: "pitfall",
    team: "payments",
    content:
      "decline code 05 spikes are issuer-side; retrying burns PSP quota and reads as fraud velocity",
    rule: "pitfall.high_confidence",
    age: "26m",
  },
  {
    id: "p2",
    kind: "decision",
    team: "platform",
    content:
      "partition counts on payment topics are fixed at 24; changing them requires a data-team sign-off",
    rule: "decision.cross_team",
    age: "1.4h",
  },
  {
    id: "p3",
    kind: "howto",
    team: "data",
    content:
      "validate monetary features after any dbt change by running the amounts sanity suite",
    rule: "howto.default_review",
    age: "3h",
  },
] as const;

export const CONTRADICTION = {
  a: "psp-gateway client timeout is 10 seconds",
  b: "psp-gateway client timeout raised to 30 seconds after the PSP incident review",
  suggestion: "supersede — B wins (incident review, newer valid_from)",
} as const;

// Canonical entity + its team-scoped surface forms (the collision-tolerance
// demo that sells the graph).
export const CANONICAL_DEMO = {
  name: "kafka",
  aliases: [
    { team: "payments", name: "Kafka" },
    { team: "platform", name: "MSK cluster" },
    { team: "data", name: "the event bus" },
  ],
} as const;

export const PIPELINE_STAGES = [
  "capture",
  "extract",
  "resolve",
  "contradict",
  "promote",
  "distribute",
] as const;
