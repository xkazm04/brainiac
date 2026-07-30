# Moonshot scan — projects

> Total: 2
> Files read: 17

## 1. Concordance Engine — turn "N projects independently agree" into org canon, and collapse the duplicates

- **Severity**: Critical
- **Category**: knowledge-convergence
- **File**: `crates/brainiac-pipeline/src/divergence.rs:15` and `:233` (the discard),
  `crates/brainiac-store/src/projects.rs` (the project registry this reasons over),
  anchors: `crates/brainiac-store/src/governance.rs:141` `insert_promotion` / `:212`
  `apply_supersession` / `:170` `set_memory_status`, `crates/brainiac-pipeline/src/policy.rs:22`
  `PolicyEngine::evaluate`, `crates/brainiac-store/src/memories.rs:53` (the `project_id` INSERT
  column), `crates/brainiac-pipeline/src/library_sweep.rs:104` `mine_divergences`
- **Scenario**: An org runs 12 applications. `payments-api`, `checkout-web` and `billing-jobs`
  each independently recorded "retry the ledger write with jittered backoff, never a fixed sleep" —
  three separate memories, three separate `project_id` stamps, all correct, all saying the same
  thing. Today the org owns three copies and knows it owns three copies exactly nowhere. After this
  moonshot, the sweep that already asks a provider "do these applications disagree?" also *keeps
  the yes-they-agree answer*, and a concordance verdict backed by ≥K independently-stamped projects
  becomes a **promotion proposal to the org-shared tier**: one memory with `project_id = NULL`,
  provenance listing every project that witnessed it, and the K project-local copies superseded by
  it via the existing `apply_supersession`. A reviewer sees "3 of your 12 applications learned this
  separately — make it org canon?" — the single highest-signal review card the product can show,
  because independent rediscovery is the strongest evidence a claim generalizes that any
  organization ever produces. From then on, an agent booting into application 4 gets the rule on
  its first `memory_context` call instead of relearning it in week six.
- **Root cause**: Two compounding constraints. (a) `divergence.rs` explicitly throws concordance
  away — the module doc says "the sweep only stores `divergence: true` verdicts" (`:15`) and
  `:233` gates on `if v.divergence`. The org pays for an LLM adjudication of every cross-project
  cluster and destroys half the answer, the *useful* half for a platform team. (b) `project_id` is
  a write-once stamp: the only `project_id` write in the whole workspace is `onboard.rs:144`
  (approving a pairing request); there is no UPDATE path on `memories.project_id` anywhere, and
  PROJECT-PLAN records "Promotion flow project→org-shared" as deferred with "until then the stamp
  is immutable". So knowledge can flow *down* into a project forever and can never flow *up* out
  of one. Org-shared knowledge can only exist if a human happened to write it from an org-wide key.
  The corpus therefore grows linearly in (projects × lessons) and never converges — the opposite of
  what a memory server is for. Note the existing convergence loops do NOT cover this:
  `mine_divergences` (library_sweep.rs:104) turns *conflict* into a Library standard, and
  `mine_reinforced` (`:134`) requires a memory a human *already* made `canonical` plus N `helpful`
  feedback rows. Neither counts independent cross-project rediscovery, and neither produces a
  memory-tier canon row — they produce Library artifacts on a different plane.
- **Impact**: The 10x is a change of growth curve. An org with P projects and a shared stack today
  stores ~P copies of every cross-cutting lesson, pays P× the review cost, and gives every project
  P-independent (i.e. mostly zero) benefit from the others' scars. Concordance turns that into 1
  canonical row with P witnesses: reviewer cards per lesson drop from P to 1; retrieval stops
  returning three near-duplicate hits inside one k-budget (a direct, meridian-measurable precision
  win — duplicate suppression is the cheapest recall-for-precision trade in the corpus); and the
  canon tier becomes something that *emerged bottom-up from evidence*, which is a categorically
  stronger artifact than a standard somebody wrote. It also creates the metric the product has no
  way to state today: **knowledge convergence rate** — what fraction of an org's lessons have been
  learned once vs. relearned per application. That number is the CTO-facing case for the whole
  product, and it only exists once the project dimension can converge.
