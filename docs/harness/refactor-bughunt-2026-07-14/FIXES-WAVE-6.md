# Fix Wave 6 — Eval-Gate Integrity

> 6 commits, 6 findings resolved (all High) — the gates that are supposed to catch
> everything else. Gates green: `cargo check --workspace --all-targets` 0 errors ·
> 113 DB-free unit tests pass (core 55, eval 20, gateway 4, pipeline 29, fixtures 5;
> +9 new). The `*_pg` profiles still need a live Postgres + LLM to run.

**Why this wave first:** Waves 1–5 changed retrieval, promotion, reembed and
temporal logic. The eval harness is what proves none of that regressed — a blind
gate can't validate the work. So the gates got fixed before the remaining themes.

## Commits

| # | Findings | Files |
|---|---|---|
| 1 | R14#1 missing pages.yaml voids the gates | `fixtures/loader.rs`, `tests/load_v1.rs` |
| 2 | R15#2 refusal behavior ungated | `eval/gates.rs` |
| 3 | R14#2 collision check covers 4 of 10 namespaces | `fixtures/validate.rs` |
| 4 | R16#1 leak gate scans only prose | `eval/docs_profile.rs` |
| 5 | R16#2 trailing citation mis-split (placement half) | `eval/docs_profile.rs` |
| 6 | R15#1 superseded gate not derived from the graph | `eval/retrieval_profile.rs` |

## What was fixed

1. **A missing `pages.yaml` silently voided the composition + leak gates (R14#1).** The loader treated it as optional and produced an empty list, so every composition-gold check and the zero-tolerance leak gate iterated zero items and validated GREEN — the "vacuous pass" validate.rs itself calls "worse than having no gate at all". Now a `documents/` directory declares the profile: if it exists but the file doesn't, the load fails. Added the missing `documents.len() == 2` assertion (nothing guarded it).

2. **Refusal behavior was ungated (R15#2).** The diagnostics flag a negative query that returns hits (`pass=false`), but no gate read that signal — the artifact said "fail" while CI said "pass". Wired `negative_empty_rate` into the regression gate. **Verified before fixing:** a hard gate would break the build instantly — the rate is **0.0 in every committed baseline**, i.e. the engine never refuses an out-of-scope query today. Landed as a ratchet (`#[serde(default)]`, harmless at 0.0, locks in quality once re-baselined).

3. **The uuid-collision check covered 4 of ~10 persisted namespaces (R14#2).** `stable_uuid` is a namespace-flat hash, so cross-type collisions silently overwrite rows at seed time; documents/transcripts/contradictions/temporal/qa/leak ids were all unchecked while the comment claimed full coverage. Chained all ten and switched to Vec iteration for deterministic, diffable output. The v1 corpus validates clean under the widened check.

4. **The zero-tolerance leak gate only scanned prose (R16#1).** It reused the *hallucination* sentence set, which deliberately drops fences, headings, `<sub>` footers and pinned content — so a forbidden fact restated in a heading or a config snippet (and never cited, so the provenance half missed it too) walked straight through. Added `leak_scan_segments` (fences unwrapped, heading/`<sub>` text kept) as a dedicated full-render scan; tests assert the precondition that the prose set really does drop all three evasion paths.

5. **A trailing citation failed a correctly-cited page (R16#2, placement half).** `"The cap is 30s. [m:abc]"` split into an uncited claim plus a dropped 8-char fragment, tripping a hard build-failure gate on citation placement. A fragment that is only citation tokens now merges into the sentence it trails.

6. **The superseded-in-top3 hard gate was fed by 3 hand-annotated strings (R15#1).** 6 gold memories carry `superseded_by`; only 3 `forbidden_top3` annotations exist, so adding a supersession pair without editing the annotations made a resurfaced deprecated fact invisible. Now derived from the seeded graph on every current-time query, with `forbidden_top3` kept as an extra layer. Safe to widen blind because the exclusion mechanism is universal (`valid_at(now)` drops any memory whose `valid_to` closed at supersession), and `superseded_in_top3` is 0 in every committed report.

## Deferred (with reason)

- **R16#2, validity half** — "a sentence counts as cited iff it contains `[m:`" is narrower than the finding reads: `compose.rs` already runs a **citation firewall** that requires each `[m:uuid]` be in the page's `allowed` set and strips invented ids, so the residual gap is citing an *allowed-but-irrelevant* memory. Closing it means making a HARD gate stricter on a similarity threshold that cannot be calibrated without a live DB + LLM — a mistuned cutoff fails legitimate releases, the same class of harm. Needs an eval run.

## Patterns established (catalogue items 13–15)

13. **An optional input that feeds a gate must fail loudly when absent.** "File missing" and "nothing to check" are different states; conflating them turns a safety gate into a vacuous pass that reports safety it never verified. Make presence explicit and assert expected counts in the load test.
14. **Verify the current measurement before tightening a gate.** Both a hard refusal gate and a widened superseded gate looked correct on paper; the committed reports decided it — 0.0 refusal made a hard gate impossible (ratchet instead), while a universal exclusion mechanism made the superseded widening safe. Read the baseline before you gate on it.
15. **A metric's exclusion set is not a detector's scan set.** The hallucination metric must exclude scaffolding (don't blame the model for text it didn't author); a breach detector must include it (a leak in a heading is still a leak). Sharing one set silently inverted the requirement.

## What remains

~33 findings fixed across Waves 1–6 + R01#1. Waves 7–9 + refactor tail: reliability (SIGTERM shutdown, unbounded stdin, recompose storm, breaker herd), console session/route auth (C15 keyless cookie, open redirect, rate-limit; C04 `.txt` matcher), correctness edge cases, ~48 M/L refactor.
