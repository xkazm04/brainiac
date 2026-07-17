# PROJECT-PLAN — the project dimension across memory, knowledge, and the console

**Status:** analysis settled 2026-07-17; **PR0, PR1 and PR2 built 2026-07-17**
(0035 + stamping through HTTP/MCP/worker; retrieval lens in both candidate
queries + `admits`, REST `SearchBody.project_id`, MCP `scope: "project"`;
console facet pass — archive project channel with a selectable org-shared
bucket, reviews project chip + filter, disputes project facet, MCP
memory_list project scope, demo data de-conflated to application-shaped
projects). **PR3 built 2026-07-17**: divergence gains the cross-project class
(migration 0037 `axis` column, a project-axis adjudication pass with its own
higher-bar prompt, class badge on the board), graph_overview grows project
lobes/links + hub project spread with a team/project lens in the Star Chart,
the observatory matrix axis-swaps kind×team ↔ kind×project, and Knowledge
Health carries cross-project entities/contradictions (live payload;
snapshot-table persistence deliberately deferred until the counters have
non-zero history). **PR4 built 2026-07-17**: `documents.project_id`
(migration 0038, nullable — an unstamped page composes exactly as before),
the project cap in the compose `admits` firewall (own project + org-shared,
never a sibling's; unit-tested beside the visibility invariants), and
`DocSummary.project_id` over REST. Deferred from PR4: per-project digest
scaffolding (until stamping coverage makes them non-empty), a project facet
in the wiki UI (the docs surface is under active rework), and a
binding-level project override (the page-level stamp is the meaningful
unit until a real page needs finer grain).

**All planned phases (PR0–PR4) are built.** Follows migration
0034 (projects + repo whitelist + project-scoped keys + the brainiac-onboard
skill), which put `project_id` on credentials and stopped there — deliberately:
`AuthContext.project_id` reaches every handler and nothing enforces on it yet.

## Why this exists

Onboarding now mints keys scoped to a **project** (an application or business
domain; repos whitelist into exactly one). But every content row — memories,
sources, documents, divergences — knows only `org` and `team`. So a
project-scoped key is currently a label, not a lens: nothing it writes is
attributed to its project, and nothing it reads can be narrowed to one.

The audit of both planes says the gap is narrow and the payoff is wide:

- **Server:** one write funnel (`NewMemory`/`memories::insert`,
  `governance::insert_source`) feeds everything; retrieval has exactly two SQL
  candidate queries plus one post-filter; every aggregation that says
  `GROUP BY team_id` or `team_id IS DISTINCT FROM team_id` is a template for a
  project twin.
- **Console:** every surface that shows a grouping (archive facet channels,
  star-chart lobes/legend, observatory kind×team matrix, divergence
  approach cells, review filter chips, disputes facets, usage pulses) is
  already generic over a team-shaped dimension; none of the content schemas
  carry `project_id`. Schema first, then mostly-mechanical UI.

## Principles

1. **Org stays the tenancy root; project is a facet, not a partition.**
   `project_id` is nullable everywhere. NULL = org-shared, and org-shared is a
   *legitimate tier* (standards, conventions, cross-cutting decisions), not
   missing data. Model on `memories.title` (0023, nullable+fallback), NOT
   `lifecycle` (0015, NOT NULL) — forcing a default project would launder
   org-knowledge into a project it doesn't belong to.
2. **Team answers *who*; project answers *what about*.** They are orthogonal:
   two teams contribute to one application; one platform team spans every
   project. Nothing replaces team — project is added *beside* it.
3. **Attribution never widens silently.** A write from a project-scoped key is
   stamped with that project, always. Promotion of a project memory to
   org-shared (NULLing the stamp) is an explicit human act — same asymmetric-
   projection instinct as KB-PLAN.
4. **Reads are "my project + org-shared", never "my project only".** A
   project lens filters to `project_id = X OR project_id IS NULL`. Hard
   project isolation (RLS) is deliberately out of scope — visibility tiers
   already answer *who may see*; project answers *what is relevant*.
5. **No implicit filters in v1.** A project-scoped key's searches are NOT
   silently narrowed; the lens is an explicit lever (REST param, MCP scope,
   console facet). Once stamping coverage is high, revisit as a *ranking
   boost* for the session's project — a boost degrades gracefully where a
   filter silently hides.

## Phases

### PR0 — Stamp the writes (the enabler; smallest surface, do first)

Everything else is display; this is the data. Until writes are stamped, every
downstream surface renders an empty dimension.

- **Migration 0035:** `sources.project_id` + `memories.project_id`, nullable
  `REFERENCES projects(id)`, composite index `(org_id, project_id)` on both.
  (Provenance rows need nothing: they point at sources.)
- **Store:** `NewMemory.project_id` + INSERT column (memories.rs:13-72);
  `insert_source`/`insert_source_idempotent` gain the arg (governance.rs:11-68).
- **HTTP writes:** the three source-insert sites (memory_add single/keyed,
  bulk) pass `ctx.project_id` (http.rs:557-671).
- **MCP:** `McpState` currently DROPS `ctx.project_id` at startup (mcp.rs:
  234-264) — carry it, stamp `memory_add`'s source with it.
- **Worker/extract:** re-derive project from the source row in `run_extract`
  (extract.rs:701-723) — no queue-payload change; the source row stays the
  single source of truth.
- **Tests:** extend onboard_pg: a memory written with the onboarded key lands
  with its project stamped; a memory from an org-wide key lands NULL.

### PR1 — The retrieval lever

- `RetrievalFilters.project_id` + the predicate
  `(project_id = $N OR project_id IS NULL)` in `search_vector`,
  `search_fts` (memories.rs:283-392) and `admits` (retrieval.rs:104-113).
- REST: `SearchBody.project_id`; MCP: extend the `scope` lever
  (org|team → org|team|project, resolving "project" from the session key).
- Compose (KB) picks it up for free once `RetrievalFilters` carries it; the
  binding facet itself is PR4.

### PR2 — Console: the mechanical facet pass

Add `project_id`/`project` (name) to the content DTOs and let the
already-generic components do the rest:

- **Archive:** `MemoryRow.project`, the server facet menu
  (`archive.rs::facets` gains the dimension; `MemoryFacetMenu.projects`), one
  entry in the client `FACETS` array (Archive.tsx:92-99) — channel strip,
  column filter, and search scope come along automatically.
- **Reviews:** `PromotionMemory.project` → a chip in the pane and a filter
  next to team (ReviewWorklist Filters). A reviewer deciding whether a claim
  generalizes needs to know which application it came from — this is the
  cheapest real improvement to decision quality in the whole plan.
- **Disputes:** `FeedbackFacets.projects` beside teams.
- **Keys/Projects demo data:** de-conflate the demo — today demo project
  names mirror team names (payments/platform/data), which teaches the wrong
  model. Make demo projects application-shaped (`payments-api`,
  `checkout-web`) with teams crossing them.

### PR3 — Maps, drift, decisions (the enrichment the console earns)

The team-aggregation surfaces get project twins, in value order:

1. **Divergence, project axis.** The sweep (divergence.rs:85-197) clusters by
   team and adjudicates ≥2-team clusters. Add project to positions
   (`{team, project, approach}`) and detect **cross-project** divergence as
   its own class: "checkout retries one way, billing another" is a different
   finding from "two teams disagree" — often more actionable, because the
   standard that resolves it is a per-stack rule the Library already models.
   UI: approach cells show project alongside team; a class toggle
   (cross-team | cross-project).
2. **Graph, project lobes.** `graph_overview` (console.rs:2997-3039) computes
   team lobes, hub team-spread, team links. Add the project equivalents
   (lobes from `memories.project_id`, hub project-spread, project links) and
   a lens toggle in CortexMap/StarChart — the legend/lobe machinery is
   already generic over the grouping.
3. **Observatory:** kind×project matrix as an axis swap next to kind×team;
   top-entity project-spread coloring.
4. **Health:** `cross_project_entities` / `cross_project_contradictions` +
   a liquidity read ("does knowledge cross application lines"), mirroring the
   existing cross-team counters (console.rs:2321,2455) into the snapshot
   table.

### PR4 — Knowledge base

- `documents.project_id` (nullable, same semantics) + `project` as a section
  binding facet + the compose `admits`-style cap so a project page never
  quotes another project's private tier.
- Per-project digest pages once stamping coverage makes them non-empty.

## Deferred (recorded, not planned)

- **RLS enforcement on project** (hard isolation) — would change principle 4;
  only if a customer needs contractual separation inside one org.
- **Monorepo `path_prefix` on project_repos** — the schema slot exists in the
  design; add when a real monorepo onboards.
- **Promotion flow project→org-shared** — explicit console action + audit
  event; until then the stamp is immutable.
- **GitHub App repo-access proof at approval** (onboarding phase 2) — changes
  only onboard.rs's approve step.
- **Library:** project as a usage-pulse split and skill-targeting facet.
- **Backfill:** historical memories stay NULL (honest: their project is
  unknown). A best-effort backfill via `sources.external_ref` remotes is
  possible if ever worth it.

## Open decisions (flagged, with defaults)

1. **Divergence axis semantics** — default: keep team clusters, ADD
   cross-project as a second detected class (not a replacement).
2. **MCP default lens** — default: no implicit filter (principle 5); the
   agent opts in via `scope: "project"`. Revisit as a ranking boost after
   PR0 has been live long enough to have coverage.
3. **Should `memory_add` (REST) accept an explicit `project_id` override for
   org-wide keys?** — default: yes, validated against the org, so CI/import
   jobs holding an org key can still attribute correctly.