- **Fix sketch**: The sweep already computes everything needed; the axis loop in
  `scan_org` (divergence.rs:163-238) groups positions by canonical entity and already knows each
  position's `project` (LEFT JOIN at `:131`).
  1. **Slice one (proves it, no schema drama):** widen the project-axis prompt
     (`DIVERGENCE_PROJECT_PROMPT_V1`, `:46`) from a boolean to a three-way verdict —
     `divergence | concordance | unrelated` — and persist concordance rows to a new
     `practice_concordances` table (same shape as `practice_divergences`: org_id, canonical_id,
     practice, summary, `witnesses` jsonb of {project, memory_id}, impact, model_ref). Nothing
     else changes; the console gets a board twin of the divergence board. This alone is a shippable
     product surface and turns an already-paid-for LLM call into new value.
  2. **Slice two (the promotion):** a `knowledge_promotions` proposal that a maintainer accepts in
     the existing reviews flow. Accept = insert one `NewMemory` with `project_id: None`,
     `status: candidate`, provenance rows pointing at each witness's source (`insert_provenance`,
     governance.rs:115), then `apply_supersession(winner = canon, loser = each project copy)` —
     which already sets `valid_to`/`superseded_by` and is already honored by
     `apply-supersessions` in retrieval, so the project copies degrade to time-travel history
     rather than disappearing. Gate the auto-path in `PolicyEngine::evaluate` with a new rule id
     (`concordance_k_projects`, requiring witnesses ≥ 3 and zero open contradictions — the
     `contradiction_pending` guard at policy.rs:41 already composes correctly here).
  3. **Guardrails, cite them in the tests:** an *isolated* project (migration 0040) must never be
     a witness whose content lands in an org-shared row — that is the same asymmetric-projection
     leak already logged against `compose::admits`, and this path must not reopen it. Filter
     witnesses on `NOT projects.isolated` in the gathering query; assert it in
     `divergence_pg.rs`-style coverage.
  4. **Prove it:** a meridian profile twin of `divergence_profile.rs` (`:280` reads the
     divergence rows back) with golden fixtures where three projects state the same rule three
     ways — assert one canon row, three supersessions, and that a project-lensed search now
     returns one hit where it previously returned three.

## 2. Project lineage — fork, archive and merge an application's brain

- **Severity**: High
- **Category**: platform-surface
- **File**: `crates/brainiac-store/src/projects.rs:32-111` (create/list/isolation is the whole
  lifecycle), `crates/brainiac-server/src/projects.rs:24-33` (the four routes),
  `migrations/0034_projects_onboarding.sql` (the 4-column table),
  anchors: `crates/brainiac-store/src/retrieval.rs:74-79` + `:121-123` (`admits`, the project
  lens), `crates/brainiac-store/src/memories.rs:319` and `:382` (the two candidate-query
  predicates), `crates/brainiac-pipeline/src/compose.rs:78-88` (the compose project cap),
  `crates/brainiac-store/src/archive.rs:196` (the time-travel read that makes archiving safe)
- **Scenario**: `checkout-web-v2` is spun up as a rewrite of `checkout-web`. Today its agent
  memory starts empty — every payment-gateway quirk, every "the tax service lies about rounding"
  pitfall, every ADR is on the other side of an immutable stamp, and the team relearns it. After
  this: `POST /v1/projects/{id}/fork` creates v2 with a **lineage edge** to v1. v2's project lens
  resolves to `own ∪ ancestors ∪ org-shared`, with inherited rows ranked below native ones and
  tagged `inherited_from: checkout-web` in the provenance ref an agent already receives. The first
  time an agent in v2 records something incompatible with an inherited belief, the existing
  contradiction machinery opens it at the right level and the belief *forks* — v2 gets its own
  native memory, v1 keeps its own, and neither corrupts the other. Meanwhile the sunset app
  `checkout-legacy` gets `POST /v1/projects/{id}/archive`: its memories leave default retrieval so
  agents stop being told how a decommissioned system behaves, but stay fully readable in the
  archive and under `as_of` time travel — so the engineer who asks "why did we ever do it that
  way" still gets an answer three years later. When two apps consolidate, `merge` re-parents one
  project's stamps into the other behind an audit event — the first and only legitimate reason for
  a `memories.project_id` UPDATE.
