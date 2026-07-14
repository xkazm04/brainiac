# Fix Wave 5 — Silent Failures / Success Theater

> 3 commits, 6 findings resolved (all High). Gates green: `cargo check --workspace
> --all-targets` 0 errors · console `tsc` 0 · vitest 50/50.

## Commits

| # | Commit | Findings | Files |
|---|---|---|---|
| 1 | `…` | R09#1 phantom approval | `store/governance.rs`, `server/console.rs` |
| 2 | `…` | R13#2 empty choices / missing usage | `gateway/lib.rs` |
| 3 | `…` | C07 + C10 + C09(+copy) + C03#2 | 8 console files |

## What was fixed

1. **Phantom promotion approval (R09#1).** `set_memory_status` returned `Ok(())` regardless of rows affected, so if the memory was deleted or out of RLS scope, the audit row read "approved" while the memory's status never moved. Now returns `Result<bool>` (rows_affected == 1, only marking dirty on a real change); the reviewer path treats `false` as a 409 and returns before commit, rolling back the promotion stamp.

2. **Empty provider response served as success (R13#2).** Empty `choices` fell through `unwrap_or_default()` to `text = ""` and returned `Ok`, so the pipeline extracted from an empty string with nothing going red — now an `Err`. A missing `usage` object metered a billed call at 0 tokens — now logged as a warning.

3. **Live pages fabricated data / swallowed failures (C07/C10/C09/C03#2).**
   - Graph drill-down (C07) and memory inspector (C10): a failed per-item fetch on a *live* page synthesized `demoDetail` and rendered it as real (no banner). The hooks now keep synthesis only for whole-page demo mode and return an `error` the views surface on a live-fetch failure.
   - Key revoke (C09): a swallowed failure collapsed the confirm as success while the key stayed active — now surfaces an error and keeps the confirm open. "✓ copied" was shown unconditionally, losing a one-time secret on a blocked clipboard — now only flips on a resolved write.
   - `withDemoFallback` (C03#2): logs the underlying error before degrading to fixtures, so a real 401/403/500 is diagnosable instead of vanishing behind the demo banner.

## Verification

| Gate | Result |
|---|---|
| `cargo check --workspace --all-targets` | 0 errors |
| console `tsc` / vitest | 0 errors / 50 pass |
| pg integration tests | not run (need Postgres) — verify store_pg (set_memory_status now Result<bool>) + console_pg |

## Patterns established (catalogue items 11–12)

11. **A repository mutation must report whether it changed a row.** `Result<()>` on an `UPDATE … WHERE id=$1` cannot distinguish "done" from "matched nothing" — return the rows-affected/`RETURNING` so the caller can reject a phantom success instead of committing a divergence between the audit trail and reality. (R09#1)
12. **"Offline mode" fallback belongs at the page (banner) level, never per-item on a live page.** Synthesizing fixtures inside a live drill-down renders fabrication as real with no signal; keep synthesis gated on the whole-page live flag and surface per-item failures as errors. (C07/C10)

## What remains / follow-ups

- **Follow-up (C03#2):** a distinct banner for "server misconfigured/erroring" vs "offline" is a per-page change; logged for now.
- Waves 6–9 + refactor tail: eval-gate integrity (Wave 6), reliability shutdown/retry-storm/breaker (Wave 7), console session/route auth incl. C15 keyless cookie + C04 `.txt`-matcher (Wave 8), correctness edge cases (Wave 9), ~48 M/L refactor.

## Milestone

~27 findings fixed across Waves 1–5 + R01#1 (all 5 Criticals + the highest-value Highs). ~133 open, no Criticals.
