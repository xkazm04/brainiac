> Context: Gateway: BYOM Model + Resilience + Health/Redact
> Total: 5 (Critical: 1, High: 3, Medium: 1, Low: 0)

## 1. Provider HTTP calls have no timeout — a stalled upstream hangs the worker forever
- **Severity**: Critical
- **Lens**: bug-hunter
- **Category**: network-hang
- **File**: crates/brainiac-gateway/src/lib.rs:66,181 (client construction) + crates/brainiac-gateway/src/resilience.rs:152-192 (send/text)
- **Scenario**: DashScope (or any BYOM endpoint) accepts the TCP connection and then stalls — never sends response headers, or trickles the body a byte at a time (slow-loris, LB half-close, model overload). `QwenProvider`/`QwenEmbedder` build their client with `reqwest::Client::new()` (no connect/read timeout), and the request in `resilience::send` (`this_try.send().await` then `resp.text().await`) is never wrapped in `tokio::time::timeout`. A confirming grep found zero `.timeout(`, `Client::builder`, or `ClientBuilder` in the crate.
- **Root cause**: reqwest applies NO default timeout; the resilience layer was designed to handle *errors* and *non-2xx statuses*, but a hang produces neither an `Err` nor a status. The design assumed every call eventually returns.
- **Impact**: The awaiting future never resolves. The retry loop never fires (no error/status to react to) and the circuit breaker never records a failure (so it never opens to protect the rest of the queue). One dead-but-connected upstream pins an ingest/embedding worker task indefinitely; enough of them starve the pipeline. This is the crown-jewel hang.
- **Fix sketch**: Build clients via `reqwest::Client::builder().connect_timeout(..).timeout(..).build()` (e.g. 10s connect, 60s total), and/or wrap each attempt in `resilience::send` with `tokio::time::timeout(per_attempt, this_try.send())` so a stall is converted into a retryable/breaker-recordable error. Make the per-attempt budget configurable alongside the existing `BRAINIAC_GATEWAY_*` knobs.

## 2. Missing/empty provider `usage` silently meters 0 tokens and empty `choices` returns a fake success
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: metering-bypass / success-theater
- **File**: crates/brainiac-gateway/src/lib.rs:100-112,140-154
- **Scenario**: DashScope returns 200 but omits the `usage` object (some OpenAI-compatible gateways do on certain paths, on content-filter stubs, or on streamed-then-aggregated bodies). Because `usage` is `#[serde(default)] Option`, and `prompt_tokens`/`completion_tokens` are each `#[serde(default)]`, `unwrap_or(OpenAiUsage{0,0})` records **0 input / 0 output tokens** for a real billed call. Separately, if `choices` is empty (filtered, or an error shaped as 200), `choices.first()...unwrap_or_default()` yields `text = ""` and the function still returns `Ok`.
- **Root cause**: The parser is maximally lenient to avoid hard-failing on schema drift, but leniency on the metering fields turns "we don't know the cost" into "the cost was zero", and leniency on `choices` turns "no answer" into "empty answer, success".
- **Impact**: BYOM per-call usage accounting under-records — an org's own key is spent while the gateway's usage ledger reads 0, so quotas/analytics/cost attribution are wrong and calls can slip metering entirely. The empty-`choices` path is success theater: the pipeline extracts from `""` (0 memories) and nothing goes red, exactly the "served garbage as truth" failure the health breaker was built to catch.
- **Fix sketch**: Treat a missing `usage` as an explicit signal (log/flag `usage_missing`, or estimate from prompt length) rather than silently 0; and turn an empty `choices` (or empty `content` when `json_mode`) into an `Err("<what>: provider returned no choices")` so it fails loudly / retries instead of returning a hollow `Ok`.

