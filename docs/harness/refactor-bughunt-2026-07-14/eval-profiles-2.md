> Context: Eval: Docs/Contradiction/Extraction/Resolution/Pipeline Profiles
> Total: 5 (Critical: 0, High: 2, Medium: 3, Low: 0)

## 1. Zero-tolerance leak gate only inspects prose sentences — a forbidden fact in a heading or code fence walks straight through
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: gate-blind-spot
- **File**: crates/brainiac-eval/src/docs_profile.rs:448-478 (with prose_sentences 209-243, sent_vecs 394,403-410)
- **Scenario**: A composed page renders a forbidden team-private fact inside a fenced ```code block, a `#` heading, or an `<sub>` evidence footer, WITHOUT citing the memory (so its id never lands in `rev.composed_from`). The provenance half of the gate (line 451, `composed_from.contains(&fmid)`) misses it because there is no citation; the "belt-and-braces" semantic half (lines 462-477) embeds `gold.content` and compares it against `sent_vecs`, but `sent_vecs` are the embeddings of `sentences = prose_sentences(&md, &pinned)`, and `prose_sentences` *explicitly discards* every fenced-code line (`in_fence`), every heading (`t.starts_with('#')`), every `<sub>` line, and all pinned content (226-232, 210-213). The forbidden fact was never embedded, so `best` stays ~0 and no `Leak` is pushed.
- **Root cause**: `prose_sentences` was designed for the *hallucination* metric, where scaffolding must be excluded so the model is not charged for text it did not author. The leak check was then layered onto the same sentence set — but a breach detector has the opposite requirement: it must scan *everything the page renders*, scaffolding included, because a leak in a heading or a config snippet is still a leak.
- **Impact**: The gate the module's own docstring calls "ZERO TOLERANCE, build failure … not a quality regression, it is a breach" has a silent evasion path. A private runbook value paraphrased into a code block or a section heading auto-publishes to an org page while the gate reports clean — the exact breach the semantic half was added to prevent.
- **Fix sketch**: Run the leak similarity check against the full rendered markdown (e.g. embed `strip_citations(&md)` split on lines/sentences with only fences *unwrapped rather than dropped*, plus heading text), separately from the hallucination sentence set. At minimum, additionally embed heading text and code-fence bodies and fold them into the `best` max for each forbidden memory.

## 2. Auto-published-hallucination hard gate checks for the `[m:` substring, not that the citation backs the sentence (and mis-splits trailing citations)
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: success-theater-gate
- **File**: crates/brainiac-eval/src/docs_profile.rs:395,398-400 (gate 148-154; splitter 234)
- **Scenario**: Two independent failures of the same hard gate. (a) FALSE NEGATIVE — a sentence counts as "cited" iff it literally contains `[m:` (line 395). A model that fabricates a claim and appends *any* citation token — `The retry cap is 45s [m:unrelated-real-memory].` — passes as backed, so a hallucinated sentence with a bogus/irrelevant citation auto-publishes. (b) FALSE POSITIVE — sentences are cut with `split_inclusive(['.','!','?'])` (234); a genuinely-cited claim whose citation trails the period, `The cap is 30s. [m:abc]`, splits into `"The cap is 30s."` (no `[m:` → counted uncited) and `" [m:abc]"` (len 8 < 15 → dropped), so a correctly-cited page trips the build-failure gate.
- **Root cause**: The check equates "has a citation token somewhere in the sentence" with "this claim is backed by a memory." It verifies citation *syntax and position*, never citation *validity* — whether the cited memory actually supports the sentence. Nothing embeds the sentence against its cited memory.
- **Impact**: `auto_published_hallucinations > 0` is a hard build-failure gate (148-154) whose stated purpose is that an auto-published revision "may not" carry an unbacked sentence. In the false-negative direction it green-lights hallucinations that carry a fabricated citation (real LLMs fabricate citations); in the false-positive direction it blocks legitimate releases over citation placement. Both directions make the gate untrustworthy.
- **Fix sketch**: For backing, verify each cited `[m:id]` resolves to a memory whose embedding is within `MATCH_THRESHOLD` of the sentence (reuse the coverage cosine machinery) rather than trusting the token's presence. For placement, attach a trailing `[m:…]` to the preceding sentence before the punctuation split (or count a citation that immediately follows the terminator as belonging to the prior sentence).

