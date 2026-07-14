# Fix Wave 2 — Knowledge Integrity

> 6 commits, 7 findings resolved (1 Critical + 6 High). R01#1 remains (see below).
> Gates green: `cargo check --workspace --all-targets` 0 errors · DB-free unit tests
> core 55 / pipeline 29 / gateway 4 (incl. 5 new). pg integration tests need a live
> Postgres (not run) — two commits carry explicit verify-against-DB notes.

## Commits

| # | Commit | Findings | Severity | Files |
|---|---|---|---|---|
| 1 | `f441fcb` | R07 reembed serve-gate + R07#3 dim check | Critical + High | `store/memories.rs`, `pipeline/reembed.rs`, `server/http.rs`, `server/mcp.rs` |
| 2 | `3a5e331` | R12 supersession-cycle dedupe | High | `core/temporal.rs` |
| 3 | `aa65f64` | R09#2 apply_supersession winner validation | High | `store/governance.rs` |
| 4 | `5ee9d75` | R05#2 extraction valid-but-wrong shape | High | `pipeline/extract.rs` |
| 5 | `b5d99b9` | R05#1 policy holds on contradiction | High | `pipeline/policy.rs`, `pipeline/worker.rs` |
| 6 | `a763882` | R05#3 kind-agreement in merge fast-path | High | `store/entities.rs`, `pipeline/resolve.rs` |

## What was fixed

1. **Half-backfilled embedding version was fully servable (R07, Critical).** `is_active` was set eagerly at version creation and never read; the serve path re-derived the version from the configured embedder, so restarting on embedder B after an interrupted reembed served a version populated for only part of the corpus — memories vanished from results with no error. `is_active` now means "backfill complete, safe to serve": swap-target versions are born inactive (the first-ever version stays active, so fresh systems are unaffected), reembed calls the new `activate_embedding_version` only after both loops drain, and the REST/MCP serve bootstraps use `serving_embedding_version`, which refuses an inactive (incomplete) version. **+ R07#3:** reembed now asserts each vector's dimension (the embeddings column is typmod-free, so a wrong-length vector inserted silently then was permanently unsearchable).

2. **Supersession cycles served both versions (R12, High).** `chain_head` was cycle-terminating but not cycle-canonical, so a 1↔2 cycle resolved to different heads by start node and dedupe kept both. It now returns the min id of the cycle members, so a cycle collapses to one survivor (self-supersession → itself). Two new tests.

3. **`apply_supersession` never validated the winner (R09#2, High).** Added a self-supersession reject and a paired `... WHERE id IN (loser, winner) ORDER BY id FOR UPDATE` that locks both rows (serializing opposite-direction A↔B races) and requires the winner be visible and not already superseded by the loser.

4. **Valid-but-wrong extraction JSON silently dropped knowledge (R05#2, High).** A refusal/reasoning wrapper deserialized to an empty vec (`#[serde(default)]` memories) and read as a clean 0-extraction, bypassing the repair loop. The object shape now requires a `memories` key and a lone recovered array must be non-empty; otherwise an Err drives the repair re-prompt. New test.

5. **Auto-promotion ignored a just-opened contradiction (R05#1, High).** Threaded a `PolicyContext { open_contradictions }` into `PolicyEngine::evaluate`; a memory that opened a contradiction is held `NeedsReview` ("contradiction_pending") instead of auto-promoting conflicting knowledge into retrieval. New test. *(Verify-against-DB note in the commit: preserves pipeline_pg's `auto+needs == total` invariant.)*

6. **Lexical merge fast-path false-merged across kinds (R05#3, High).** `find_canonical_by_name_or_alias` now requires kind agreement, so a surface-form collision across kinds (person "Mercury" vs service "Mercury") no longer auto-links at 1.0. The legitimate same-kind cross-team acronym bridge is preserved.

## Verification

| Gate | Result |
|---|---|
| `cargo check --workspace --all-targets` | 0 errors (1 pre-existing dead-code warning) |
| unit tests (core/pipeline/gateway, no DB) | 88 pass (+5 new: 2 temporal, 1 extract, 1 policy, and existing) |
| pg integration tests | not run (need live Postgres) — see notes in `f441fcb`, `b5d99b9` |

## Patterns established (catalogue items 4–7)

4. **A "servable" flag must be *read*, not just written.** A completeness/active flag that nothing consults is worse than none — it implies a gate that isn't there. Gate the serve path on it and set it only when the invariant actually holds. (R07)
5. **Cycle-*terminating* ≠ cycle-*canonical*.** Stopping a walk on revisit avoids a hang but can still produce start-dependent results; when corrupt data forms a cycle, pick a deterministic representative so all members agree. (R12)
6. **`#[serde(default)]` on a required field turns a wrong shape into a silent empty.** For LLM output, require the key's presence (parse to `Value` and check) so a refusal/wrapper drives the repair path instead of masquerading as "nothing to extract." (R05#2)
7. **Exact-match ≠ authorization to merge.** An exact surface-form hit against a model-accumulated alias pool is not the guarantee a canonical-name hit is; gate auto-merge on kind agreement (and, ideally later, adjudicate alias-only hits). (R05#3)

## What remains

- **R01#1 (High, carried from Wave 1)** — Knowledge Health analytics run under the caller's RLS, so cross-team contradictions with an invisible other side are undercounted and `POST /snapshot` persists viewer-scoped numbers into shared org history. The fix computes org-true totals on an admin (RLS-bypassing) pool, which must first be plumbed into `AppState` (it has none — the sweep builds its own in main.rs). Router-signature change; do as a focused follow-on.
- **Deferred (needs a product call):** route alias-ONLY merge matches through the LLM adjudicator (R05#3 residual) — cuts more false-merges but changes the no-seed cross-team bridging pipeline_pg enshrines.
- Waves 3–9 + refactor tail per the INDEX.
