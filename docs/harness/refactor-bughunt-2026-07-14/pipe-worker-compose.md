> Context: Pipeline: Worker + Compose + Ingest
> Total: 5 (Critical: 1, High: 2, Medium: 2, Low: 0)

## 1. Compose clears `dirty_at` unconditionally — a memory change that lands mid-compose is silently lost
- **Severity**: Critical
- **Lens**: bug-hunter
- **Category**: lost-update / marked-clean-while-write-lost
- **File**: crates/brainiac-pipeline/src/worker.rs:210-293 (loop + insert_revision call at 258-271)
- **Scenario**: `compose_tick` reads the dirty work list in its own committed tx (223-227), then for each page opens a fresh tx and runs `compose_document`, which begins with an LLM-bound `bound_memories` read of the page's memories and takes seconds. While that page is composing, a governance write on one of its dependency memories runs on another connection — `apply_supersession`/`set_memory_status` → `documents::mark_dirty_for_memory`, which does `dirty_at = COALESCE(dirty_at, now())`. Because the page is *already* dirty (that's why compose picked it), COALESCE keeps the old timestamp — the new change is absorbed into the existing marker, invisible. Compose then finishes on its stale snapshot and `insert_revision` runs `UPDATE documents SET dirty_at = NULL` (store documents.rs:266-278), marking the page clean.
- **Root cause**: The dirty flag is a single nullable timestamp with no compose cursor/versioning. `mark_dirty` uses `COALESCE` (never advances an already-set timestamp) and `insert_revision` clears it blindly, with no check that nothing changed during the compose window. The design assumes compose reads and the flag-clear are effectively atomic; they are separated by a multi-second LLM call.
- **Impact**: The exact failure the product's headline claim ("the wiki cannot rot") is built to prevent. A superseded/changed dependency memory that arrives during a compose leaves the page marked fresh while serving the losing belief, and it will not recompose until some *other*, unrelated change re-dirties the same page. No test catches it (all compose_pg tests are single-threaded and serialize via `DB_LOCK`). The ingest worker and compose loop run concurrently in production, so this is a live window on every page.
- **Fix sketch**: Capture each doc's `dirty_at` when `dirty_documents` reads it and thread it into `insert_revision`; clear only conditionally: `SET dirty_at = NULL WHERE id = $1 AND dirty_at = $captured`. Also change `mark_dirty_for_memory` to *advance* the timestamp on an already-dirty row (drop COALESCE, use `now()` or `GREATEST(dirty_at, now())`) so a mid-compose change moves the timestamp forward and the conditional clear becomes a no-op, leaving the page dirty for the next tick.

## 2. A failed best-effort audit-row write (`write_pipeline_run`) aborts the ack and forces full source reprocessing / dead-lettering
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure / at-least-once cost / retry-storm
- **File**: crates/brainiac-pipeline/src/worker.rs:335-364
- **Scenario**: On the success path, `write_pipeline_run(...).await?` runs *before* `queue::complete(...)`. If that observability-row INSERT fails transiently (pool exhaustion, statement timeout, RLS blip), the `?` propagates out of `process_claimed_job`, the job future resolves to `Err`, and `tick`'s fold (`let run = outcome?;`, 184) aborts the whole tick. Critically, `queue::complete` never ran, so the already-committed job (its memories are durable) is left in-flight; after the 300s visibility window it is redelivered, `process_job` re-runs (re-calling the LLM for every extract chunk — dedup at extract.rs:550 only skips the DB write, not the LLM call), and `write_pipeline_run` fails again. `attempts` is bumped on every claim, so after `MAX_ATTEMPTS` the *successfully-ingested* source is reaped into the dead-letter archive as `dead` with no `pipeline_runs` row ever written.
- **Root cause**: The code's own doc comment (388-393) states losing the run row is "acceptable... the memories still exist" — but the implementation gates the queue ack on that write instead of treating it as best-effort. An `Err` from the audit write is handled identically to an infrastructure failure that must abort.
- **Impact**: A transient failure on a row explicitly designed to be losable causes repeated full-chain LLM re-extraction (token burn), discards the entire tick's stats, and can dead-letter a source whose ingest actually succeeded — an operator sees a "dead" job while the memories are live and un-audited.
- **Fix sketch**: Make `write_pipeline_run` best-effort on the ack path: `if let Err(e) = write_pipeline_run(...) { tracing::error!(...) }` and proceed to `queue::complete` regardless (the memories carry `run_id` via provenance either way). Keep the `?` only on `queue::complete`/`queue::fail` themselves, which are the true ack primitives.

