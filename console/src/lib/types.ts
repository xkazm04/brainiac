// Typed mirror of the brainiac REST payloads (crates/brainiac-server).
// Hand-written for the small v0 surface; switch to utoipa-generated types
// once the API stabilizes.

export interface SearchHit {
  id: string;
  content: string;
  kind: string;
  status: string;
  score: number;
  via_graph: boolean;
  provenance_id: string | null;
}

export interface PendingPromotion {
  id: string;
  memory_id: string;
  to_status: string;
  policy_rule: string | null;
}

export interface ReviewedPromotion {
  promotion_id: string;
  memory_id: string;
  decision: "approved" | "denied";
  memory_status: string;
}

export interface ContradictionMemory {
  id: string;
  content: string | null;
}

export interface Contradiction {
  id: string;
  memory_a: ContradictionMemory;
  memory_b: ContradictionMemory;
  detected_by: string;
  suggested_resolution: string | null;
}

export type ContradictionResolution = "supersede" | "coexist" | "dismiss";

export interface GraphCanonical {
  id: string;
  name: string;
  kind: string;
}

export interface GraphEntity {
  id: string;
  name: string;
  kind: string;
  team_id: string;
  canonical_id: string | null;
}

export interface GraphEdge {
  src: string;
  dst: string;
  relation: string;
  memory_id: string | null;
  evidence: string | null;
}

export interface Graph {
  canonicals: GraphCanonical[];
  entities: GraphEntity[];
  edges: GraphEdge[];
}

export interface SourceFeedItem {
  id: string;
  kind: string;
  external_ref: string | null;
  created_at: string;
  team: string | null;
  status: "queued" | "retrying" | "processed" | "failed" | "unknown";
  attempts: number | null;
  memories: number;
  promoted: number;
  pending_review: number;
}

export interface PipelineRun {
  id: string;
  stage: string;
  status: string;
  detail: string | null;
  started_at: string;
  duration_secs: number;
}

export interface QueueHealth {
  queue: string;
  ready: number;
  in_flight: number;
  oldest_ready_secs: number;
  attempts_histogram: { attempts: number; count: number }[];
  archived: { ok: number; failed: number };
  dead_letters: number;
}

export interface MemoryRow {
  id: string;
  content: string;
  kind: string;
  status: string;
  visibility: string;
  team: string;
  team_id: string;
  valid_from: string | null;
  valid_to: string | null;
  superseded_by: string | null;
  created_at: string | null;
  confidence: number | null;
}

export interface MemoriesList {
  total: number;
  memories: MemoryRow[];
}

export interface ChainLink {
  id: string;
  content: string;
  status: string;
  valid_from: string | null;
  valid_to: string | null;
  depth: number;
}

export interface MemoryDetail {
  memory: MemoryRow;
  provenance: {
    actor_kind: string;
    actor_id: string;
    model_ref: string | null;
    source_kind: string | null;
    source_ref: string | null;
  } | null;
  entities: { name: string; kind: string; team: string }[];
  promotions: {
    from_status: string;
    to_status: string;
    policy_decision: string;
    policy_rule: string | null;
    reviewed_at: string | null;
    created_at: string | null;
  }[];
  chain: { predecessors: ChainLink[]; successors: ChainLink[] };
}

export interface GraphOverview {
  teams: { id: string; name: string; memories: number; entities: number }[];
  canonicals: {
    id: string;
    name: string;
    kind: string;
    memories: number;
    teams: number;
    team_ids: string[];
  }[];
  team_links: { a: string; b: string; shared: number }[];
}

export interface CanonicalDetail {
  canonical: { id: string; name: string; kind: string; summary: string | null };
  surface_forms: {
    entity_id: string;
    name: string;
    kind: string;
    team_id: string;
    team: string;
    confidence: number | null;
    method: string | null;
  }[];
  edges: {
    src: string;
    src_name: string;
    dst: string;
    dst_name: string;
    relation: string;
    memory_id: string | null;
    evidence: string | null;
  }[];
  neighbors: { id: string; name: string; kind: string; shared_edges: number }[];
  memories: { id: string; content: string; kind: string; status: string; team: string }[];
}

export interface ObservatoryPayload {
  totals: { status: string; count: number }[];
  weekly: {
    captured: { week: string; count: number }[];
    promoted: { week: string; count: number }[];
  };
  by_kind: { kind: string; team: string; count: number }[];
  top_entities: { name: string; kind: string; memories: number; teams: number }[];
  review: {
    pending: number;
    oldest_pending_secs: number;
    reviewed: number;
    avg_latency_secs: number;
    auto_promoted: number;
  };
  contradictions: { status: string; count: number }[];
  queue: { ingest_depth: number };
  embedding_model: string;
}

export interface Analytics {
  memories_by_status: { status: string; count: number }[];
  reviews: {
    pending_promotions: number;
    oldest_pending_secs: number;
    open_contradictions: number;
  };
  graph: { entities: number; canonicals: number };
  queue: { ingest_depth: number };
  embedding_model: string;
}
