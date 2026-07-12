// Shared substrate for the Ingest Monitor variants: the six-stage model,
// stage inference from a source's rollup, and the demo feed.

import type { PipelineRun, QueueHealth, SourceFeedItem } from "@/lib/types";

export interface IngestData {
  live: boolean;
  sources: SourceFeedItem[];
  runs: PipelineRun[];
  health: QueueHealth;
}

export const STAGES = [
  "capture",
  "extract",
  "resolve",
  "contradict",
  "promote",
  "distribute",
] as const;

/**
 * Where a source currently sits on the conduction path, inferred from its
 * rollup. 0-based stage index + whether it's stuck.
 */
export function stageOf(s: SourceFeedItem): { stage: number; stuck: boolean } {
  if (s.status === "queued" || s.status === "retrying") {
    return { stage: 1, stuck: s.status === "retrying" };
  }
  if (s.status === "failed") return { stage: 1, stuck: true };
  if (s.memories === 0) return { stage: 2, stuck: false }; // processed, nothing extracted
  if (s.promoted > 0) return { stage: 5, stuck: false };
  if (s.pending_review > 0) return { stage: 4, stuck: false };
  return { stage: 3, stuck: false };
}

export function ageLabel(iso: string): string {
  const secs = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (secs < 90) return `${Math.round(secs)}s`;
  if (secs < 5400) return `${Math.round(secs / 60)}m`;
  if (secs < 129600) return `${Math.round(secs / 3600)}h`;
  return `${Math.round(secs / 86400)}d`;
}

/** Tiny stable hash → 0..1 (deterministic mock layouts, no Math.random). */
export function h01(s: string, salt = 0): number {
  let h = 2166136261 ^ salt;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return ((h >>> 0) % 10000) / 10000;
}

// ── demo feed ───────────────────────────────────────────────────────────

const src = (
  id: string,
  minsAgo: number,
  status: SourceFeedItem["status"],
  overrides: Partial<SourceFeedItem> = {},
): SourceFeedItem => ({
  id: `ds-${id}`,
  kind: "session_transcript",
  external_ref: null,
  created_at: new Date(Date.now() - minsAgo * 60000).toISOString(),
  team: "payments",
  status,
  attempts: status === "retrying" ? 2 : 0,
  memories: 0,
  promoted: 0,
  pending_review: 0,
  ...overrides,
});

// ── 40-source stress mock (7 teams, realistic status mix) ───────────────
// Deterministic apart from the "now" anchor, so layouts are stable within
// a session. Used by the lab's scale toggle to validate operator UX under
// load: ~15% queued/retrying, ~8% failed, the rest processed at varying
// yields.

const LOAD_TEAMS = ["payments", "platform", "data", "mobile", "security", "ml", "support"] as const;
const LOAD_KINDS = ["session_transcript", "manual", "repo", "doc"] as const;
const LOAD_SLUGS = [
  "incident-retro", "design-review", "oncall-handoff", "arch-sync", "postmortem",
  "pairing-session", "migration-notes", "spike-findings", "runbook-update", "qa-triage",
] as const;

