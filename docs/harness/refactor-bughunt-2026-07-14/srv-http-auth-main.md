> Context: Server: REST HTTP + Auth + Entrypoint
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## 1. Graceful shutdown never fires under a container SIGTERM
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: reliability-shutdown
- **File**: crates/brainiac-server/src/main.rs:355-361 (serve) and :423-427 (worker)
- **Scenario**: `axum::serve(...).with_graceful_shutdown(async move { let _ = tokio::signal::ctrl_c().await; ... shutdown_tx.send(true) })` — the shutdown future completes only on `ctrl_c()`, which on Unix is SIGINT. Container orchestrators (Cloud Run, k8s, systemd `stop`) send **SIGTERM** on every deploy/scale-down/rollout, never SIGINT. The standalone `worker` (line 424) has the identical `ctrl_c`-only wiring.
- **Root cause**: `tokio::signal::ctrl_c()` listens for SIGINT only; SIGTERM requires `tokio::signal::unix::signal(SignalKind::terminate())`. Interactive ctrl-c was the only signal considered.
- **Impact**: In the primary production deploy target the entire graceful-shutdown machinery is dead code. On every deploy the process runs until the orchestrator's grace timer elapses, then is SIGKILLed: in-flight HTTP requests (a search, a token mint, a `/v1/memories` write mid-commit) are dropped with a connection reset, and `serve --with-worker` never signals the worker to finish its in-flight batch (`worker_handle.await`, line 363, is never reached because `axum::serve` blocks forever waiting for a signal that never arrives — the process is hard-killed instead). The queue's retry design absorbs the worker interruption, but every rollout is an ungraceful drop.
- **Fix sketch**: Replace the `ctrl_c`-only future with a select over SIGINT and SIGTERM: `tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = async { signal(SignalKind::terminate())?.recv().await } => {} }` (with a Windows `#[cfg]` fallback to `ctrl_c`). Apply in both `serve` and `worker`.

## 2. Source is committed before the queue job is enqueued — a failed enqueue silently orphans it, and the non-keyed path can't be retried safely
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: atomicity-orphan
- **File**: crates/brainiac-server/src/http.rs:464-482 (`ingest_source`)
- **Scenario**: `insert_source` runs in a tx, `tx.commit()` (line 476) succeeds, then `enqueue_source` (line 477-480) runs as a **separate** DB operation. If the pool connection drops between the two (or the queue insert errors), the source row is durably committed but no job exists. `ingest_source` returns a 500, but the source is already persisted and will never be extracted. Because the plain `POST /v1/memories` path (no `Idempotency-Key`, line 506) and *every* bulk item route through here, a client that retries after the 500 mints a **second** source (no dedupe without a key) — the keyed path's careful re-enqueue recovery (lines 554-576) has no equivalent here.
- **Root cause**: The queue lives in its own schema (`queue.jobs`) outside the RLS-scoped source transaction, so the source insert and the enqueue can't share one commit; the non-keyed path never got the recovery logic the keyed path has.
- **Impact**: Orphaned, un-processable sources that `GET /v1/sources/{id}` reports as `status: "unknown"` forever (http.rs:1154), plus duplicate sources burning duplicate pipeline runs on retry. Silent partial-success from the caller's perspective.
- **Fix sketch**: Enqueue inside the same transaction as `insert_source` (write the job row via the same `tx` before `commit`), or make the source→job pair recoverable by reusing `job_id_for_source` on the non-keyed path the way the keyed branch already does.

