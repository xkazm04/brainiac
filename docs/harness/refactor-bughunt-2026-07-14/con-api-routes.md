> Context: Console: Next.js API Routes
> Total: 5 (Critical: 0, High: 2, Medium: 3, Low: 0)

Scope note: `middleware.ts` DOES gate `/api/*` (line 42–47 returns a 401 for anonymous callers, and the matcher on line 58–60 covers these routes), and every `[id]` param is strictly hex-validated before it reaches the backend URL — so the "unauthenticated confused-deputy" and "SSRF via `[id]`" that this unit was fished for are **not present as working exploits**. The honest top findings are a middleware/route coupling that leaves the id-regex load-bearing for auth, an un-timed upstream fetch, an all-or-nothing feed aggregation, a validation gap on token minting, and the per-route boilerplate. No fabricated Critical.

## 1. GET-by-id routes are reachable without a session; the hex id-regex is the sole gate before the admin token is used
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: auth-boundary
- **File**: console/app/api/memories/[id]/route.ts:10-13 (identical: console/app/api/graph/canonical/[id]/route.ts:10-13)
- **Scenario**: `middleware.ts`'s matcher (line 59) excludes any path ending in an image/`.txt` extension: `.*\.(?:svg|png|jpg|jpeg|gif|webp|ico|txt)$`. Because these two routes end with the dynamic `[id]` segment, a request to `GET /api/memories/anything.txt` (or `…/x.png`) never runs the middleware session check — it lands directly in the handler with `id = "anything.txt"`, unauthenticated. The handler is only saved by `/^[0-9a-f-]{36}$/i.test(id)` returning false → 400, *before* `getMemoryDetail(configFromEnv(), id)` fires the admin-token backend call. (Note `keys/[id]/revoke` is safe here — its path ends in `revoke`, not the id, so the extension exclusion can't apply.)
- **Root cause**: These handlers do no in-body auth check of their own; they assume middleware always runs. But the matcher's extension carve-out (meant for static assets) also carves a hole for any dynamic route whose last segment can carry an extension. The id-regex was written as input hygiene, yet it is now the *only* control standing between an anonymous caller and an admin-token proxy call for extension-suffixed URLs.
- **Impact**: Today it is non-exploitable (the regex rejects `foo.txt`), but the security property rests entirely on one regex line that a future refactor could loosen (e.g. accepting slugs, or `startsWith`-style checks) — at which point it becomes a real pre-auth admin-token backend call. Auth-by-input-validation-coincidence is fragile and undocumented.
- **Fix sketch**: Don't rely on the matcher for auth on dynamic routes. Either add an explicit `isValidSession` check at the top of each token-using handler (defense-in-depth), or tighten the middleware matcher so `/api/*` is unconditionally included regardless of suffix (e.g. add a dedicated `"/api/:path*"` matcher entry). Keep the id-regex, but stop treating it as the auth gate.