## 3. Redaction misses bearer/JWT and compound key names (`access_token`, `refresh_token`) despite claiming bearer coverage
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: secret-leak
- **File**: crates/brainiac-core/src/redact.rs:33-63 (module doc claim at :12-13)
- **Scenario**: A transcript contains `Authorization: Bearer eyJhbGci...` (a raw JWT), or `access_token= 9f8c7b6a5e4d3c2b1a09` / `refresh_token: "…"`. None are masked. The key-name rule (:58-61) alternates on `token`, but `\btoken\b` cannot match the `token` inside `access_token`/`refresh_token` because the preceding `_` is a word char (no word boundary) — and none of `access[_-]?key`, `secret`, etc. match `access_token` either. There is also no `Authorization`/`Bearer`/JWT (`eyJ...`) rule at all, even though the module doc explicitly claims "bearer … secrets" are covered.
- **Root cause**: The pattern set enumerates provider *prefixes* and a fixed key-name list, and the `\b…\b` anchoring silently excludes the most common compound OAuth key names; the "bearer" coverage claimed in the doc was never implemented.
- **Impact**: Live bearer tokens / OAuth access & refresh tokens survive `redact()` and are stored as memory bodies and served verbatim through `memory_provenance` to any agent RLS admits — the exact "credential pasted into a session becomes a team-visible secret" breach this module exists to stop. `contains_secret()` also returns false, so nothing flags it.
- **Fix sketch**: Add a rule for `(?i)\b(?:bearer)\s+([A-Za-z0-9._\-]{20,})` and a JWT shape `\beyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\b`; and change the key-name rule to allow a leading segment (e.g. `(?:[a-z]+[_-])?(?:access|refresh|api|auth)?[_-]?(?:key|token|secret|password))` or drop the leading `\b` so `access_token`/`refresh_token`/`auth_token` match. Add these shapes to the redact tests.

## 4. Circuit-breaker half-open admits the entire waiting herd (non-atomic state transition)
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: circuit-breaker-race
- **File**: crates/brainiac-gateway/src/resilience.rs:104-116,122-133
- **Scenario**: The circuit is open and many queued ingest jobs are blocked on the same `Resilience`. At cooldown expiry the *first* `check()` sets `*open = None` and returns `Ok`; every subsequent concurrent `check()` now also sees `None` and returns `Ok`. So instead of the "one probe call" the doc promises (:102-103), the whole herd is released at once against a still-dead upstream, each running a full `max_attempts` retry storm before any of them reaches `record_failure()` to re-open. State is also split across two independently-locked fields (`open_until` Mutex + `consecutive_failures` Atomic), so `check` and `record_failure` never see a consistent snapshot.
- **Root cause**: Half-open is modeled by clearing `open_until` rather than by admitting a single token; there is no "probe in flight" flag, so N concurrent callers all treat themselves as the probe.
- **Impact**: The breaker fails to fail-fast in exactly the scenario it exists for (dead upstream + backpressured queue): a thundering herd burns the collective retry/backoff budget and can hammer a recovering provider, delaying recovery and wasting the org's key spend. Defeats the "a dead upstream doesn't burn every queued job" guarantee.
- **Fix sketch**: Represent breaker state as a single `Mutex<State{ open_until, probing }>` (or CAS a `probing` atomic on half-open); admit exactly one probe, make other callers fail-fast until the probe's outcome closes or re-opens the circuit. Reset `consecutive_failures` when entering half-open so a stale count can't instantly re-trip.

## 5. DashScope client scaffolding duplicated across `QwenProvider` and `QwenEmbedder`
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-gateway/src/lib.rs:52-83,166-201
- **Scenario**: `QwenProvider` and `QwenEmbedder` each carry the same fields (`http`, `base_url`, `api_key`, `model`, `resilience`), each has its own `new`/`from_env` doing the same `reqwest::Client::new()` + `base_url.unwrap_or(DEFAULT_BASE)` + `Resilience::from_env()` wiring, and each builds a POST with the same `bearer_auth(&self.api_key).json(...)` → `resilience.send(..)` shape.
- **Root cause**: The embedder was added alongside the chat provider by copy-paste rather than factoring the shared DashScope transport into one place.
- **Impact**: Two copies drift independently, and — directly relevant to finding #1 — the missing-timeout fix must be applied in two constructors instead of one; likewise any auth/header/base-url change. Meaningful maintenance surface for a security-sensitive seam (the API key handling lives in both).
- **Fix sketch**: Extract a `DashScopeClient { http, base_url, api_key, resilience }` with `from_env()` and a `post_json(path, body, what) -> Result<String>` helper; have both `QwenProvider` and `QwenEmbedder` hold one and call it. This gives a single, timeout-configured client builder and one code path for bearer auth.
