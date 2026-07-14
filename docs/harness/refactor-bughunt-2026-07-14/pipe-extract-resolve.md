> Context: Pipeline: Extract + Resolve + Contradict + Policy
> Total: 5 (Critical: 0, High: 3, Medium: 2, Low: 0)

## 1. Auto-promotion ignores a just-opened contradiction — conflicting knowledge enters retrieval with no human gate
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: promotion-policy
- **File**: crates/brainiac-pipeline/src/policy.rs:11-42 (call site: crates/brainiac-pipeline/src/worker.rs:519-554)
- **Scenario**: A source yields a new team-visible pitfall (confidence ≥ 0.9) or an explicit decision (confidence ≥ 0.95) whose claim directly conflicts with an existing canonical/candidate memory. In `process_job` the worker runs `run_contradict` for that memory (opening a contradiction row), then immediately calls `engine.evaluate(m, Candidate)` and, on `AutoApproved`, flips the memory to `Candidate` in the same transaction. `PolicyEngine::evaluate` takes only `(&Memory, to)` — it has no input for "a contradiction was just detected against this memory," so the fresh, conflicting memory auto-promotes anyway.
- **Root cause**: Contradiction detection is deliberately advisory (governance-path-only, `contradict.rs:44-49`), but the promotion policy was designed as a pure function of the memory's own fields and confidence. The two signals are never joined, even though the worker holds `c.opened` at the exact point it calls `evaluate`.
- **Impact**: Candidate memories are retrievable (retrieval excludes only `rejected`; `search_vector` line 205 / default filters keep candidate+canonical). So a model-hallucinated or stale high-confidence claim that contradicts established org knowledge is promoted into the trusted tier and surfaces in answers org-wide — before any human resolves the queued contradiction. This is "promote when it should hold": poisoned/conflicting knowledge circulates with no human in the loop. (Confidence is also the model's own self-report from extraction, so a model can auto-promote its own output by claiming ≥ 0.9.)
- **Fix sketch**: Thread a "conflicts detected" signal into the policy seam — e.g. `evaluate(&Memory, to, ctx: &PolicyContext { open_contradictions: usize })` — and add a guard rule `if ctx.open_contradictions > 0 { return (NeedsReview, "contradiction_pending"); }` ahead of the auto-approve branches. The worker already has `c.opened`; pass it in.

## 2. Any valid JSON object without a `memories` key is accepted as "0 extracted" — repair loop bypassed, silent knowledge loss
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-llm-parse-failure
- **File**: crates/brainiac-pipeline/src/extract.rs:340-362 (with 84-88, 396-402)
- **Scenario**: The extractor returns a syntactically valid JSON object that is *not* the expected shape — a refusal wrapper `{"refusal":"I won't do that"}`, a reasoning wrapper `{"result":{...}}`, or `{"status":"ok","data":[]}`. `extract_json_object` returns it, `serde_json::from_str::<ExtractionOutput>` succeeds because `memories` is `#[serde(default)]` (absent → empty vec, the custom deserializer is never invoked), and `parse_extraction` returns `Ok(ExtractionOutput{ memories: [] })`. `extract_once` sees `Ok`, returns `repaired: false`, and the chunk records **zero** memories.
- **Root cause**: The repair loop only fires on a hard parse *failure* (`parse_extraction` → `Err`). A valid-but-wrong object is not a failure, so nothing distinguishes "model emitted a well-formed non-answer" from "genuinely nothing to extract." `ExtractionOutput` has no `deny_unknown_fields` and no required `memories`, so the wrong shape parses cleanly to empty.
- **Impact**: A transcript full of durable knowledge is silently dropped whenever the model wraps/refuses in valid JSON. No repair re-prompt, no error, no dead-letter; `parse_failures`/`repairs` stay 0 and the run row reports `ok` with 0 memories — indistinguishable from an empty transcript. Given the module's own note that Qwen failures are largely stochastic, this defeats exactly the retry mechanism built to recover them.
- **Fix sketch**: Treat "valid JSON but no `memories` key present" as a parse failure that drives a repair, not a clean empty. Deserialize into a shape where the key's presence is observable (e.g. `memories: Option<Vec<..>>`, or check the raw object for the key) and return `Err("response had no `memories` field")` so the repair re-prompt (and its final escape hatch) engages instead of silently yielding zero.

## 3. Lexical alias fast-path false-merges distinct entities at confidence 1.0 with no adjudication or kind check
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: entity-resolution / false-merge
- **File**: crates/brainiac-pipeline/src/resolve.rs:76-96 (with `record_aliases` 52-62; store `find_canonical_by_name_or_alias` entities.rs:279-306, `accumulate_canonical_aliases` 308-331)
- **Scenario**: `resolve_entity`'s first branch links a new entity to a canonical whenever its name OR any captured alias exactly matches (case-insensitively) the canonical's name or **accumulated alias set** — writing `link(..., 1.0, "alias_lexical")` and returning `Linked`, with no embedding check, no model adjudication, and no `entity_kind` comparison. Aliases in that set are model-proposed surface forms (`clean_aliases` only trims/dedups/drops self-name; it does not guard genericness or hallucination). One overbroad alias — a model emitting `"api"`, `"gateway"`, `"v2"`, or a hallucinated synonym — pollutes the canonical via `accumulate_canonical_aliases`, after which every future entity bearing that surface form is instantly merged. Two teams legitimately using the same short name for different things (e.g. "PSP") merge identically.
- **Root cause**: The fast-path was added so cross-team acronyms resolve without hand-seeded aliases, trading adjudication for exactness. But "exact match against an ever-growing, model-authored alias pool" is not the same guarantee as "exact match against a canonical name," and the branch skips the very adjudicator (`ADJUDICATE_SYSTEM_PROMPT_V1`, whose whole job is "a repo is not the model it trains") that the module relies on to prevent merges.
- **Impact**: A false merge collapses two distinct real-world entities into one canonical — the module's own stated zero-tolerance failure (resolve.rs:1-7). Memories from unrelated entities then co-mingle under one canonical id and surface together in cross-team graph retrieval, and bogus `depends_on`/`owns` edges propagate. It is silent (no review row) and cascades (each merge widens the alias pool that swallows the next entity).
- **Fix sketch**: Restrict the lexical fast-path to matches on the canonical **name** (and hand-vetted aliases), and require `entity_kind` agreement before auto-linking. For alias-only matches, drop into the adjudication band instead of auto-linking at 1.0. Alternatively, tag model-captured aliases as "unverified" and exclude them from the auto-link predicate until confirmed.

## 4. Contradiction verdict matched with a brittle `== "supersede"` — capitalized/padded relations silently drop real conflicts
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: contradiction-false-negative
- **File**: crates/brainiac-pipeline/src/contradict.rs:106
- **Scenario**: The verdict's relation is compared with a raw, exact `verdict.relation == "supersede"`. If the model returns `"Supersede"`, `"SUPERSEDE"`, or `" supersede"` (leading space, trailing period, etc.) — variations models emit even under a lowercase-only prompt — the equality fails and the branch falls through to the `coexist/dismiss` path, which writes no row ("silence is the correct output").
- **Root cause**: Unlike extract (`MemoryKind::parse`, tolerant coercion) and resolve (typed `bool`), the contradiction relation is kept as a free `String` and compared byte-exact against one literal. There is no normalization (trim/lowercase) and no "unrecognized relation" branch.
- **Impact**: A genuine contradiction is silently discarded whenever the model's casing/whitespace drifts — a false negative in the crown-jewel contradiction path. Because coexist/dismiss are also the "no row" outcomes, an unrecognized relation is indistinguishable from a correct dismissal; nothing is counted or logged. Over-flagging was engineered against; this is the opposite, invisible failure.
- **Fix sketch**: Normalize before matching — `match verdict.relation.trim().to_ascii_lowercase().as_str() { "supersede" => …, "coexist" | "dismiss" => …, other => tracing::warn!(?other, "unrecognized contradiction relation") }` — so casing/whitespace can't drop a conflict and a truly unknown verdict is surfaced rather than swallowed.

## 5. `dropped_invalid` is counted in four places but never surfaced — silent discard + dead field
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: dead-code / silent-failure-observability
- **File**: crates/brainiac-pipeline/src/extract.rs:181, 535, 543, 640, 648
- **Scenario**: `ExtractStats::dropped_invalid` is incremented when a memory has an unparseable kind (535), empty content after redaction (543), a relation naming an unlisted entity (640), or an invalid relation kind (648). But `run_extract`'s only production caller (`worker.rs:469-489`) reads `memories_written`, `entities_created`, `chunks`, `parse_failures`, `repairs`, `deduped`, `model_ref`, `memory_ids` — and never `dropped_invalid`. It is absent from `RunStats` and from the `pipeline_runs` INSERT (worker.rs:402-428). The field is read only in tests.
- **Root cause**: The counter was added as a validation-firewall signal but never wired into the run-stats fold or the observability row, so it accumulates into a field with no live reader.
- **Impact**: Two-fold. (bug) A model emitting all-invalid kinds (or relations) has every memory/edge silently dropped; the run reports `ok` with `memories_written = 0`, indistinguishable from an empty transcript — an operator cannot see that extractions are being gutted. (cleanup) The field is genuinely unused outside tests: either dead weight to remove, or (better) a real signal to plumb through.
- **Fix sketch**: Add `dropped_invalid` to `RunStats` and the `pipeline_runs` INSERT (mirroring `deduped`/`parse_failures`) and fold it in `process_job`, so silent drops are visible; or, if intentionally unobserved, delete the field and its four increments. Surfacing is the better choice given the silent-loss risk.
