# Moonshot scan — graph-visualization

> Total: 2
> Files read: 14

## 1. The graph becomes an answering machine: agent-callable impact, path, and ownership queries

- **Severity**: Critical
- **Category**: agent-graph-interface
- **File**: `crates/brainiac-server/src/mcp.rs:2034` (`entity_lookup` — the *entire* agent-facing
  graph surface), plus `crates/brainiac-server/src/mcp.rs:430` (`tool_definitions`),
  `crates/brainiac-server/src/mcp.rs:674` (dispatch), `crates/brainiac-store/src/entities.rs:197`
  (`neighbors`), `crates/brainiac-server/src/console.rs:3317` (`graph_canonical`),
  `console/app/console/modules/graph/views/StarChartView.tsx:104` (constellation lines),
  `crates/brainiac-core/src/types.rs:246` (`EdgeRelation`).
- **Scenario**: An agent is told "move refunds off the PSP gateway." Before writing a line it asks
  `graph_impact { entity: "psp-gateway", relation: "depends_on", hops: 2 }` and gets back a blast
  radius: `refund-worker`, `ledger-service`, `checkout-feature` — each with the relation chain that
  reached it and a `memory:<uuid>` evidence id per hop. It asks
  `graph_path { from: "checkout-feature", to: "event-lake" }` and gets the shortest *explained*
  route (`checkout-feature --uses--> kafka --owns--> event-lake`), every hop citable. It asks
  `graph_owners { entity: "kafka" }` and learns three teams speak three dialects of the same thing,
  so it knows whose pitfall memories to load. Then a human opens the Cortex Map, clicks the same
  star, and sees the *same* traversal drawn as a directed, relation-labelled constellation — the
  picture and the agent's answer are one query with two renderers.
- **Root cause**: The graph is a *retrieval feature* and a *picture*, and nothing else. Agents get
  exactly one node-shaped tool: `entity_lookup` resolves a name → one canonical, its alias set, and
  up to 8 anchored memories (`mcp.rs:2034-2092`). It cannot traverse, cannot see a single edge, and
  cannot name a relation — even though `edges.relation` is a hardened 5-value typed vocabulary
  (`EdgeRelation`, `types.rs:246`, enforced by the extraction firewall at `extract.rs:777`). The one
  traversal primitive that exists, `entities::neighbors` (`entities.rs:197`), is *internal*: stage 4
  of retrieval calls it with `GRAPH_HOPS = 2` (`retrieval.rs:300`) and then **throws the structure
  away**, keeping only the extra memory rows. It doesn't even distinguish a `depends_on` from a
  `relates_to`, and the recursive CTE unions same-canonical siblings with edges in both directions,
  so direction — the thing blast radius is made of — is erased by design. The console has the same
  ceiling: `graph_canonical` returns `neighbors` ranked by a relation-blind `shared_edges` count
  (`console.rs:3364-3376`), which `StarChartView` draws as undirected dashed lines
  (`StarChartView.tsx:104-124`). Result: the product's *structural* knowledge is the only knowledge
  an agent cannot ask a structural question about. `memory_search` can find text about kafka; nothing
  can answer "what breaks if kafka changes."
- **Impact**: This is the difference between a memory server and an org's architectural oracle. Every
  other tool in the MCP surface answers "what did someone write down?"; this answers "what is true
  about the shape of our systems?" — the question that actually gates a change. Concretely: three new
  tools turn N canonical hubs into O(N²) answerable questions, and they are answers that today require
  a human who has been at the company three years. Blast-radius-before-edit is the single highest-value
  thing an org can hand a coding agent: it converts Brainiac from "context that improves the diff" to
  "the check that prevents the outage," which is a different budget line. It also makes graph quality
  *measurable and self-improving* — every `graph_impact` call is a labelled query against a subgraph,
  so `meridian` gains a structural profile (does the graph recover the known dependency set?) alongside
  its retrieval profiles, and `memory_feedback`-style verdicts on a returned path become the training
  signal that tells maintainers which edges are wrong. Today no one can be wrong about the graph
  because no one can ask it anything.
