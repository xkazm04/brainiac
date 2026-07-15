# Fix Wave 7 — Reliability

> 7 commits, 7 findings resolved (all High). Gates green: `cargo check --workspace
> --all-targets` 0 errors · 117 DB-free unit tests pass (+4 new) · console `tsc` 0 /
> vitest 50/50. One migration (0021) — see the DB caveat below.

## Commits

| # | Finding | Files |
|---|---|---|
| 1 | R02 SIGTERM never drains | `server/main.rs` |
| 2 | R03#2 unbounded stdio frame | `server/mcp.rs` |
| 3 | R06#2 audit row blocks the ack | `pipeline/worker.rs` |
| 4 | R06#3 compose retry storm | `migrations/0021`, `store/documents.rs`, `pipeline/worker.rs` |
| 5 | R13#4 breaker half-open herd | `gateway/resilience.rs` |
| 6 | R13#3 redaction misses bearer/JWT | `core/redact.rs` |
| 7 | C08#2 ingest poll storm | `console/…/ingest/useIngestFeed.ts` |

## What was fixed

1. **Graceful shutdown never fired in production (R02).** Both shutdown futures waited on `ctrl_c()` — SIGINT only. Every orchestrator (Cloud Run, k8s, systemd) sends **SIGTERM**, so the entire graceful-shutdown path was dead code in the primary deploy target: on every rollout the process ran until the grace timer elapsed and was SIGKILLed, dropping in-flight requests, and `serve --with-worker` never reached `worker_handle.await`. Added `shutdown_signal()` — a select over SIGINT and SIGTERM on unix (degrading to ctrl_c if the handler can't register), ctrl_c on non-unix. Both call sites wired.

2. **The MCP input caps ran too late (R03#2).** Every cap (`MAX_CONTENT_CHARS` et al.) is enforced *after* parsing, but the transport used `BufReader::lines()`, which buffers a frame of any size — so a caller that simply never sends a newline streams unbounded input and OOMs the process before a single cap runs. This is a trust boundary reached by autonomous agents. Frames now read through `.take(MAX_FRAME_BYTES)` (1 MB); hitting the cap with no terminator ends the session (resyncing would mean draining an equally unbounded frame). A short final line without a trailing newline is still processed.

3. **A losable audit row blocked the queue ack (R06#2).** `write_pipeline_run` ran with `?` before `queue::complete`, so a transient failure on that observability INSERT aborted the tick with the job left in-flight — redelivered after the visibility window, re-running the whole chain (re-calling the LLM per extract chunk) and bumping `attempts` until a **successfully-ingested** source was dead-lettered. The function's own docs already said losing the row is acceptable; the implementation now agrees (log and proceed). `queue::complete`/`fail` keep their `?` — those are the real ack primitives.

4. **A poison page recomposed every tick forever (R06#3).** No attempt counter, no backoff, no terminal state — one LLM call per tick indefinitely, never producing a revision, while crowding healthy pages out of the tick limit. Migration 0021 adds `compose_attempts` + `compose_next_at`; the dirty scan skips pages in their backoff window; failures stamp `base * 2^(attempts-1)` capped at 1h; `insert_revision` clears both on success. `compose_attempts` also makes the stuck state queryable rather than invisible. Recording the backoff is **best-effort** — applying the very lesson from #3: bookkeeping about a failure must not turn one page's error into an aborted tick.

5. **The breaker released the whole herd at half-open (R13#4).** Half-open was modelled by *clearing* `open_until`, so the first caller past the cooldown let everyone through — each burning a full retry storm against a still-dead upstream before any reached `record_failure`. State also spanned a Mutex and a separate atomic, so `check` and `record_failure` never saw a consistent snapshot. Unified under one lock with a single probe token: others fail fast, a successful probe closes, a **failed probe re-opens immediately** (not after `threshold` more failures). The token is timestamped, not a bool — `send()` can exit without recording an outcome, and a never-returned token would wedge the breaker half-open forever.

6. **Live bearer tokens and JWTs survived redaction (R13#3).** The module doc claims bearer coverage that no rule implemented, and `\btoken\b` cannot match inside `access_token`/`refresh_token` (`_` is a word char, so there's no boundary) — so the most common OAuth key names were missed entirely, stored as memory bodies and served verbatim through `memory_provenance`. Added bearer + raw-JWT rules and an optional `(?:[a-z0-9]+[_-])?` prefix on the key-name rule. Redaction stays idempotent (asserted).

7. **The ingest feed poll was an un-throttled storm (C08#2).** A fixed 6s interval with no in-flight guard stacked overlapping requests when the feed was slow (checking the `refreshing` *state* can't prevent it — state is async), and hammered a down endpoint every 6s indefinitely. Replaced with a self-scheduling loop that awaits each request before scheduling the next (overlap is structurally impossible), a synchronous ref guard, exponential backoff (6s → 60s, reset on success), and an AbortController so a late response can't clobber fresher state.

## ⚠ Verify against a DB

Migration **0021** and the `make_interval` backoff expression are unrun here (no Postgres). `dirty_documents` now depends on 0021 having applied. `sqlx::migrate!` embeds it, so `migrate()` applies it on startup and in pg tests — but run `compose_pg` / `docs_pg` / `publish_pg` to confirm.

## Patterns established (catalogue items 16–18)

16. **Bookkeeping must never gate the primary path.** Both the run-row write and the compose-backoff write are *about* an outcome; propagating their errors turned a losable row into a dead-lettered source and (nearly) one page's failure into an aborted tick. If the doc says "losing this is acceptable", the `?` must agree.
17. **Retry needs a schedule, not just a flag.** "Stay dirty and retry" is correct in intent and unbounded in practice. Any retry loop over a paid resource needs an attempt counter, exponential backoff, a cap, and a queryable stuck state — the ingest queue had all four; compose had none.
18. **A single-admission token must be timestamped, not boolean.** Any "one probe / one leader" flag whose holder can vanish (dropped future, early bail) wedges forever as a bool. Timestamp it and let a stale holder be taken over.

## What remains

~40 findings fixed across Waves 1–7 + R01#1. Wave 8 (console session/route auth: C15 keyless cookie, open redirect, no rate-limit; C04 `.txt` matcher), Wave 9 (correctness edge cases), ~48 M/L refactor tail.