## 3. `must_mark_unshipped` is a doc-global substring scan, not section-scoped — any incidental "will "/"planned" marks every unshipped section satisfied
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: metric-correctness
- **File**: crates/brainiac-eval/src/docs_profile.rs:435-445
- **Scenario**: For each section with `must_mark_unshipped`, the code lowercases the ENTIRE document (`let lower = md.to_lowercase()`) and marks the requirement satisfied if the whole page contains `"not yet"`, `"planned"`, `"will "`, or `"in progress"` anywhere. A page whose roadmap section correctly says "planned," but which states a *different* section's unshipped decision as current fact, still scores `unshipped_marked += 1` for the failing section. Worse, `"will "` matches any incidental future-tense prose — "engineers will be paged," "this will change" — so nearly every non-trivial page satisfies the requirement unconditionally.
- **Root cause**: The unshipped signal was implemented as a cheap keyword presence test over `md`, and it is evaluated at document scope even though the requirement (`s.must_mark_unshipped`) is per-section. `md` is even re-lowercased once per qualifying section inside the `for s in &d.sections` loop (redundant work).
- **Impact**: `unshipped_marked` / `unshipped_required` is a reported quality signal (KB-PLAN D2, "the most common way a wiki lies") that is systematically inflated toward `marked == required` on essentially every run — success theater. It is not currently wired into `regression_failures`, which caps the blast radius to a misleading report field, but any future gate built on it would pass while the underlying property fails.
- **Fix sketch**: Scope the scan to the section's rendered text, not the whole page, and match a marker that carries intent (a "⚠ not yet shipped"/"planned" badge the composer emits) rather than the bare substring `"will "`. Hoist the single `to_lowercase()` out of the section loop.

## 4. `cosine`, `ratio`, and the micro-F1 formula are copy-pasted across the profile files
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-eval/src/docs_profile.rs:180-192; extraction_profile.rs:168-182,375-379,402-408; pipeline_profile.rs:298-302,354-360; contradiction_profile.rs:565-571
- **Scenario**: `cosine(&[f32],&[f32]) -> f64` is byte-identical in `docs_profile` (180-192) and `extraction_profile` (168-182), including the `na==0.0||nb==0.0 → 0.0` guard. `fn ratio(num,den)` is a verbatim third-copy across `contradiction_profile` (565-571), `extraction_profile` (402-408), and `pipeline_profile` (354-360). The micro-F1 harmonic-mean expression (`if precision+recall==0.0 {0.0} else {2.0*p*r/(p+r)}`) is duplicated in `extraction_profile` (375-379) and `pipeline_profile` (298-302).
- **Root cause**: The five profiles were written in parallel, each self-contained; the shared numeric kernels were pasted rather than factored, and `brainiac_core::metrics` (which already owns `b_cubed`/`pairwise_prf`/`false_merge_count`) was never extended with the scalar helpers.
- **Impact**: Divergence risk on the exact code that computes the quality numbers — a future fix or threshold tweak to one `cosine`/`micro_f1` silently leaves the other profile on the old behaviour, producing non-comparable scores. Pure maintenance tax with a correctness downside.
- **Fix sketch**: Move `cosine`, `ratio`, and `micro_f1` into one crate-internal `eval::metrics` (or extend `brainiac_core::metrics`) module and import them from all five profiles.

## 5. The `truncate` test helper is duplicated across three PG tests — and the resolution copy silently diverges (omits the queue tables)
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication-drift
- **File**: crates/brainiac-eval/tests/contradiction_pg.rs:16-26; tests/pipeline_pg.rs:13-23; tests/resolution_pg.rs:22-29
- **Scenario**: `contradiction_pg` and `pipeline_pg` each define a byte-identical `async fn truncate(admin)` whose TRUNCATE lists the same 18 tables *including* `queue.jobs, queue.archive`. `resolution_pg` open-codes the same TRUNCATE inline (22-29) but its list STOPS at `pipeline_runs` and omits `queue.jobs, queue.archive`. All three tests share one `DATABASE_URL` Postgres instance.
- **Root cause**: Three hand-maintained copies of one truncation contract. The resolution test predates the queue-draining profiles (it calls `resolve_entity` directly, never `worker::tick`), so its author never added the queue tables — and no shared helper forced them back into sync.
- **Impact**: A resolution run leaves any rows a prior contradiction/pipeline run left in `queue.jobs`/`queue.archive` in place. Today resolution ignores the queue so it is latent, but it is a genuine cross-test-contamination hazard the instant resolution (or a shared fixture) touches the worker, and it is a maintenance trap: adding a new table to the "clean slate" requires remembering three edit sites, one of which is already wrong.
- **Fix sketch**: Extract a single `truncate(admin)` (or a `tests/common` module / fixture) with the complete table list and call it from all three tests, deleting the divergent inline copy.