export function makeLargeIngest(): IngestData {
  const now = Date.now();
  const sources: SourceFeedItem[] = Array.from({ length: 40 }, (_, i) => {
    const key = `load-${i}`;
    const team = LOAD_TEAMS[Math.floor(h01(key, 3) * LOAD_TEAMS.length)];
    const kind = LOAD_KINDS[Math.floor(h01(key, 5) * LOAD_KINDS.length)];
    const slug = `${team}-${LOAD_SLUGS[Math.floor(h01(key, 7) * LOAD_SLUGS.length)]}-${String(100 + i)}`;
    // age: log-spread over 48h so the conduction axis fills edge to edge
    const ageMin = Math.pow(10, h01(key, 11) * Math.log10(2880));
    const roll = h01(key, 13);
    let status: SourceFeedItem["status"];
    if (roll < 0.1) status = "queued";
    else if (roll < 0.17) status = "retrying";
    else if (roll < 0.25) status = "failed";
    else status = "processed";
    const memories =
      status === "processed" ? (h01(key, 17) < 0.15 ? 0 : 1 + Math.floor(h01(key, 19) * 5)) : 0;
    const promoted = memories > 0 && h01(key, 23) < 0.6 ? Math.ceil(memories * h01(key, 29)) : 0;
    const pending =
      memories - promoted > 0 && h01(key, 31) < 0.5 ? Math.min(memories - promoted, 2) : 0;
    return {
      id: `dl-${key}`,
      kind,
      external_ref: kind === "manual" ? null : slug,
      created_at: new Date(now - ageMin * 60000).toISOString(),
      team,
      status,
      attempts: status === "retrying" ? 1 + Math.floor(h01(key, 37) * 4) : status === "failed" ? 5 : 0,
      memories,
      promoted,
      pending_review: pending,
    };
  });

  const runs: PipelineRun[] = Array.from({ length: 18 }, (_, i) => {
    const key = `run-${i}`;
    const stage = STAGES[1 + Math.floor(h01(key, 3) * 4)];
    const failed = h01(key, 5) < 0.15;
    return {
      id: `dr-${key}`,
      stage,
      status: failed ? "failed" : "ok",
      detail: failed
        ? "validation firewall: provider returned malformed JSON"
        : `${1 + Math.floor(h01(key, 7) * 4)} items · ${stage}`,
      started_at: new Date(now - h01(key, 11) * 2880 * 60000).toISOString(),
      duration_secs: 1 + Math.floor(h01(key, 13) * 40),
    };
  });

  return {
    live: false,
    sources,
    runs,
    health: {
      queue: "ingest",
      ready: 6,
      in_flight: 2,
      oldest_ready_secs: 214,
      attempts_histogram: [
        { attempts: 0, count: 4 },
        { attempts: 1, count: 2 },
        { attempts: 3, count: 2 },
      ],
      archived: { ok: 29, failed: 3 },
      dead_letters: 3,
    },
  };
}

export const DEMO_INGEST: IngestData = {
  live: false,
  sources: [
    src("live-1", 1, "queued", { kind: "manual", team: "payments" }),
    src("s1", 8, "processed", { memories: 3, promoted: 2, team: "payments", external_ref: "pay-incident-011" }),
    src("s2", 22, "processed", { memories: 2, pending_review: 2, team: "platform", external_ref: "plat-policy-009" }),
    src("s3", 47, "retrying", { team: "data", external_ref: "data-backfill-021" }),
    src("s4", 90, "processed", { memories: 3, promoted: 3, team: "data", external_ref: "data-fraud-017" }),
    src("s5", 130, "failed", { team: "platform", external_ref: "plat-incident-004", attempts: 5 }),
    src("s6", 200, "processed", { memories: 2, promoted: 1, pending_review: 1, team: "payments", external_ref: "pay-retro-015" }),
    src("s7", 320, "processed", { memories: 0, team: "support", kind: "manual" }),
  ],
  runs: [
    { id: "r1", stage: "extract", status: "ok", detail: "3 memories from pay-incident-011", started_at: new Date(Date.now() - 8 * 60000).toISOString(), duration_secs: 12 },
    { id: "r2", stage: "resolve", status: "ok", detail: "2 linked, 1 review", started_at: new Date(Date.now() - 8 * 60000).toISOString(), duration_secs: 4 },
    { id: "r3", stage: "contradict", status: "ok", detail: "1 opened (psp timeout)", started_at: new Date(Date.now() - 7 * 60000).toISOString(), duration_secs: 6 },
    { id: "r4", stage: "promote", status: "ok", detail: "2 auto, 1 review", started_at: new Date(Date.now() - 7 * 60000).toISOString(), duration_secs: 1 },
    { id: "r5", stage: "extract", status: "failed", detail: "validation firewall: bad JSON from provider", started_at: new Date(Date.now() - 130 * 60000).toISOString(), duration_secs: 31 },
  ],
  health: {
    queue: "ingest",
    ready: 2,
    in_flight: 1,
    oldest_ready_secs: 74,
    attempts_histogram: [
      { attempts: 0, count: 2 },
      { attempts: 2, count: 1 },
    ],
    archived: { ok: 6, failed: 1 },
    dead_letters: 1,
  },
};
