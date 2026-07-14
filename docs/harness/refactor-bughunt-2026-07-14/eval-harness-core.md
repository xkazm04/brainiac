> Context: Eval: Harness Core + Retrieval/Grid Profiles
> Total: 5 (Critical: 0, High: 2, Medium: 3, Low: 0)

## 1. "Superseded in top-3" hard gate is fed only by hand-annotated `forbidden_top3`, never by the seeded supersession graph
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: gate-coverage
- **File**: crates/brainiac-eval/src/retrieval_profile.rs:129-138 (and report.rs:180-185)
- **Scenario**: A gold memory is superseded (`status: deprecated`, `superseded_by`/`valid_to` set — the seed writes all of these, seed.rs:133-142) and the retrieval engine surfaces it in the top-3 of a *current-time* (no-`as_of`) query. Unless a QA fixture item happens to list that memory id under `forbidden_top3`, `superseded_in_top3` stays 0 and `RetrievalReport::gate_failures()` returns empty — the build ships green.
- **Root cause**: The hard gate counts only fixture-declared `forbidden_top3` hits (v1 has exactly 3 such annotations across all 54 QA queries). The supersession data that is actually seeded into the DB — `superseded_by`, `valid_to`, `status=deprecated` — is never read back to derive the forbidden set. `asof.yaml`'s own header declares a two-part metric ("exact hit at rank 1; superseded memories absent from top-3 on current-time queries"), but the temporal loop (retrieval_profile.rs:194-273) implements only the rank-1 half; the top-3 half is delegated entirely to those 3 manual annotations.
- **Impact**: The single hard invariant that guards temporal correctness (a deprecated fact resurfacing as if current is exactly the corruption this engine exists to prevent) is only as complete as 3 hand-maintained strings. Add a new supersession pair to the fixtures without also editing `forbidden_top3` and the regression is invisible. This is a gate that passes when quality regressed.
- **Fix sketch**: Derive the forbidden-current set from the seeded memories: for each memory with `superseded_by` set (or `valid_to < now`), assert it is absent from the top-3 of any current-time query that returns its successor, and fold that into `superseded_in_top3`. Keep `forbidden_top3` as an additional explicit layer, not the sole source.

## 2. Negative-stratum refusal failures are recorded as diagnostic violations but no gate can ever consume them
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-gate / success-theater
- **File**: crates/brainiac-eval/src/retrieval_profile.rs:139-145, 339-347 (and report.rs:171-188, gates.rs:78-151)
- **Scenario**: A negative (out-of-scope) query — 6 exist in v1, all with `relevant: []` (qa-080..083 etc.) — starts returning junk hits after an embedder/reranker change. retrieval_profile.rs:140 pushes the violation `"negative query returned N hits (expected none)"` and the query's diagnostic gets `pass=false`. But `RetrievalReport::gate_failures()` checks only `rls_leaks` and `superseded_in_top3`, and `regression_failures()` checks only NDCG/temporal/thesis. Neither looks at negative behavior or at `negative_empty_rate`. The build passes.
- **Root cause**: The harness clearly *intends* negative-returns-hits to be a failure (it manufactures a violation and flips `pass`), but `negative_empty_rate` was wired as an informational field only, and the per-query `violations`/`pass` flags feed the drill-down artifact — not the gates. The diagnostic's verdict and the gate's verdict are structurally disconnected.
- **Impact**: A memory engine that silently loses its refusal behavior (answers questions it has no memory for) reports failing per-query diagnostics while returning a green build — the worst kind of success theater, because the artifact says "fail" and CI says "pass". Refusal quality is a headline property of the product and is currently ungated in every path.
- **Fix sketch**: Either add `negative_empty_rate` to the soft gate in gates.rs (regress vs a baseline floor, e.g. must stay within a delta below committed), or promote the negative-returns-hits violation into `gate_failures()` as a hard/soft failure. At minimum, make the aggregate gate consume the same signal the diagnostics already compute.

