> Context: Core: Fusion + Temporal + Metrics + Scoring + Rerank
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## 1. Cyclic supersession chains defeat dedupe — a superseded memory and its successor coexist
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: temporal-invariant
- **File**: crates/brainiac-core/src/temporal.rs:37-50, 58-99
- **Scenario**: Corrupt/backfilled data forms a supersession cycle, e.g. memory `1.superseded_by = 2` and `2.superseded_by = 1`. `chain_head(m1)` walks `1→2`, hits `1` again (already in `seen`), breaks, and returns node `2`. `chain_head(m2)` walks `2→1`, breaks, and returns node `1`. So `chain_of[1] = 2` and `chain_of[2] = 1` — the two nodes of one cycle resolve to *different* heads. In `dedupe_for_time`, each is therefore treated as a separate single-member chain; if both are valid at `as_of`, both survive.
- **Root cause**: `chain_head` is cycle-*terminating* (it stops the walk) but not cycle-*canonical* — the head it returns depends on the start node, so members of a cycle are not grouped together. The module doc (lines 5-8) advertises the invariant "a retrieval result must never contain both a superseded memory and its successor," and the `chain_head_is_cycle_safe` test only asserts termination, not correct grouping.
- **Impact**: Retrieval silently returns two contradictory versions of the same fact for one point in time — exactly the failure the temporal layer exists to prevent — and does so without any error. The reassuring "cycle-safe" comment is success theater: only the hang is prevented, not the wrong result.
- **Fix sketch**: Make the head canonical for a cycle: when the walk detects a revisit, pick a deterministic representative of the visited set (e.g. the min `Uuid` among `seen`, or the node whose id sorts first) and return that for every start node in the cycle. Then all cycle members share one `chain_of` entry and collapse to a single winner. Add a cyclic-data test asserting `out.len() == 1`.

