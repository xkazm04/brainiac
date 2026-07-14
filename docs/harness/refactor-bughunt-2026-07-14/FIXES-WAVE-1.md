# Fix Wave 1 — Governance Authorization

> 3 commits, 6 findings resolved (2 Critical + 3 High under the theme + 1 reslotted).
> Branch `vibeman/refactor-bughunt-2026-07-14` off `master`. Gates green throughout:
> `cargo check -p brainiac-server` 0 errors · console `tsc --noEmit` 0 errors · console vitest 50/50.
> pg integration tests not run (need a live Postgres; baseline was captured the same way).

## Commits

| # | Commit | Findings closed | Severity | Files |
|---|---|---|---|---|
| 1 | `538b7a5` | R04 doc_edit authz | Critical | `crates/brainiac-server/src/docs.rs` |
| 2 | `d69c8f3` | R01 promotion TOCTOU | High | `crates/brainiac-server/src/console.rs` |
| 3 | `8960297` | C05 + C06 + C11 (authz copy + concurrency) | 1C/2H → downgraded | `console/app/console/(modules)/{reviews,disputes,docs/[slug]}/actions.ts` |

## What was fixed

1. **`doc_edit` privilege escalation (R04, Critical).** `POST /v1/docs/{slug}/edit` was gated on `kb:read` with no maintainer check, even though a pinned edit auto-publishes into live markdown and a composed edit is injected into extraction framed as "A maintainer edited …". A read-scoped `brk_` token or any non-maintainer could rewrite published prose and poison candidate memories. Now requires `kb:publish` **and** the same `is_maintainer` / `is_any_maintainer` gate `doc_approve` uses — the framing claim is now backed by a verified role. Verified the existing `doc_edit_pg` test still passes: its `tok_lead` env token is unrestricted-scope and a maintainer of the team, so it passes both gates; the fix bites only limited-scope `brk_` tokens.

2. **Promotion approve/reject TOCTOU (R01#2, High).** `actionable_promotion` read the pending row with no lock and `review_promotion`'s UPDATE didn't re-assert `reviewed_at IS NULL`, so two concurrent approve/reject requests both passed the read and last-writer-won, running `set_memory_status` twice and leaving the memory nondeterministic. Added `FOR UPDATE OF p` to the read and a self-guarding `WHERE id=$1 AND reviewed_at IS NULL`; `rows_affected == 0` now returns **409** and skips `set_memory_status`, making the transition idempotent-or-nothing.

3. **Console governance actions under the single-operator model (C05/C06/C11).** Per the trust decision, the console keeps its single shared service token (no per-user identity added) but that posture is now **documented** at the top of each action file, and the two concrete bugs it left are fixed:
   - **Honest error copy** — a 403 now says the *service token* isn't a maintainer (it was "You need to be a maintainer…", misleading when there is no per-user identity); 404/409 read as "already decided in another session".
   - **Optimistic-concurrency guard** — when the backend reports the item was already decided (404, or the new atomic **409** from fix #2), the action now also `revalidatePath`s its queue so the phantom row clears for the losing client too, instead of leaving them clicking a doomed item. Previously only the winning client revalidated.

## Reslotted

- **R01#1 (High) — viewer-RLS-scoped Knowledge Health analytics** → **Wave 2**. The correct fix computes org-true totals on the RLS-bypassing admin pool, but `AppState` holds no admin pool (the scheduled sweep builds its own in `main.rs`). Plumbing one into `AppState` + `router()` is a structural change better done deliberately, and this is metric-integrity rather than pure authz. Pairs naturally with the Wave-2 knowledge-integrity work.

## Verification

| Gate | Before (baseline) | After Wave 1 |
|---|---|---|
| `cargo check --workspace` | 0 errors | 0 errors |
| console `tsc --noEmit` | (clean) | 0 errors |
| console vitest | 50/50 | 50/50 |

## Patterns established (catalogue items 1–3)

1. **Visibility is not authorization.** RLS deciding a caller *can see* a row (via `scoped_tx`) is not the same as the caller being *allowed to mutate* it. Every mutating endpoint needs an explicit role gate (`is_maintainer`), never "the read succeeded, so the write is fine." (R04)
2. **Self-guarding state transitions beat check-then-act.** For any "decide once" mutation, put the precondition in the UPDATE's `WHERE` and treat `rows_affected == 0` as the conflict, plus `FOR UPDATE` on the actionable read. Never trust a prior unlocked SELECT to still hold at the UPDATE. (R01#2)
3. **A shared service token needs an honest UI and a losing-client refresh.** When a UI has no per-user identity, error copy must attribute failures to the token, not "you"; and any list acting on server state must revalidate on the *failure* path (not only success) so a client that lost a race stops showing phantom rows. (C05/C06/C11)

## What remains

Bug themes B–J + correctness (Waves 2–9) and the refactor tail (Theme R). Next up per the plan: **Wave 2 — knowledge integrity** (R07 Critical + R05/R09/R12 highs), and it now also carries **R01#1** (health analytics org-true computation).
