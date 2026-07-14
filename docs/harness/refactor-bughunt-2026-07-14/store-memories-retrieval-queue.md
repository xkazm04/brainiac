> Context: Store: Memories + Hybrid Retrieval + Queue
> Total: 5 (Critical: 0, High: 1, Medium: 4, Low: 0)

## 1. Graph-expansion picks memories by UUID, not "strongest" — `DISTINCT ON` forces the wrong order
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: wrong-results
- **File**: crates/brainiac-store/src/memories.rs:357-375 (ORDER BY at 363)
- **Scenario**: An anchor entity has 40 canonical/candidate memories linked. Stage 4 calls `for_entities(tx, neighbor_entities, 10)` to surface the +10 cross-team extras. The SQL is `SELECT DISTINCT ON (m.id) … ORDER BY m.id, m.created_at DESC LIMIT 10`.
- **Root cause**: Postgres `DISTINCT ON (m.id)` requires the `ORDER BY` to lead with `m.id`, so the *final* result set is ordered by `m.id` (a UUID) ascending; `LIMIT 10` then returns the 10 smallest UUIDs. The trailing `created_at DESC` only breaks ties *within* one `m.id` group (never happens — one row per memory), so it is dead. The doc comment ("Strongest visible memories … Bounded") describes intent the query does not implement.
- **Impact**: The headline "cross-team knowledge surfaces here" feature (retrieval.rs §4) returns an essentially arbitrary 10 memories keyed on UUID rather than recency/strength. In retrieval.rs every graph extra is scored with the *identical* `graph_relevance(anchor_strength)`, so the SELECTION is the whole result — arbitrary selection = arbitrary graph output. If UUIDs are time-ordered (v7) it deterministically returns the *oldest* memories, the opposite of intent.
- **Fix sketch**: Nest the dedupe and the ranking: `SELECT * FROM (SELECT DISTINCT ON (m.id) … ORDER BY m.id) s ORDER BY s.created_at DESC LIMIT $2` (or add a real strength column: confidence, recency, feedback) so the cap keeps the strongest, not the lowest-UUID.

## 2. Memory UPDATE policy is org-only while read is visibility-scoped — `extend_validity` mutates (and probes) memories the caller can't see
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: rls-scope-asymmetry
- **File**: crates/brainiac-store/src/memories.rs:421-437 (policy: migrations/0001_init.sql:252-272)
- **Scenario**: A principal in org O calls `extend_validity(id, days)` with the UUID of another user's `private` memory (or another team's `team` memory) in O. `memories_read` (0001 + 0002) enforces the three-tier `private/team/org` model, but `memories_update` USING/WITH CHECK is only `org_id = current_setting('app.org_id')::uuid`. The UPDATE `WHERE id = $1 AND superseded_by IS NULL` therefore matches, sets a new `valid_to`, and `fetch_optional` returns `Some(new valid_to)`.
- **Root cause**: Read authority is visibility-scoped; write authority was left org-scoped (schema comment at 0001:284 says "org-scoped RLS for the remaining tables" and never re-tightens UPDATE to visibility). The read/write policies are asymmetric.
- **Impact**: A caller can extend the lifecycle of — i.e. mutate — memories it is not allowed to read, and the returned `valid_to`/`None` is an existence+validity oracle across the private/team boundary within an org. Any UPDATE path (not just this one) inherits the same over-broad authority. Not a cross-org breach, but a within-tenant authorization gap on the re-verification write path.
- **Fix sketch**: Mirror the `memories_read` visibility predicate in the `memories_update` USING clause (owner/team/org), or add the visibility predicate to `extend_validity`'s `WHERE`. If the path is deliberately worker-only, gate it behind `worker_tx` and document that end-user `scoped_tx` must never reach it.

## 3. `job_id_for_source` seq-scans the never-pruned archive on every idempotent-replay check
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: unbounded-scan
- **File**: crates/brainiac-store/src/queue.rs:375-386
- **Scenario**: Every retried `memory_add` that hits an existing keyed source resolves the original job via `SELECT id FROM queue.jobs WHERE payload->>'source_id' = $1 UNION ALL SELECT id FROM queue.archive WHERE payload->>'source_id' = $1 ORDER BY id LIMIT 1`.
- **Root cause**: There is no index on the JSON expression `payload->>'source_id'` on either table, and — as this function's own doc states — `queue.archive` is "never time-prune[d]". `idx_queue_jobs_ready` covers `(queue_name, visible_at)`, not the payload path, so both arms are sequential scans.
- **Impact**: Cost is O(all jobs ever enqueued) per replay check and grows without bound over the deployment's lifetime (archive only accumulates). `LIMIT 1` bounds the *rows returned*, not the *rows scanned*. Latent scalability failure on a hot ingest path; invisible at fixture scale, steadily worse in production.
- **Fix sketch**: Add an expression index `CREATE INDEX … ON queue.jobs ((payload->>'source_id'))` and the same on `queue.archive` (or promote `source_id` to a real indexed column), and/or add an archive retention/prune job.

## 4. `vector_literal` emits `NaN`/`inf` for non-finite floats — invalid pgvector text, a hard error on a degenerate embedding
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: edge-case
- **File**: crates/brainiac-store/src/memories.rs:441-452 (used by search_vector:213 and upsert_embedding:162)
- **Scenario**: An embedder produces a non-finite component (e.g. L2-normalizing a zero/all-OOV vector → division by zero → `NaN`, or numeric overflow → `inf`). `x.to_string()` renders these as literal `NaN` / `inf` / `-inf`, so the built literal is `"[NaN,…]"`.
- **Root cause**: `vector_literal` assumes every `f32` is finite and formats each element unconditionally with `to_string()`; pgvector's `vector` input parser rejects NaN/±infinity outright.
- **Impact**: The `$1::vector(dim)` cast (query path) or the `$3::vector` insert (embedding path) fails with a DB error, turning a degenerate-but-recoverable embedding into a 500 for the whole retrieval request / a failed ingest write, rather than an empty or skipped result. (The related cosine `<=>` on a genuine zero vector also yields `NaN` scores that `sort_candidates` orders arbitrarily via `partial_cmp … unwrap_or(Equal)`.)
- **Fix sketch**: Guard in `vector_literal` — reject or sanitize non-finite inputs before building the literal (e.g. return an error, or map `!x.is_finite()` to `0.0`), and have callers treat a non-finite query embedding as "no vector candidates" instead of erroring.

## 5. `vector_literal` is duplicated verbatim across `memories.rs` and `entities.rs`
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-store/src/memories.rs:441-452 and crates/brainiac-store/src/entities.rs:339-350
- **Scenario**: Both files define a byte-identical private `fn vector_literal(v: &[f32]) -> String`; entities.rs's doc even says "mirrors memories.rs". Both are the sole builders of pgvector literals in the store.
- **Root cause**: The helper was copied when canonical-entity embeddings (Direction 2) were added rather than shared, to avoid a cross-module dependency.
- **Impact**: Standard divergence risk, but sharper here: the non-finite-float bug in finding #4 exists in *both* copies (search_vector, upsert_embedding, upsert_canonical_embedding, nearest_canonical), so any fix must be applied twice or it silently regresses one call site. Two functions to keep in lock-step for the same wire format.
- **Fix sketch**: Hoist a single `pub(crate) fn vector_literal` into a shared module (e.g. a `crate::vector` util or lib.rs) and have both call it; fixing #4 there covers every pgvector-literal call site at once.
