# Fix Wave 4 — Timeouts / Hangs

> 3 commits, 4 findings resolved (1 Critical + 3 High). This closes the LAST
> Critical — all 5 are now fixed. Gates green: `cargo check --workspace
> --all-targets` 0 errors · console `tsc` 0 · vitest 50/50.

## Commits

| # | Commit | Findings | Severity | Files |
|---|---|---|---|---|
| 1 | `…` | R13 provider HTTP timeout | Critical | `gateway/lib.rs` |
| 2 | `…` | R17 Confluence timeout | High | `publish/confluence.rs` |
| 3 | `…` | C03 + C04 console/route timeout | 2 High | `console/src/lib/api.ts` |

## What was fixed

1. **Provider HTTP calls had no timeout (R13, Critical — the last Critical).** `QwenProvider`/`QwenEmbedder` used `reqwest::Client::new()`, which sets no timeout, so a stalled-but-connected upstream made `.send()`/`.text()` never return — neither an error nor a status, so the resilience retry loop never fired and the circuit breaker never opened. One dead endpoint pinned a worker task forever. Added `build_http_client()` with `connect_timeout` (10s) + total `timeout` (60s), tunable via `BRAINIAC_GATEWAY_CONNECT_TIMEOUT_SECS` / `BRAINIAC_GATEWAY_REQUEST_TIMEOUT_SECS`. reqwest's timeout is per `send()`, so it bounds each resilience attempt while the retry loop still governs totals — a stall now becomes a retryable/breaker-recordable error.

2. **Confluence client had no timeout (R17, High).** Same `Client::new()` gap, but the call sits inside a live `store.scoped_tx` in `publish_org`, so a hung wiki pinned a DB transaction open indefinitely, starving the pool. Built the client with `connect_timeout(10s)` + `timeout(30s)`.

3. **Console/route fetches had no timeout (C03 + C04, both High, one root cause).** The shared `call()` transport in `lib/api.ts` fetched with no timeout, so a stalled backend hung the awaiting server component — and every console route and server action goes through it. A hang also slipped past `withDemoFallback`'s offline net. Attached `AbortSignal.timeout` (15s default, `BRAINIAC_API_TIMEOUT_MS`) and convert a timeout/transport error into an `ApiError` (504/0). Fixes both the direct-client hang and the route-proxy hang.

## Verification

| Gate | Result |
|---|---|
| `cargo check --workspace --all-targets` | 0 errors |
| console `tsc --noEmit` | 0 errors |
| console vitest | 50/50 (the timeout signal doesn't break the fetch mocks) |

## Patterns established (catalogue item 10)

10. **A hang is neither an error nor a status — bound every network call.** `reqwest::Client::new()` and browser `fetch` apply NO default timeout, so a stalled-but-connected upstream produces a future that never resolves, silently defeating retry loops, circuit breakers, and fallback nets. Build clients with `connect_timeout` + total `timeout` (Rust) / attach `AbortSignal.timeout` (JS), and convert the timeout into a typed error. Especially critical when the call is held inside a DB transaction. (R13, R17, C03/C04)

## What remains / follow-ups

- **Follow-up (noted in the C03/C04 commit):** `console/src/lib/governance-api.ts` hand-rolls its own untimed fetch (a C03 secondary + a code-refactor "forked transport" finding); it sits in an in-progress console refactor, so it was left untouched. Fold it onto the timed `call()` client.
- Waves 5–9 + refactor tail per the INDEX: silent-failures/success-theater (Wave 5), eval-gate integrity (Wave 6), reliability (Wave 7), console session/route auth (Wave 8), correctness edge cases (Wave 9), ~48 M/L refactor.

## Milestone

All 5 Criticals are now closed (R04, C05, R07, R06, R13) across Waves 1–4. 21 findings fixed total; ~139 open (mostly High/Medium/Low).