## 3. A deterministically-failing page recomposes on every tick forever — no backoff, no cap, unbounded LLM cost
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: retry-storm / unbounded-resource
- **File**: crates/brainiac-pipeline/src/worker.rs:229-291 (Err arm 284-290)
- **Scenario**: When `compose_document` errors (a provider hard-reject, a malformed binding, a persistently failing embedder), the Err arm logs, increments `stats.failed`, and drops the tx so the page stays dirty. The caller loops calling `compose_tick`; `dirty_documents` returns the same page again next tick; it composes (one LLM call), fails, stays dirty — indefinitely. Unlike the job queue, which has `MAX_ATTEMPTS`, exponential `backoff_secs`, and a dead-letter archive, the compose path has no per-document attempt counter, no backoff, and no poison-page terminal state.
- **Root cause**: The comment "The page stays dirty: a failed compose must retry, never silently leave a stale page looking fresh" is correct in intent but implemented as an *unbounded* immediate retry. The dirty flag is boolean-ish (a timestamp), carrying no failure count.
- **Impact**: One poison page burns a full LLM call every compose tick, forever, while never producing a revision — a silent money/quota drain and a page that is permanently, invisibly stuck. At limit 50 pages/tick a handful of poison pages also crowd out healthy dirty pages' share of each tick.
- **Fix sketch**: Add a `compose_attempts` / `compose_next_at` (or `last_compose_error`) column; on failure set an exponential `compose_next_at` and have `dirty_documents` filter `compose_next_at <= now()`; after N failures, park the page in a `needs_attention` state surfaced to operators (mirroring the queue's dead-letter split) instead of retrying blindly.

## 4. Visibility window (300s) can be shorter than the worst-case chain → an in-flight job is re-claimed and processed concurrently
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: lease-timeout / at-most-once gap
- **File**: crates/brainiac-pipeline/src/worker.rs:33-35, 158-180
- **Scenario**: `tick` claims a batch with `visibility_secs` (default 300) and drains `concurrency` (default 4) jobs at once. A single source's chain is `extract` (one LLM call per chunk) + `resolve` (one LLM call per new entity) + `contradict` (one LLM call per new memory) — for a large source with many entities/memories this is easily dozens of serial LLM round-trips and can exceed 300s. When it does, `queue::read` on the next tick sees the job ready again (visible_at elapsed) and re-claims it while the first invocation is still running, so the same source is processed twice concurrently.
- **Root cause**: The visibility window is a static tunable documented as needing to "comfortably exceed the slowest full chain," but the slowest chain is unbounded (scales with entities/memories in the source) while the window is fixed. Safety against the resulting duplicate then rests entirely on extract's dedup being race-safe across two *uncommitted* concurrent runs (a DB unique constraint, not just the SELECT-then-insert guard at extract.rs) — otherwise both runs write duplicate memories and double-insert promotions.
- **Impact**: At best, wasted full-chain LLM cost on the duplicate run; at worst, if the dedup guard is not enforced by a unique constraint, duplicate memories / duplicate promotion rows for the non-deduped tail. Either way an at-most-once expectation is violated on long sources.
- **Fix sketch**: Scale/extend the lease to the work: heartbeat the job (periodically push `visible_at` forward while a chain runs), or size `visibility_secs` from an upper bound on chunks×per-call budget. Ensure extract's dedup is backed by a `UNIQUE(source_id, content_hash)` constraint so a concurrent double-claim degrades to a constraint error (retry) rather than duplicate rows.

## 5. Per-entity `SELECT name, kind, aliases` inlined in the worker resolve loop — raw SQL bypasses the store layer and is N+1
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: layering-violation / N+1 / duplication
- **File**: crates/brainiac-pipeline/src/worker.rs:492-514
- **Scenario**: `process_job` resolves each newly-created entity by issuing a hand-written `sqlx::query("SELECT name, kind, aliases FROM entities WHERE id = $1")` inside a `for entity_id in &extracted.entities_created` loop, with inline `use sqlx::Row;` and manual `row.get(...)` column plucking. Every other table access in this pipeline goes through the `brainiac_store` crate (`memories::get_by_ids`, `governance::*`, `documents::*`); this is the one spot reaching directly into a store-owned table from the orchestration layer, one row per query.
- **Root cause**: A convenience shortcut — the store crate has no `entities::get_by_ids`/`get_for_resolve` helper, so the worker grew its own raw query rather than adding one. It mirrors the batched pattern already used two lines earlier (`memories::get_by_ids`) but does the opposite (one-at-a-time).
- **Impact**: N round-trips for N new entities on a source (a chatty source with many entities pays a query each), plus a schema-coupling leak: a column rename to `entities` now silently breaks the worker instead of a single store module. Minor duplication of row-decoding logic the store already owns.
- **Fix sketch**: Add `brainiac_store::entities::get_by_ids(conn, &ids) -> Vec<{id,name,kind,aliases}>` (batched, like `memories::get_by_ids`) and call it once before the resolve loop; drop the inline `sqlx`/`Row` use from worker.rs.
