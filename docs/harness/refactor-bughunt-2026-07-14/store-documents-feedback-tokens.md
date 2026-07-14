> Context: Store: Documents + Feedback + Publishing + Tokens
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. Compose window drops memory changes: `insert_revision` unconditionally clears `dirty_at`
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: lost-update-race
- **File**: crates/brainiac-store/src/documents.rs:266-278 (with mark_dirty_for_memory:186-195)
- **Scenario**: The compose worker (brainiac-pipeline/src/worker.rs:229-271) fetches the dirty list in one committed tx, then composes each page (a slow LLM call) in a *new* tx, and finally calls `insert_revision`, which runs `UPDATE documents SET dirty_at = NULL … WHERE id = $1`. If a memory the page depends on changes during that compose window, `mark_dirty_for_memory` fires `dirty_at = COALESCE(dirty_at, now())` — but the row is *already* dirty (that is why the worker picked it), so COALESCE is a no-op and the timestamp never advances. `insert_revision` then clears `dirty_at` to NULL. The change that landed mid-compose is silently discarded.
- **Root cause**: The clear is unconditional and there is no version/CAS token proving the freshly written revision reflects the memory state as of the clear. `mark_dirty`'s COALESCE deliberately preserves the oldest dirty timestamp, which means "re-dirtied while already dirty" is invisible.
- **Impact**: A published page silently presents superseded/stale knowledge and is marked clean — it will not recompose until some *unrelated* future memory change happens to re-dirty it. This defeats the module's entire stated anti-rot guarantee ("the difference between this and every wiki that has ever rotted"), and fails silently.
- **Fix sketch**: Capture `dirty_at` when the worker claims the doc and clear conditionally: `SET dirty_at = NULL WHERE id = $1 AND dirty_at = $claimed_dirty_at`; if the row was re-dirtied, leave it set so it recomposes. Alternatively add a monotonic `dirty_seq bigint` bumped on every mark_dirty and cleared only when unchanged. Either way `mark_dirty*` must *advance* the signal (e.g. bump a counter) rather than COALESCE it away.

## 2. `approve_revision` regresses a page to an already-superseded revision (no monotonicity check)
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: correctness / stale-write
- **File**: crates/brainiac-store/src/documents.rs:356-384
- **Scenario**: Revision R1 is written `needs_review` and sits in `pending_revisions`. Before a human signs it, a `memory_change` produces R2 which auto-publishes — `insert_revision` sets `current_revision = R2` and never touches R1's `reviewed_by`, so R1 stays in the review queue. A maintainer later approves R1; `approve_revision` runs `UPDATE documents SET current_revision = $2, status = 'published'` with `$2 = R1` and no check that R1 is newer than the current revision.
- **Root cause**: The publish transition trusts the reviewer's choice of revision id absolutely; it never compares R1 against `documents.current_revision` (or `created_at`) to reject a backwards move. The two revision-producing paths (auto-publish vs. human-approve) evolved independently.
- **Impact**: Approving a stale pending revision silently reverts a live page to older content built from memories that have since been superseded/deprecated — a confident republish of known-stale belief, exactly the failure KB3 exists to prevent. Also, `approve_revision` never clears `dirty_at`, so the state after approval is inconsistent with the auto-publish path.
- **Fix sketch**: Guard the promotion: `… WHERE id = $1 AND (current_revision IS NULL OR current_revision IN (SELECT id FROM document_revisions WHERE created_at <= (SELECT created_at FROM document_revisions WHERE id = $2)))`, or simpler, only allow approval when R1 is the newest revision for the doc; otherwise return false so the UI can prompt a recompose. Clear `dirty_at` on approve to match the auto-publish path.

## 3. `tokens::resolve` writes `last_used_at` on every authenticated request
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: performance / write-amplification
- **File**: crates/brainiac-store/src/tokens.rs:31-45
- **Scenario**: Every `brk_…` API call goes through `resolve_bearer` (brainiac-server/src/auth.rs:129), which runs `UPDATE api_tokens SET last_used_at = now() WHERE token_hash = $1 …` on the raw pool. A single service token shared across all app instances (the common deployment shape for a machine-facing memory engine) means every request in the fleet issues a heap+WAL write to the *same* row, and concurrent requests serialize on that row's write lock for the statement's duration.
- **Root cause**: Precise last-used tracking is folded into the auth hot path as a synchronous write, treating an observability nicety as if it were free.
- **Impact**: Read-only (`scope=read`) requests incur a write; auth can never be served from a read replica; write volume scales with total request volume; and a hot shared token adds tail latency and lock contention under load — a scalability ceiling on the busiest code path in the system.
- **Fix sketch**: Decouple the touch from resolution: make `resolve` a pure `SELECT`, and update `last_used_at` opportunistically/coarsely (e.g. only when the stored value is older than N minutes: `… WHERE token_hash=$1 AND (last_used_at IS NULL OR last_used_at < now() - interval '5 min')`), or batch touches asynchronously off the request path.

## 4. `feedback::flagged` aggregates every note per memory in SQL, then keeps only 5 in Rust
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: performance / unbounded-transfer
- **File**: crates/brainiac-store/src/feedback.rs:151-155,182-186
- **Scenario**: The query builds `array_agg(f.note ORDER BY f.created_at DESC) FILTER (WHERE f.note IS NOT NULL)` — the *entire* set of notes across all open claims for each memory — then Rust does `.into_iter().take(5)`. A controversial memory can accumulate hundreds of `wrong`/`outdated` notes; the page returns up to 200 such memories. Every one of those note strings is materialized in Postgres and shipped over the wire only to be dropped after the first five.
- **Root cause**: The cap the comment promises ("most recent first, capped") is applied client-side after full aggregation instead of inside SQL, so array size grows with claim count.
- **Impact**: Memory and bandwidth on the triage-queue endpoint scale with total feedback volume per memory rather than the fixed 5 that are displayed; a few heavily-flagged memories can bloat a single `flagged` response into megabytes and spike DB memory for the aggregate.
- **Fix sketch**: Cap inside SQL, e.g. `(array_agg(f.note ORDER BY f.created_at DESC) FILTER (WHERE f.note IS NOT NULL))[1:5]`, or a lateral subquery selecting the 5 newest notes per memory. Drop the redundant Rust `.take(5)`.

## 5. Duplicated "make this revision current" transition (already drifted)
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-store/src/documents.rs:266-278 & 375-382
- **Scenario**: The publish state-transition — set `current_revision`, `status = 'published'`, `updated_at = now()` — is written twice: once in `insert_revision`'s auto-publish branch (the `CASE WHEN $2 …` UPDATE) and once verbatim in `approve_revision`'s second UPDATE. The two copies have already diverged: the auto-publish path clears `dirty_at` (via the preceding statement), while `approve_revision` leaves `dirty_at` untouched.
- **Root cause**: Two revision-promotion entry points were implemented separately rather than routing through one primitive; the shared transition was copied rather than extracted, so a fix to one (e.g. the monotonicity guard in finding #2, or dirty_at handling) will not reach the other.
- **Impact**: Low today, but it is a correctness-adjacent duplication: the drift already produced the inconsistent `dirty_at` behavior noted in finding #2, and future changes to the publish semantics must be remembered in two places.
- **Fix sketch**: Extract a private `async fn set_current_revision(conn, doc_id, revision_id) -> Result<()>` that performs the canonical UPDATE (including whatever `dirty_at` policy is chosen) and call it from both `insert_revision` (auto-publish branch) and `approve_revision`.
