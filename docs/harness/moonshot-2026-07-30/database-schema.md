# Moonshot scan — database-schema

> Total: 2
> Files read: 22

## 1. Break the tenant ceiling: a knowledge commons above `org_id`

- **Severity**: Critical
- **Category**: network-effect
- **File**: `migrations/0001_init.sql:66` (`CREATE TYPE visibility AS ENUM ('private','team','org')`),
  `migrations/0001_init.sql:252` (`memories_read`), `migrations/0028_library_substrate.sql`
  (standards/skills/`standards_attribution` trigger, all `org_id = current_setting('app.org_id')`),
  `migrations/0020_kb_publishing.sql` (`publish_targets`, `document_publications` — outbound only),
  `migrations/0022_identities.sql` (one identity → one org), `migrations/0040_project_isolation.sql`
  (the RESTRICTIVE-policy precedent), `crates/brainiac-core/src/redact.rs`,
  `crates/brainiac-core/src/fusion.rs`
- **Scenario**: A scope exists *above* the org. An org flags a standard, a skill version, or a
  `pitfall` memory as shareable; a redaction gate strips org identifiers; the atom lands in a
  commons registry addressed by a **dependency coordinate** (`cargo:tokio@1.38`,
  `npm:next@15`, `aws:sdk-s3`) rather than by whose org produced it. Other orgs *subscribe* to
  coordinates their repos actually use. Commons atoms appear in retrieval **below** local ones,
  always carrying `origin` provenance, and always overridable — and a local rule that contradicts
  an upstream one records `overrides_upstream_id`, so "we deliberately disagree with the world
  here, and here is why" becomes a queryable artifact instead of tribal lore. Usage flows back as
  anonymous counts; `library_usage_events` already has a `team_id` and *no* user column, so the
  privacy shape needed for this is already the shipped shape. A brand-new customer's first day
  stops being an empty corpus.
- **Root cause**: `org_id` in this schema is not merely a tenancy column — it is the **terminal**
  scope, and the ceiling is encoded in a *type*: the `visibility` enum's top rung is `org`, so
  knowledge above the tenant is not unimplemented, it is *unrepresentable*. Every policy in every
  migration is the equality `org_id = current_setting('app.org_id')::uuid`, and `identities`
  hard-binds a person to exactly one org. The only outward path that exists is `publish_targets`
  (0020, `git|confluence|okf`) — a one-way push, with `document_publications` recording what *left*
  the building and nothing whatsoever recording what came *in*, or where a rule came from upstream.
  Consequence: the value of the corpus to any org is bounded by the work that one org does. The
  Library — whose atoms (`standards`, `skill_versions`) are precisely the artifacts that are *not*
  org-specific, since "always set a tokio task budget" is nobody's private judgment — is duplicated
  from scratch in every tenant, N times, forever. `docs/LIBRARY-PLAN.md:138` names cross-org
  exchange as a follow-up but frames it as *publishing outward*; the leverage is in the return leg
  (subscription + lineage + override), which nothing in the schema anticipates.
- **Impact**: this is the difference between a tool and a registry. Per-org corpus utility on day 0
  goes from 0 atoms to the commons' N; the Library's value scales with the *number of tenants*
  instead of per-tenant effort, which is the only mechanism in this product with a genuine network
  effect. It also converts the deployment story: instead of selling an install per company,
  Brainiac becomes the thing every coding agent points at by default — crates.io/npm positioning for
  agent knowledge, with each org's private corpus layered on top. Secondary payoff: cross-*org*
  divergence is strictly more valuable than the cross-*team* divergence 0016 already adjudicates —
  "eleven orgs pin this differently than you do" is a stronger signal than any internal one.
