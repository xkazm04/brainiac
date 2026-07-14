> Context: Pipeline: Divergence + Reembed + Integration Tests
> Total: 5 (Critical: 1, High: 2, Medium: 2, Low: 0)

## 1. A half-backfilled embedding version is fully servable — silent, corpus-wide recall loss after an interrupted reembed
- **Severity**: Critical
- **Lens**: bug-hunter
- **Category**: mixed-embedding-inconsistency
- **File**: crates/brainiac-pipeline/src/reembed.rs:55-106 (+147)
- **Scenario**: Operator swaps embedders and runs `reembed(pool, embedder_b, batch)`. Line 60 calls `memories::ensure_embedding_version`, which (memories.rs:93-100) INSERTs the new version with `is_active = true` immediately, then the loops backfill batch-by-batch with per-batch autocommit and NO wrapping transaction (documented "resumable"). A crash / OOM / provider outage after N of M batches leaves version B populated for only part of the corpus. There is no completion marker and no "activate only when done" gate — `is_active` is written here but **never read anywhere** (confirmed: no `WHERE is_active` / no active-version selector exists). The served version is simply `ensure_embedding_version(configured_embedder)` resolved at server/worker startup (http.rs:47, mcp.rs:162, main.rs:502). So the instant an operator restarts the server configured with embedder B — even after a *failed* reembed — every search runs against version B and matches only the fraction of memories/canonicals that got embedded.
- **Root cause**: The module trusts an "insert version → backfill → flip active" contract (its own header, and migration 0001's `is_active`), but `ensure_embedding_version` eager-activates at creation and nothing consults `is_active`; reembed has no post-completion gate, so "servable" and "fully backfilled" are decoupled.
- **Impact**: A partially-reembedded corpus silently returns incomplete results — memories with no version-B row simply don't appear (`embedding_version_id = B` filter), and canonicals missing version-B embeddings blind cross-team resolution (`nearest_canonical`). No error, no warning: success theater over a corrupted retrieval surface. Worst on the large corpora where backfill is most likely to be interrupted.
- **Fix sketch**: Don't let `reembed` create-and-activate in one step. Either (a) write into an inactive version and add an explicit `activate_version(version_id)` that flips `is_active` and demotes the prior — only callable after both loops complete — and make the serve path (`ensure_embedding_version`-callers) resolve the version via `is_active` instead of re-deriving it; or (b) record a `backfilled_at`/expected-row-count on `embedding_versions` and refuse to serve a version whose backfill is incomplete. At minimum, return/log a completeness assertion at the end of `reembed`.

## 2. Divergence clustering double-counts positions — the entity→canonical join multiplies one memory into many, then the 8-cap drops the real divergent team
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: correctness
- **File**: crates/brainiac-pipeline/src/divergence.rs:91-138
- **Scenario**: The gather query joins `memories → memory_entities → entities → entity_links → canonical_entities` with no `DISTINCT`. A single memory that mentions two surface forms of the same thing (e.g. "psp-gateway" and "PSP") has TWO entities, both linked to the one canonical — so that memory yields two identical rows, and each becomes a separate `Position` (lines 112-119). This is the system's *normal* alias case (see the alias test in pipeline_pg.rs), not an edge case. Then `positions.iter().take(MAX_POSITIONS_PER_CLUSTER=8)` (line 135) feeds the adjudicator: duplicate copies of one team's memory crowd out other teams' genuinely-different positions past the 8-cap.
- **Root cause**: The query treats "position" as a join row rather than as a distinct (memory) fact; canonical-anchoring intentionally fans a memory across its aliased entities, and that fan-out is never collapsed before building the LLM prompt.
- **Impact**: The adjudicator sees repeated, lopsided input: real divergences get truncated out of the top-8 (false negatives — the whole point of the sweep missed), and repetition of one team's wording biases the verdict. Token spend is inflated by duplicates. The `teams.len() < 2` guard still passes (it dedups teams), so nothing catches it.
- **Fix sketch**: `SELECT DISTINCT` on `(m.id, t.name, m.kind, m.content)` (or `GROUP BY m.id`) so each memory contributes exactly one position; dedup `Position`s by `memory_id` before `.take(MAX_POSITIONS_PER_CLUSTER)`. Consider ordering positions to guarantee cross-team coverage under the cap rather than arbitrary row order.

## 3. Reembed validates batch COUNT but not vector DIMENSION — a wrong-dim vector is stored, then silently excluded from every search
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: dimension-mismatch
- **File**: crates/brainiac-pipeline/src/reembed.rs:90-98, 121-129
- **Scenario**: `anyhow::ensure!(vecs.len() == rows.len(), …)` checks only that the embedder returned the right *number* of vectors, not that each is `embedder.dim()` long. `upsert_embedding` writes into `memory_embeddings.embedding`, which is deliberately typmod-free ("multiple dims can coexist", migration 0001) — so a vector of the wrong length inserts with no DB error. The remote embedder further chunks each batch to its own per-request cap (header comment: Qwen 10), exactly the seam where a misaligned/partial response could return a count-correct but length-wrong vector.
- **Root cause**: The version's declared dim (`embedder.dim()`, line 61) and the actual stored vector length are never cross-checked; the count assertion gives false confidence that the batch is well-formed.
- **Impact**: `search_vector` constrains `vector_dims(e.embedding) = {dim}` (memories.rs:204) and `nearest_canonical` casts to the version dim — a wrong-dim row is silently never a candidate. The memory looks reembedded (has a version-B row, satisfies `missing_embedding`'s `NOT EXISTS`, so resume won't retry it) yet is permanently unsearchable. Silent, unrecoverable recall loss with no signal.
- **Fix sketch**: After `embed_batch`, assert every vector's length equals `embedder.dim()` (`anyhow::ensure!(v.len() == dim)`) before upserting, in both the memories and canonicals loops. Optionally enforce the dimension at the storage layer for the target version.

## 4. Divergence re-scan silently ERASES prior verdicts when the adjudicator returns unparseable JSON
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: crates/brainiac-pipeline/src/divergence.rs:153-194
- **Scenario**: For each cluster, a non-JSON / unparseable adjudicator response hits `let Some(json) = … else { continue }` (154-156) or `let Ok(v) = … else { continue }` (157-159) — the cluster is dropped with no log and no counter. After the loop, `scan_org` unconditionally `DELETE FROM practice_divergences WHERE org_id = $1` (168) and re-inserts only `confirmed`. So a cluster that was a stored divergence last run, but whose adjudication returns junk this run, is deleted and never re-added — even though the underlying divergence still exists. (A provider *error* is safe: `?` at line 151 aborts before the DELETE; only a successful-but-malformed body triggers the erasure.)
- **Root cause**: The "atomic replace" (delete-all-then-insert-survivors) assumes every cluster is adjudicated successfully every run; the tolerant `continue` on parse failure quietly shrinks the survivor set instead of preserving the prior row or aborting.
- **Impact**: One flaky/malformed LLM response per re-scan silently removes a real, previously-surfaced divergence from the platform-lead's list, and `stats.divergences` under-reports it — a disappearing finding with zero diagnostics.
- **Fix sketch**: Count parse failures into `DivergenceStats` and `tracing::warn!` each skip; and either upsert per-cluster (preserving a cluster's prior row when this run couldn't adjudicate it) or abort the replace if the failure rate exceeds a threshold rather than committing a lossy DELETE.

## 5. The full TRUNCATE statement is copy-pasted 4× inline despite the `truncate_all` helper — and the two test files' lists have already drifted
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-pipeline/tests/pipeline_pg.rs:122-130, 557-565, 737-745, 1367-1375 (helper at 906-916); crates/brainiac-pipeline/tests/reembed_pg.rs:101-109
- **Scenario**: `truncate_all` (pipeline_pg.rs:906) exists precisely to centralize the ~18-table `TRUNCATE … CASCADE`, yet four tests in the same file still inline a byte-identical copy instead of calling it, and `reembed_pg.rs` carries its own near-copy. The lists have already diverged: reembed_pg's includes `canonical_entity_embeddings`; pipeline_pg's copies omit it and rely on FK CASCADE from `canonical_entities`. Adding/renaming a table means editing 5+ locations, and a missed one either leaves cross-test residue or (once a table stops cascading) a silent seed leak between tests.
- **Root cause**: The helper was introduced later and only retrofitted onto some tests; the two test binaries can't share code trivially, so the statement was hand-duplicated.
- **Impact**: Maintenance hazard and correctness drift risk in the DB-gated suite; the very inconsistency (one list carries `canonical_entity_embeddings`, the others don't) is the kind of latent divergence that produces flaky cross-test contamination.
- **Fix sketch**: Route all four inline `TRUNCATE`s through `truncate_all`, and lift the table list into one shared source (e.g. a `const` or a helper in a small `tests/common` module / `#[path]`-included file) used by both `pipeline_pg.rs` and `reembed_pg.rs` so there is a single authoritative reset list.