## 2. Upstream fetch has no timeout — a stalled backend hangs the handler (worst on the polled feed route)
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: hang / resource-exhaustion
- **File**: console/app/api/ingest/feed/route.ts:15-19 (shared root cause: `call()` in src/lib/api.ts:71 sets no `signal`; affects every route in this unit)
- **Scenario**: The feed handler awaits `Promise.all([getSourcesFeed, getPipelineRuns, getQueueHealth])`, each of which reaches `fetch(\`${baseUrl}${path}\`, { … })` with no `AbortSignal`. If the Rust backend on :8600 accepts the TCP connection but stalls (deadlocked worker, paused GC, half-open socket), the handler blocks. Node/undici's defaults only cap this at ~300s (headers/body timeout), not immediately — and this is the route the client *polls continuously*, so hung requests pile up.
- **Root cause**: The shared `call()` helper was written for a trusted localhost backend and omits a request deadline. Every route inherits it, but the feed route amplifies it: three parallel un-timed fetches behind a polling client.
- **Impact**: A single unresponsive backend endpoint ties up server request slots for up to ~5 minutes each, multiplied by poll frequency and viewer count — a self-inflicted DoS of the console, with no error surfaced to the operator until the long timeout finally elapses.
- **Fix sketch**: Give `call()` an `AbortSignal.timeout(cfg.timeoutMs ?? 8000)` (or wrap each route's await in `Promise.race` with a timeout). A fast, explicit 502/504 is far better than a 5-minute silent stall on a polled endpoint.

## 3. Feed route is all-or-nothing: one failing/slow upstream blanks the entire monitor, and every error flattens to 502
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: reliability / silent-degradation
- **File**: console/app/api/ingest/feed/route.ts:15-26
- **Scenario**: `Promise.all` rejects as soon as *any* of the three upstream calls fails, so if only `getQueueHealth` errors (or is slow), the client receives no `sources` and no `runs` either — the whole monitor feed goes dark even though 2/3 of the data was available. Separately, the `catch` here always returns `{ status: 502 }` and never inspects `ApiError.status`, unlike the other seven handlers in this unit that map it through.
- **Root cause**: `Promise.all` chosen for brevity where partial results are actually acceptable (it is a read-only dashboard aggregation). The 502-only catch is an inconsistency where this route diverged from the shared error-mapping pattern used everywhere else.
- **Impact**: Transient trouble on one backend subsystem produces a fully blank monitor rather than a degraded one, and the operator can't tell a 404/400 from a genuine gateway failure because every upstream status is rewritten to 502.
- **Fix sketch**: Use `Promise.allSettled` and return whatever resolved (with per-section `null`/error markers the UI can degrade on), and reuse the same `ApiError`-aware status mapping the other routes use.

## 4. `createToken` forwards an unvalidated `user_id` and mistyped `scopes` straight to the mint endpoint
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: missing-validation
- **File**: console/app/api/keys/route.ts:30-36
- **Scenario**: On `POST /api/keys`, `user_id` is accepted as *any* string (`typeof body.user_id === "string" ? body.user_id : undefined`, line 34) with no format check, and `scopes` is passed through as `Array.isArray(body.scopes) ? (body.scopes as string[]) : undefined` (line 35) — the `as string[]` cast is a lie: a caller can send `scopes: [123, {}, ["x"]]` and the array of non-strings is forwarded to `/v1/tokens` untouched. Note the inconsistency: `keys/preview` (line 12) and `keys/[id]/revoke` (line 10) both enforce a strict UUID regex on their user/id input, but the *minting* path — the most privileged of the three — enforces none.
- **Root cause**: The `name` field got proper validation but `user_id`/`scopes` were treated as opt-in pass-throughs; the `as string[]` cast silenced the type system instead of validating.
- **Impact**: Malformed principal ids and non-string scope entries reach the token-minting backend, relying entirely on the Rust side to reject them; a minted token could be bound to a garbage `user_id` or carry structurally invalid scopes, and the BFF's own contract (`string[]`) is violated silently.
- **Fix sketch**: Validate `user_id` with the same UUID regex already used in `preview`/`revoke`, and validate `scopes` with `body.scopes.every(s => typeof s === "string")` (reject otherwise) instead of casting.

## 5. Error-mapping and id-validation boilerplate duplicated across seven of the eight handlers
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/app/api/keys/route.ts:11-17 & 38-44 (also preview:17-23, revoke:16-22, ingest:20-26, memories/[id]:16-22, graph/canonical/[id]:16-22)
- **Scenario**: The exact block `const status = e instanceof ApiError ? e.status : 502; return NextResponse.json({ error: e instanceof Error ? e.message : "upstream unavailable" }, { status });` is copy-pasted verbatim in **seven** handlers (an eighth, `feed`, is a drifted variant that forgot the `ApiError.status` mapping — see finding 3). Likewise `/^[0-9a-f-]{36}$/i` is hand-repeated in four files (revoke:10, memories:11, canonical:11, preview:12). Every copy also passes the backend's `e.message` straight to the browser, so any leak or inconsistency fix must currently be applied in eight places.
- **Root cause**: No shared proxy/wrapper helper for "call the backend, validate the id, map ApiError → HTTP, return JSON." Each route was written by hand from the same template, and the template has already drifted once (feed).
- **Impact**: Eight-fold maintenance cost and guaranteed drift (already happened); a single decision — e.g. "stop echoing raw backend `e.message` to the client" or "always map `ApiError.status`" — cannot be made in one place. Pure cleanup, but it is the structural reason findings 2–4 each recur across files.
- **Fix sketch**: Extract `withUpstream(fn)` (runs the call, catches, maps `ApiError.status` → response, returns a sanitized error envelope) and an `isUuid(id)` / `requireUuidParam` helper in `src/lib`; each route collapses to validate-then-call. This also becomes the single home to add the timeout (finding 2) and to decide the error-passthrough policy.
