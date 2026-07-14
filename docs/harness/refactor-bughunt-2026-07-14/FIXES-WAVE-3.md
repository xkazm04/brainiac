# Fix Wave 3 — Anti-Rot Write-Loss

> 2 commits, 4 findings resolved (1 Critical + 3 High), preceded by R01#1 (the
> carried-over Wave-1/2 analytics fix). Gates green: `cargo check --workspace
> --all-targets` 0 errors. pg integration tests need a live Postgres (not run) —
> these are all document-path fixes, so verify `compose_pg` / `docs_pg` /
> `publish_pg` against a DB.

## Commits

| # | Commit | Findings | Severity | Files |
|---|---|---|---|---|
| — | `abbc…` (pre-wave) | R01#1 org-true analytics | High | `server/http.rs`, `server/console.rs` |
| 1 | `…` | R06 dirty_at CAS + R10#1 (twin) + R10#2 approve regression | Critical + 2 High | `store/documents.rs`, `pipeline/worker.rs`, 3 test files |
| 2 | `…` | R04#2 pinned-edit optimistic concurrency | High | `store/documents.rs`, `server/docs.rs` |

## What was fixed

1. **Compose window silently dropped mid-compose changes (R06, Critical; = R10#1 from the store side).** `compose_tick` read the dirty list, composed each page across a multi-second LLM call, then `insert_revision` cleared `dirty_at` unconditionally. `mark_dirty*` uses `dirty_at = COALESCE(dirty_at, now())`, so a dependency-memory change during the compose window never advanced the timestamp on an already-dirty page — and the clear then marked it clean. The page served the losing belief while flagged fresh, defeating the "wiki cannot rot" headline. **Fix:** use `updated_at` (bumped by every `mark_dirty*`) as a compare-and-swap token — the worker threads its claim-time value through `NewRevision.claimed_updated_at`, and `insert_revision` clears `dirty_at` only when `updated_at IS NOT DISTINCT FROM` the claim. `dirty_at`'s COALESCE stays, so the staleness-age SLA is unaffected; `None` preserves the unconditional clear for non-worker callers.

2. **`approve_revision` could republish stale content (R10#2, High).** It set `current_revision` to the approved id with no check it was newer than the page's current revision, so approving a revision that sat in review while a newer one auto-published silently reverted the page to content built from since-superseded memories. **Fix:** read the current revision's `created_at` under `FOR UPDATE OF d` and reject a backwards move (return false). `dirty_at` is intentionally left untouched — clearing it would drop pending changes; a dirty page must still recompose.

3. **Concurrent pinned-section edits were last-writer-wins (R04#2, High).** `update_pinned` overwrote with no version predicate, so two maintainers editing one section clobbered each other silently. **Fix:** `update_pinned` now CAS-es on the content the handler read (`pinned_content IS NOT DISTINCT FROM $3`); a concurrent commit changes the value, matches 0 rows, and `doc_edit` returns 409. The finding's edit-vs-compose half is already closed by fix #1 (doc_edit's `mark_dirty` bumps `updated_at`, so an in-flight compose won't clear `dirty_at`).

4. **(Pre-wave) Knowledge Health analytics were viewer-RLS-scoped (R01#1, High).** Computed org-true totals on an admin (RLS-bypassing) pool now plumbed into `AppState`, for both the GET report and the POST snapshot, so the leadership score is the org's real numbers and snapshots stop polluting shared history. Detail lists stay viewer-scoped — no content leak.

## Verification

| Gate | Result |
|---|---|
| `cargo check --workspace --all-targets` | 0 errors |
| DB-free unit tests | unaffected (document path is pg-only) — 88 still pass |
| pg integration tests | not run (need Postgres) — verify compose_pg/docs_pg/publish_pg; the 5 NewRevision test sites pass `claimed_updated_at: None` (old unconditional-clear behavior, so single-threaded tests are unchanged) |

## Patterns established (catalogue items 8–9)

8. **Compare-and-swap a "clean" flag against a token the writers already bump.** When a slow read→clear window can lose a concurrent change, don't clear unconditionally — capture a monotonic/changing token (`updated_at`) at claim and clear only if it's unchanged. Reuse an existing bumped column before adding one. (R06)
9. **Content-CAS gives optimistic concurrency without a schema or API change.** When a handler reads-then-writes a row, pass the read value back into the UPDATE's WHERE (`col IS NOT DISTINCT FROM $expected`) and 409 on 0 rows — a server-only guard that catches the lost update. (R04#2)

## What remains

- **Deferred (needs a product call):** a true per-edit version/etag on `EditSectionBody` would let the console show a merge/reload UX; the content-CAS closes the silent clobber without it.
- The last Critical, **R13** (gateway provider no-timeout), leads **Wave 4 (timeouts/hangs)**: R17 (Confluence holds a DB txn), C03 (console no-timeout), C04 (route no-timeout).
- Waves 5–9 + refactor tail per the INDEX.