- **Fix sketch**: Order matters; the first slice is small and proves the whole thesis.
  1. **Store: give traversal a shape.** Add to `crates/brainiac-store/src/entities.rs` a
     `neighbors_explained(conn, anchors, hops, relations: &[EdgeRelation], direction) -> Vec<Hop>`
     where `Hop { from_entity, to_entity, relation, memory_id, depth }` — the same recursive CTE as
     `neighbors` (`entities.rs:206-232`) but *keeping* the joined `edges.relation`, `edges.memory_id`
     and the frontier parent instead of `SELECT DISTINCT entity_id`, and with the direction unions
     made optional so `depends_on` can be followed reverse-only. Then `path_between(a, b, max_hops)`
     as a bounded bidirectional variant returning one `Vec<Hop>`. Both stay inside `scoped_tx` so RLS
     still decides what a caller may see, and `memory_id` evidence is null-safe exactly like
     `CanonicalEdge.evidence` already is (`console.rs:3264`).
  2. **MCP: three tools.** In `tool_definitions()` (`mcp.rs:430`) add `graph_impact`,
     `graph_path`, `graph_owners`; register them `"read"` in `tool_scope` (`mcp.rs:217`) next to
     `entity_lookup`; dispatch in the `match name` at `mcp.rs:674`. Each returns hops with
     `memory:<uuid>` refs so the existing `memory_provenance` / `memory_feedback` loop works on a
     *path* the way it works on a memory. Tool descriptions must say when to call — "before you
     change shared behavior" — in the same voice as the `memory_context` description.
  3. **REST parity + eval.** `GET /v1/graph/impact/{id}` and `/v1/graph/path?from&to` beside the
     existing routes in `console.rs:48-61`, with `utoipa` annotations so the generated console types
     come free. Add a structural profile to `crates/brainiac-eval` that scores recovered dependency
     sets against `fixtures/*/entities/entities.yaml` — the graph's first honest quality number.
  4. **Console: same query, drawn.** Swap `graph_canonical`'s relation-blind `neighbors` for the
     explained hops and render direction + relation labels in `StarChartView.tsx:104` (arrowheads,
     relation text on hover) and in `NeighborHops`/`EvidenceEdges` (`DetailSections.tsx`). Add an
     "impact" toggle on the focus card that lights the reverse-`depends_on` closure in a warning tone.
  **Smallest first slice**: `neighbors_explained` + `graph_impact` (one relation, one direction,
  2 hops) + the console arrowheads. One store function, one tool, one visual change — and an agent
  can answer a question no one in the product can answer today.

## 2. The fourth dimension: a bitemporal graph that knows when its own knowledge went stale

- **Severity**: High
- **Category**: temporal-knowledge
- **File**: `migrations/0001_init.sql:143-153` (`edges.valid_from` / `valid_to` — declared, never
  written), `crates/brainiac-store/src/entities.rs:119` (`insert_edge`, binds 6 of 8 columns),
  `crates/brainiac-pipeline/src/extract.rs:781` (the only production edge writer),
  `crates/brainiac-server/src/console.rs:3105` (`graph_overview`, always "now"),
  `console/app/console/modules/graph/CortexMap.tsx:39` (a snapshot with no time control).
- **Scenario**: A maintainer drags a scrubber under the Cortex Map back to the week of the incident
  and the sky rearranges: `refund-worker` had no edge to `ledger-service` yet, `psp-gateway` still
  bound three teams. She sees the exact structural change that preceded the outage. Meanwhile the
  system watches the same signal without her: when a canonical hub's edge set changes — a new
  `depends_on` appears, an old one is closed — every memory anchored to that hub is flagged
  *structurally stale* and routed into the existing re-verification queue, with a reason a human can
  read: "kafka gained 2 dependencies and lost 1 since this decision was written." An agent asks
  `memory_context` and gets a warning it has never been able to get: *this knowledge predates a
  structural change to the thing it describes.* And the org gets a leading indicator — hubs whose
  edge churn spikes are the hubs about to break.