- **Root cause**: The `projects` row is an identity, not an entity with a life. Migration 0034
  gives it `(id, org_id, name, created_at)`; 0040 adds `isolated`. The API is create / list /
  add-repo / remove-repo / toggle-isolation — there is **no delete, no archive, no state, no
  description, no relationship between projects at all** (`grep "DELETE FROM projects"` returns
  nothing across the workspace; 0034 deliberately made `api_tokens.project_id` `NO ACTION` so a
  project with live keys cannot even be dropped). Combined with the immutable stamp, the model
  asserts that the set of applications an org has is flat, permanent, and unrelated. Real orgs are
  none of those: apps are forked, split, merged, renamed, and sunset constantly, and app *families*
  (v1/v2, per-tenant deployments, an agency's near-identical client builds) are the normal case,
  not the exception. Every one of those events currently either loses knowledge or leaves stale
  knowledge competing in retrieval forever — and the second failure is the quiet one, because a
  decommissioned system's memories look exactly as trustworthy as a live system's to a ranker.
- **Impact**: Three distinct 10x claims. (1) **Day-zero competence.** A forked project's agent
  starts with its ancestor's canon instead of nothing — the time-to-useful-memory for a new
  application goes from "months of accumulation" to one API call. For an org standing up its 13th
  service, that is the difference between the memory server being an asset and being a chore.
  (2) **Precision in legacy-heavy orgs.** Archiving demotes sunset systems instead of deleting
  them; in an org where a third of stamped memories describe systems that no longer exist, that is
  a large, directly meridian-measurable precision gain on exactly the queries agents ask most.
  (3) **The project becomes a transferable unit.** Once a brain has lineage, "fork a brain" is a
  primitive — the same mechanism that seeds v2 from v1 is the mechanism that seeds a new client
  project from a template project, which is the platform seam under any future marketplace of
  starting knowledge. (Cross-*org* norm packs are a sibling's scope; this is the intra-org
  primitive they would ride on.)
- **Fix sketch**: The retrieval seam is already a single narrow place, which is what makes this
  achievable rather than aspirational — `RetrievalFilters.project_id` (retrieval.rs:79) feeds
  exactly two SQL predicates (memories.rs:319, :382) and one post-filter (`admits`, :121).
  1. **Smallest first slice — archive only, no lineage, no new joins.** Add
     `projects.state text NOT NULL DEFAULT 'active' CHECK (state IN ('active','archived'))` and
     `POST /v1/projects/{id}/archive` next to `project_set_isolation` (server/projects.rs:434 is
     the exact template — same admin scope, same `rows_affected() > 0` → 404 outcome shape as
     `set_isolated`). Exclude archived-project rows from the two candidate queries with a
     `NOT EXISTS (… projects p WHERE p.id = m.project_id AND p.state = 'archived')` clause, and
     bypass the exclusion whenever `RetrievalRequest.as_of` is set — so time travel and the
     archive surface (archive.rs:196 already selects `valid_from/valid_to/superseded_by`) still see
     everything. One meridian golden fixture proves it: the archived project's memory leaves the
     default answer and returns under `as_of`. Console: a state badge in
     `console/app/console/modules/projects/Projects.tsx`.
  2. **Lineage.** `project_lineage (child_id, parent_id, mode text CHECK (mode IN ('fork','merge')),
     created_at)` + `projects::ancestors(pool, org_id, project_id)` (recursive CTE, depth-capped).
     Widen `RetrievalFilters.project_id: Option<Uuid>` to a small `ProjectScope { own: Uuid,
     inherited: Vec<Uuid> }` — the field doc at retrieval.rs:74-79 already frames the lens as
     inclusive-by-design, so this is a widening of an existing intent, not a new principle. Update
     `admits` (:121) and both predicates to `project_id = own OR project_id = ANY(inherited) OR
     project_id IS NULL`, and apply a fixed rank penalty to inherited rows so they lose ties to
     native ones (degrade-gracefully, exactly principle 5's reasoning about boosts over filters).
     `compose::admits` (compose.rs:84-88) takes the same three-way extension so a v2 page may quote
     its ancestor but still never a sibling.
  3. **Isolation must win over inheritance** — `ancestors()` must drop any parent with
     `isolated = true`, or fork becomes a bypass of migration 0040's RESTRICTIVE policy. Assert it
     as a named test beside the existing `project_page_takes_its_own_project_and_org_shared_never_a_sibling`
     (compose.rs:1037), which is the pattern to copy.
  4. **Merge last**, because it is the only step that mutates a stamp: a transactional re-parent
     plus a promotions-style audit row, admin-scoped, with the `project_lineage` `mode = 'merge'`
     edge left behind so the provenance of the move survives the move.