## 2. Plain RRF is a verbatim duplicate of weighted RRF
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-core/src/fusion.rs:16-44, 54-84
- **Scenario**: `reciprocal_rank_fusion` (lines 16-44) and `weighted_reciprocal_rank_fusion` (lines 54-84) are byte-for-byte identical except that the weighted version multiplies the contribution by `weights.get(li).unwrap_or(1.0)`. The HashMap accumulation, `first_seen` bookkeeping, the `sort_by` comparator (score-desc then first-seen), and `truncate(top)` are copy-pasted. The doc on line 47 even states plain RRF "is identical to `reciprocal_rank_fusion` when every weight is 1.0, so it is a strict generalization," and the test `weighted_unit_weights_equal_plain_rrf` proves it.
- **Root cause**: Two entry points were wanted (an unweighted contract plus a weighted one) and the second was authored by cloning the first rather than delegating.
- **Impact**: ~30 lines duplicated in the numerical core; the tie-break comparator and any future fix (e.g. finding 3's input hardening) must be edited in two places, and they can silently drift apart.
- **Fix sketch**: Make `reciprocal_rank_fusion(lists, k, top)` a one-line call to `weighted_reciprocal_rank_fusion(lists, &[], k, top)` — the `weights.get(li).unwrap_or(1.0)` default already makes an empty/short weight slice behave as all-ones (the `missing_weight_defaults_to_one` test covers this). Delete the duplicated body.

## 3. Fusion accepts unvalidated `k`/`weights`; NaN/Inf scores are then silently sorted as ties
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: numeric-edge / silent-failure
- **File**: crates/brainiac-core/src/fusion.rs:27, 37-40, 67, 77-80
- **Scenario**: `contribution = 1.0 / (k + (rank0 + 1.0))`. Nothing constrains `k`: a caller passing `k <= -1.0` makes the rank-0 denominator `0.0`, yielding `+Infinity` for that item; a `NaN` `k` or a `NaN`/`Inf` entry in `weights` propagates `NaN` into a fused score. The comparator `b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)` then maps any `NaN` comparison to `Equal`, so a `NaN`-scored item is silently treated as tied with everything and placed purely by `first_seen` — no panic, no signal. (The live caller in retrieval.rs uses `k=60` and weights `1.0/2.0`, so this is latent, not currently firing — but these are `pub` core APIs on the retrieval hot path.)
- **Root cause**: RRF's single tunable `k` is documented as "conventionally 60" but the function trusts the caller to keep the denominator positive and finite, and the defensive `unwrap_or(Equal)` prioritizes "never panic" over surfacing a corrupt score.
- **Impact**: An out-of-contract `k` or a future weight source (config, per-query tuning) can inject `Inf`/`NaN` that either lets one item dominate or scrambles ordering while looking healthy. Because fusion is the relevance backbone, a `NaN` here corrupts the entire ranked page invisibly.
- **Fix sketch**: `debug_assert!(k > -1.0)` (or clamp/return an error) so the denominator stays positive; after summing, treat a non-finite fused score as the lowest rank (or `filter` it out) instead of letting `unwrap_or(Equal)` bury it. Guarding once in the shared helper (finding 2) covers both entry points.

## 4. `dedupe_for_time` picks the winner by `valid_from`, mis-selecting when the successor has an open start
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: temporal-correctness
- **File**: crates/brainiac-core/src/temporal.rs:70-92
- **Scenario**: When two members of one chain are both valid at `as_of` (overlapping windows), the winner is "newest `valid_from`" (lines 82-86). Consider chain `a.superseded_by = b` where the successor `b` was written with `valid_from = None` (open start — a plausible sloppy/automated write) and predecessor `a` has `valid_from = Some(Jan)`, with overlapping validity at `as_of`. The tie-break `match (m.valid_from, existing.valid_from)` returns `(None, Some) => false` and `(Some, None) => true`, so the concrete-dated predecessor `a` is judged "newer" than the open-start successor `b`. The superseded memory `a` wins; the current memory `b` is dropped.
- **Root cause**: `valid_from` is used as a proxy for "which chain member is latest," but `None` semantically means "-∞," so an open-start successor is treated as the *oldest*, inverting the supersession direction. The code never consults the actual chain topology (`superseded_by` order) when breaking the tie.
- **Impact**: Retrieval surfaces the stale/retired version of a fact instead of its replacement — the opposite of the intended "newest member wins" — whenever a successor lacks an explicit `valid_from`. Silent; no test exercises a `None`-`valid_from` successor.
- **Fix sketch**: Break the winner tie by chain position, not by `valid_from`: prefer the member closer to the chain head (or the head itself among the valid ones), and only fall back to `valid_from` when positions are equal. At minimum, treat a `None` `valid_from` as "later" for a known successor rather than "-∞."

## 5. `recall_at_k` counts ranked positions, not distinct items — can exceed 1.0
- **Severity**: Low
- **Lens**: bug-hunter
- **Category**: metric-correctness
- **File**: crates/brainiac-core/src/metrics.rs:62-77
- **Scenario**: `hit = ranked.iter().take(k).filter(|i| relevant.contains(i)).count()` then `hit / relevant.len()`. If `ranked` contains a duplicate of a relevant item within the top-k (e.g. `ranked = ["x","x"]`, `relevant = {x}`), each occurrence is counted: `hit = 2`, `relevant.len() = 1`, returning `recall = 2.0`. More generally any duplicated relevant hit inflates recall above the true fraction of distinct relevant items found.
- **Root cause**: The count is over ranked slots rather than the set of distinct relevant ids covered; the function assumes `ranked` is duplicate-free but does not enforce or document it. (The live pipeline dedupes before eval, so this is latent — but the module doc bills these metrics as "shared by the eval harness and any future runtime analytics.")
- **Impact**: A metric that gates CI (NDCG/MRR/Recall regression gates) can report recall > 1.0 or otherwise overstate coverage on duplicated input, masking a retrieval regression.
- **Fix sketch**: Count distinct relevant hits: `ranked.iter().take(k).collect::<HashSet<_>>().intersection(&relevant).count()`, or dedupe the top-k before filtering. Same care applies if `reciprocal_rank`/`dcg_at_k` are ever fed non-unique rankings.