- **Root cause**: Every graph read is `SELECT ... WHERE` with no time predicate. `graph_overview`
  (`console.rs:3112-3189`) and `graph_canonical` (`console.rs:3333-3396`) compute counts over the
  live tables; the map is therefore always and only *now*, which is why it can be a picture and
  nothing more — a static value has nothing to explain. The schema already anticipated the fix:
  `edges` carries `valid_from timestamptz, valid_to timestamptz` (`0001_init.sql:151-152`), and a
  repo-wide grep finds **zero** writers and **zero** readers of either column — `insert_edge`
  (`entities.rs:119-141`) simply omits them, so every edge is timeless and no edge is ever retired.
  (`valid_to` is used, meaningfully, on *memories* in the disputes module — proving the org already
  reasons in this idiom, just never about structure.) Compounding it: extraction re-inserts edges
  `ON CONFLICT (id) DO NOTHING` keyed by a fresh uuid, so a re-ingest *accumulates* edges and the
  graph can only grow. Nothing can ever be observed to change, so nothing can be predicted from
  change, and staleness stays a per-memory TTL clock rather than a fact about the world the memory
  describes.
- **Impact**: Turns the single most expensive failure mode in an org memory server — confidently
  serving knowledge about a system that has since been rewired — from undetectable into automatic.
  Today re-verification is time-based: a memory is suspect because it is old. With structural drift
  it is suspect because *the thing changed*, which is orders of magnitude better precision: instead
  of re-reviewing everything past its TTL, maintainers re-review the (much smaller) set of memories
  whose subject actually moved. It also creates a genuinely new artifact — an audit-grade history of
  the org's architecture, derived for free from knowledge the pipeline is already extracting. No one
  buys a diagram; an incident timeline that reconstructs what the architecture looked like at 03:14
  last Tuesday is a compliance and postmortem asset. And it is the one moonshot here that makes the
  map *itself* worth more: a graph you can scrub is a graph people open twice.
- **Fix sketch**: This is not schema work — the columns exist. It is teaching the writer and readers
  to use them.
  1. **Write the seam.** Extend `entities::insert_edge` (`entities.rs:119`) with `valid_from` (the
     provenance timestamp of the extracting run, not `now()` — the pipeline already has it at
     `extract.rs:781`) and add `close_edge(conn, id, at)` setting `valid_to`. Change the extraction
     edge write to be *reconciling* rather than blind-insert: for the `(src, dst, relation)` triple,
     keep an open row if one exists, else open a new one — and add a `close_absent_edges` step so a
     re-extraction over the same source retires triples it no longer asserts. This is the whole
     mechanism; everything else is a read.
  2. **Read as-of.** Add an optional `as_of: Option<DateTime<Utc>>` to `graph_overview` and
     `graph_canonical` (`console.rs:3105`, `3317`) that appends
     `AND valid_from <= $as_of AND (valid_to IS NULL OR valid_to > $as_of)` to the `edges` joins.
     Default `None` = today's behavior, byte-identical, so nothing regresses. Same parameter on the
     `graph_impact` / `graph_path` tools from finding 1 — an agent can then ask what the dependency
     graph looked like when a decision was made.
  3. **Derive the signal.** A store query `hubs_with_edge_churn(since)` over `valid_from`/`valid_to`
     grouped by canonical, and a pipeline step that marks memories anchored to a churned hub as
     needing re-verification with a `reason` string, feeding the queue the TTL/reverify lifecycle
     already drains. Surface the churn count on `OverviewCanonical` (`console.rs:3044`) so the map
     can render it.
  4. **Render time.** A scrubber in `CortexMap.tsx` (`:39`) that sets `as_of` and re-fetches; star
     radius already encodes memory volume, so add a subtle churn ring reusing the existing
     `band("gamma")` treatment at `StarChartView.tsx:136`. Cheap, because both views already take
     one `CortexData` prop and re-render deterministically from it.
  **Smallest first slice**: write `valid_from` on insert + honor `as_of` in `graph_canonical` + one
  read-only test asserting an edge created after timestamp T is invisible at T. Two dormant columns
  become the product's time axis, in under a day's work, before any UI exists.