## 3. Thesis-check gate is silently skipped whenever either compared stratum has `None` NDCG
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: silent-gate
- **File**: crates/brainiac-eval/src/gates.rs:132-148
- **Scenario**: The "graph expansion earns its keep" thesis gate runs only inside `if let (Some(cross), Some(semantic)) = (...)`. If either the `cross_team_graph` or `semantic` stratum resolves to `ndcg_at_10 == None` — which happens when every query in that stratum loses its graded relevance (all grades 0 → `ndcg_at_k` returns `None`, metrics.rs:37) or the stratum is renamed — the entire thesis check is skipped and contributes zero failures.
- **Root cause**: `if let (Some, Some)` treats "can't compute" as "nothing to check" rather than "can't prove the thesis, so fail". The per-stratum "present in baseline but missing from run" guard (gates.rs:110-120) only catches a *renamed/dropped* stratum, and only if it was in the baseline; a stratum that still exists but degrades to `None` NDCG slips through both guards.
- **Impact**: The gate that validates the project's core hypothesis (cross-team graph retrieval beats pure-semantic by the §3.2 margin) can pass by default precisely when the data needed to evaluate it has degraded — the thesis is asserted-by-absence.
- **Fix sketch**: Make a missing/`None` value on either side an explicit breach ("thesis check could not be evaluated: cross_team_graph or semantic NDCG is unavailable") instead of a silent skip, so the gate fails closed.

## 4. `negative_empty_rate` returns 1.0 (perfect refusal) for an empty negative stratum
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: empty-set-metric
- **File**: crates/brainiac-eval/src/retrieval_profile.rs:343-347
- **Scenario**: When the negative stratum contains zero queries (a plausible state for the v2 fixture tree or any reduced corpus), `negative.is_empty()` is true and the metric is hard-coded to `1.0` — a *perfect* refusal score — rather than `0.0`/`None`. This is the classic "empty result set yields 1.0 instead of 0.0" trap: absence of evidence is scored as maximal success.
- **Root cause**: The `if negative.is_empty() { 1.0 }` default optimistically assumes "nothing to refuse = refused everything". Compare the temporal metric one screen down (retrieval_profile.rs:394-398), which correctly fails safe with `0.0` when `temporal_total == 0`. The two "no data" defaults point in opposite directions.
- **Impact**: Latent today (v1 has 6 negatives) but a landmine: if this field is ever gated (see finding 2's fix), or if a fixture revision drops the stratum, refusal quality silently reads as flawless. It also makes the JSON report claim 100% refusal for corpora that measured nothing.
- **Fix sketch**: Return `Option<f64>` (`None` when the stratum is empty) so it can't be mistaken for a real score, or default to `0.0` to fail safe consistently with the temporal metric. Whichever is chosen, align both "no data" defaults so absence is never scored as success.

## 5. TRUNCATE table-list is triplicated across the driver and both PG tests, and has already drifted
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication / drift-risk
- **File**: crates/brainiac-eval/src/grid.rs:37-41; crates/brainiac-eval/tests/retrieval_pg.rs:27-34; crates/brainiac-eval/tests/grid_pg.rs:14-22
- **Scenario**: The same ~18-table `TRUNCATE ... CASCADE` reset list is hand-written in three places, and they no longer agree: `grid.rs` and `grid_pg.rs` include `canonical_entity_embeddings`, `queue.jobs`, and `queue.archive`; `retrieval_pg.rs` omits all three. On a shared database (these suites explicitly serialize on one DB), a divergent reset list is exactly how one suite leaks rows into the next.
- **Root cause**: Copy-paste of the reset SQL instead of a single shared constant/helper, so each site evolved independently as new tables were added. (The identity+entity seeding block in seed.rs:50-72 vs 85-109 is the same copy-paste smell in the seed layer.)
- **Impact**: The `retrieval_pg` omission is probably masked today by `CASCADE` from `canonical_entities` and by retrieval not touching the queue tables — but the moment an FK/cascade changes or a pipeline run shares the DB, `retrieval_pg` starts from a dirty tenant and produces non-deterministic scores. Three lists guarantee the next table addition lands in some but not all of them.
- **Fix sketch**: Hoist one authoritative reset (e.g. a `pub const TENANT_TABLES` or a `reset_tenant(admin)` helper in the eval crate or brainiac-store) and call it from grid.rs and both tests; delete the two divergent copies. While there, extract the duplicated identity+entity seed block in seed.rs into a shared `seed_identity_and_entities`.