## 3. `create_token` mints tokens for an arbitrary, unvalidated `user_id`
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: authz-validation
- **File**: crates/brainiac-server/src/http.rs:1245 (and the `create` call, 1248-1260)
- **Scenario**: `let user_id = body.user_id.unwrap_or(ctx.principal.user_id);` — the admin-scoped caller supplies any UUID as the principal the token acts as, and it is inserted verbatim with no check that the id names a real user *in the caller's org*. An admin can mint a token attributed to a colleague, to a user from a **different** org, or to a UUID that exists nowhere.
- **Root cause**: The endpoint trusts `body.user_id` as an already-validated in-org principal; only `scopes` and `name` are validated. Org confinement is enforced later by RLS on `org_id`, so the acting `user_id` was treated as harmless.
- **Impact**: Attribution/provenance spoofing — sources, feedback, and memories written by the token are `created_by` a user_id that may belong to another tenant or no one, corrupting the audit/provenance trail that this product's whole value rests on. When resolved, `team_ids_of(org, foreign_user)` returns `[]`, so the token silently operates team-less. If `api_tokens.user_id` (or a downstream `created_by`) carries an FK to `users`, a non-existent id turns every mint into a confusing 500.
- **Fix sketch**: When `body.user_id` is provided, verify membership in `ctx.principal.org_id` (e.g. a `SELECT 1 FROM users/team_members WHERE id/user_id = $1 AND org_id = $2` under the admin's scope) and 400 on miss; otherwise default to the caller.

## 4. Bulk ingest has no idempotency and discards receipts for already-committed items on a mid-batch fault
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: reliability-idempotency
- **File**: crates/brainiac-server/src/http.rs:630-675 (`memory_add_bulk`), esp. line 665
- **Scenario**: Each item is a standalone `ingest_source` (own commit + enqueue). On the first item that raises a 500, `Err(e) if e.status == INTERNAL_SERVER_ERROR => return Err(e)` (line 665) aborts the whole handler and throws away the `results` vector — but items `0..k` already committed sources and enqueued jobs. The client receives only a bare 500 with no per-item body and no way to learn which items landed. `BulkAddBody` (line 591) has no `Idempotency-Key` support at all, so the only recovery — resend the batch — re-ingests every already-committed item as a fresh duplicate. A plain network-level retry (client never saw the 202) duplicates the entire batch the same way.
- **Root cause**: The per-item loop was designed so *business* errors don't sink the batch, but a *systemic* error mid-loop leaves committed side effects with no receipt, and the idempotency mechanism built for single-add was never extended to bulk.
- **Impact**: Duplicate sources and duplicate pipeline runs (each an LLM extraction cost) on any retry of a partially-applied or unacknowledged bulk request; lost visibility into which items of a failed batch actually succeeded.
- **Fix sketch**: Support a per-item (or per-batch prefix) `Idempotency-Key` reusing `insert_source_idempotent`; and on a systemic fault return the partial `results` accumulated so far (with the failing index marked) rather than discarding them, so a retry is safe and diagnosable.

## 5. `worker_loop` opens two separate admin (RLS-bypass) pools that could be one
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: duplication-resource
- **File**: crates/brainiac-server/src/main.rs:494 and :497 (`sweep_admin`, `kb_admin`; closed 593-594)
- **Scenario**: `let sweep_admin = admin_pool(&database_url()?).await?;` and `let kb_admin = admin_pool(&database_url()?).await?;` create two independent `PgPool`s, each `max_connections(2)` (brainiac-store/src/lib.rs:69). They are used sequentially in the same loop iteration — `compose_sweep(..., &kb_admin)` then `sweeps::run_due(&sweep_admin, ...)` — never concurrently with each other, and both exist only to enumerate orgs cross-tenant.
- **Root cause**: The KB tick and the sweep scheduler were wired independently, each grabbing its own admin pool, rather than sharing one cross-org handle.
- **Impact**: Up to 4 idle RLS-bypassing admin connections held for the worker's whole lifetime instead of 2 — on the repeatedly-cited "1 GB free-tier VM" running `serve --with-worker` these stack on top of the runtime pool's 8, doubling the admin-connection footprint for no benefit. Two `database_url()` reads and two connection handshakes at startup for one logical need.
- **Fix sketch**: Open one `admin_pool` and pass a shared `&PgPool` reference to both `compose_sweep` and `sweeps::run_due`; close it once.
