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