- **Fix sketch**:
  1. **Slice 0 — read leg on standards only, zero blast radius.** Add `orgs.commons boolean` and
     seed one reserved commons org row. Add `standards.upstream_id uuid`, `standards.origin_ref
     text` (the coordinate), and widen `standard_provenance_kind_check` with `kind='upstream'`.
     Then one *permissive* policy `standards_commons` allowing reads of the commons org's rows when
     the reader has a `commons_subscriptions` row. The existing `standards_attribution` DEFERRED
     constraint trigger stays **untouched** and does exactly the right thing: an adopted upstream
     rule now attributes to its upstream row instead of to an org-private `memories.id` — which is
     precisely the reference that must never cross a tenant boundary. Gate on a new
     `orgs.commons_enabled`, following the `kb_enabled` precedent set in 0020 ("a feature that turns
     itself on is an incident").
  2. **Slice 1 — publish leg.** `commons_publications(artifact_kind, artifact_id, version,
     origin_org_id, redaction_verdict jsonb, published_at)`, mirroring `document_publications`. Run
     `brainiac_core::redact` as a hard **admission** gate (not best-effort), and add a `commons-leak`
     profile to `brainiac-eval` beside the existing docs/contradiction/divergence profiles — the
     leak matrix `docs/LIBRARY-PLAN.md:424` correctly demands before anything leaves a building.
  3. **Slice 2 — subscription + override.** `commons_subscriptions(org_id, coordinate,
     pinned_version)`; `standards.overrides_upstream_id`; add an upstream candidate leg in
     `brainiac-core/src/fusion.rs` ranked strictly below local candidates, so a commons atom can
     never outrank the org's own judgment.
  4. **Slice 3** — extend to `skill_versions` (already semver-unique per skill) and then to
     `memories WHERE kind='pitfall' AND project_id IS NULL`.
  5. **The invariant to write the test for first**: 0040 proves this codebase already knows how to
     add a policy that is provably a no-op by default (`NOT isolated` making the RESTRICTIVE clause
     always-true). The commons read policy is the mirror image — a *widening* — so it needs the
     same discipline: an integration test in `crates/brainiac-store/tests/store_pg.rs` asserting a
     non-subscribing org's visible row set is byte-identical before and after the migration.

## 2. Anchor memory to code: knowledge that decays with the repo, not the calendar

- **Severity**: High
- **Category**: code-anchored-lineage
- **File**: `migrations/0035_project_stamping.sql` (`memories.project_id` — the finest spatial
  granularity that exists), `migrations/0039_project_path_prefix.sql`
  (`project_repos.path_prefix`), `crates/brainiac-core/src/types.rs:114`
  (`MemoryKind::default_ttl_days` — 365/540/180 hard-coded),
  `crates/brainiac-server/src/mcp.rs:1265` (`find_by_remote(..., "")`),
  `crates/brainiac-store/src/projects.rs:165`, `migrations/0017_document_layer.sql`
  (`document_dependencies` — the anti-rot pattern to mirror)
- **Scenario**: `memory_anchors(memory_id, project_id, remote, path, symbol, blob_sha, anchored_at,
  drift_at)`. Extraction and the MCP write path stamp anchors from the files the agent already had
  open — the session already carries `BRAINIAC_REPO_REMOTE`, and `find_by_remote` already accepts a
  `path` argument that the MCP call site passes as `""`. A sweep compares each anchor's `blob_sha`
  against HEAD; a changed blob sets `drift_at`, which enqueues re-verification **with evidence**
  rather than on a timer, and lands in the reviews queue maintainers already work. On the read side,
  an agent editing a file gets that file's anchored memories at the top with no embedding
  round-trip — a filesystem-precision join, not a similarity guess. And the inverse index answers
  the question the corpus currently cannot even be asked: a **knowledge coverage map**, the repo
  tree heat-mapped by anchored-canonical density, so a VP Eng sees "`auth/` has 40 memories,
  `payments/` has 2 and six open contradictions". That is the first view in this product that is
  about the *code* rather than about the corpus.
- **Root cause**: the corpus has no spatial dimension. The finest link from a belief to code is
  `memories.project_id` → `project_repos.remote + path_prefix` — an *application*, not a location —
  and `provenance.source_id → sources.external_ref` is free text that the MCP path fills with kind
  `"manual"` and no path at all. So "this pitfall is about
  `brainiac-store/src/governance.rs::apply_supersession`" is inexpressible. Two capabilities fall
  out of that hole. (a) **Staleness is guessed by the calendar**: `default_ttl_days` gives every
  `howto` 180 days and every `decision` 540, so a procedure invalidated by yesterday's refactor
  stays "presumed true" for six months while a still-correct architectural decision is pointlessly
  re-verified — the TTL sweep's precision and recall are both set by a constant in a Rust match arm.
  (b) **Coverage is unmeasurable**: you cannot report where knowledge is missing when knowledge has
  no address to be missing from, which is why every health metric in
  `knowledge_health_snapshots` measures the corpus against itself.
- **Impact**: re-verification volume collapses onto rows with actual evidence of change — on a
  100k-memory corpus a TTL sweep offers a maintainer thousands of undifferentiated chores per
  quarter, while a drift sweep offers the handful whose underlying code actually moved, and catches
  the fast rot TTL structurally misses. Retrieval precision on file-scoped agent tasks stops
  depending on the embedder for information `git` already knows exactly. And coverage becomes a
  burn-down instead of a feeling. Structurally, this gives `document_dependencies` its missing
  twin: 0017 made pages recompose when memories change; this makes memories re-verify when *code*
  changes — the same inverted-index trick applied one layer down, which is what "GitOps for
  organizational AI knowledge" would actually mean.
- **Fix sketch**:
  1. `memory_anchors` + explicit `GRANT` + org-scoped RLS gated on a visible parent memory — copy
     the `document_dependencies_org` policy shape verbatim from 0017. Indexes:
     `(org_id, remote, path)` for the read path and a partial
     `... WHERE drift_at IS NOT NULL`, mirroring `idx_documents_dirty`.
  2. **Write path, smallest proving slice**: an optional `files: [{path, symbol?}]` argument on the
     ingest/`memory_write` MCP tool, anchored only to what the agent explicitly names — no repo
     scanning, no inference. Reuse `projects::find_by_remote(pool, org, remote, path)` as-is by
     finally passing the real path at `mcp.rs:1269` instead of `""`, which also improves project
     attribution for monorepos as a side effect.
  3. **Drift sweep** in `brainiac-pipeline`, registered alongside the existing sweep schedules
     (0018): resolve blob shas via the repo-access-proof work already queued, set `drift_at` on
     mismatch, and route into the *existing* re-verify/deprecate governance flow
     (`brainiac-store/src/memories.rs:617`, `governance::apply_deprecation`) rather than inventing a
     second queue.
  4. **Read path**: `memory_context` gains `files: []`; add an anchor-exact candidate leg in
     `brainiac-core/src/fusion.rs` beside the FTS/vector/graph legs, and a `meridian` fixture case
     proving an anchored memory beats a semantically-similar unanchored one for a file-scoped query.
  5. **Coverage map**: one aggregate over `memory_anchors` grouped by path prefix, surfaced in the
     console next to knowledge health — as a *signal*, deliberately not as a fifth pillar in the
     composite, the same call `docs/LIBRARY-PLAN.md`'s status log already made and documented.
