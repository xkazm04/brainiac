> Context: Console: API Client + Types + Auth
> Total: 5 (Critical: 0, High: 3, Medium: 2, Low: 0)

## 1. Every REST call fetches without a timeout — a hung server hangs the console
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: fetch-no-timeout
- **File**: console/src/lib/api.ts:70-79 (and console/src/lib/governance-api.ts:12-15, 25-33)
- **Scenario**: The brainiac server accepts the TCP connection but never sends response bytes (stuck DB query, deadlocked sweep worker, half-open connection, an upstream proxy that holds the socket). `doFetch(...)` has no `AbortController`/`signal` and no timeout, so the `await` never resolves.
- **Root cause**: `fetch` has no default timeout; the client passes only `method`/`headers`/`body`/`cache`. Every endpoint (`call`, plus governance-api's `call`/`post`) shares this one code path, so the gap is total.
- **Impact**: The server component / server action that awaited the call blocks indefinitely, holding the render until the platform's own (much longer) timeout kills it. For a page that awaits several of these sequentially, one slow endpoint stalls the whole page. Because there is no bounded failure, `withDemoFallback` never gets a chance to fire — it only catches *throws*, and a hang never throws — so the "offline → demo fixture" safety net silently does not engage.
- **Fix sketch**: Add a shared timeout in `call`/`post`: `const signal = AbortSignal.timeout(cfg.timeoutMs ?? 10_000)` passed into `doFetch`, translating the resulting `AbortError`/`TimeoutError` into an `ApiError(504, ...)`. Put it in the single `call` in api.ts and reuse it from governance-api (see finding 5).

## 2. `withDemoFallback` swallows every error — auth failures and server 500s become plausible fixtures
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: masked-outage
- **File**: console/src/lib/demo-fallback.ts:26-35 (bare `catch {}` at 32-34)
- **Scenario**: The server is *up and reachable* but returns 401/403 (token missing/lacks the `read` scope — exactly the case api.test.ts:96 constructs) or 500 (a real server bug). `fetchLive` throws a typed `ApiError` carrying that status and message; the bare `catch {}` discards it and returns the demo fixture with `live:false`.
- **Root cause**: The helper treats *every* throw as "server unreachable / offline" and keeps no signal of *why*. The `catch` binds nothing and logs nothing, so the status/message the client worked to build in `call` (api.ts:80-96) is dropped on the floor.
- **Impact**: A misconfigured `BRAINIAC_API_TOKEN`, a revoked token, an RLS scope gap, or a 500-throwing endpoint are all indistinguishable from a clean offline state. The operator sees fabricated tokens/memories/graph nodes under only a generic "demo" banner and has no way to learn the token is wrong or the server is erroring — the actionable failure is invisible. It also masks bugs in the *fetch-composing closure itself* (e.g. the `async () => ({ live, overview: await getGraphOverview(cfg) })` wrapper in demo-fallback.test.ts:45): any TypeError there is silently treated as "offline".
- **Fix sketch**: Log/record the caught error (at least `console.error` server-side, ideally return it: `DemoResult<T>` gains `reason?: { status?: number; message: string }`) so the banner can distinguish "offline" from "auth/permission error — check BRAINIAC_API_TOKEN". Optionally do NOT fall back on 401/403 (config error, not an outage) and surface it like the reviews write-surface hard-stop.

## 3. Unvalidated `res.json() as T` — a 200 with the wrong/empty body silently becomes wrong data
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: type-lie-unvalidated-json
- **File**: console/src/lib/api.ts:98 (`return (await res.json()) as T;`); envelope unwraps at api.ts:112, 121, 138, 212-218, 233, 255; also governance-api.ts:20, 38, 72-76
- **Scenario**: A `200 OK` arrives whose body is not the promised shape: an intermediary/login proxy returns an HTML page with status 200, an older server omits the `hits`/`promotions`/`tokens` envelope key, or a partial payload. `res.json()` on HTML throws (masked per finding 2); a *valid* JSON of the wrong shape is cast to `T` with zero runtime checks, and `out.hits` / `out.promotions` / `out.tokens` destructure to `undefined`.
- **Root cause**: The generated types (`types.ts`) give *compile-time* confidence that console and server agree, but there is no *runtime* guard — `as T` asserts a shape the wire is merely assumed to have. The envelope-unwrap helpers (`out.hits` etc.) compound it: they trust a nested key exists.
- **Impact**: `searchMemories` returns `undefined` typed as `SearchHit[]`; a consumer that maps over it crashes, or (worse) renders it as "0 results" — a silent *wrong-data* outcome that looks like a legitimate empty state. No error, no demo banner, because the fetch "succeeded".
- **Fix sketch**: Validate at the boundary — a lightweight zod/valibot schema per response (or at minimum guard the envelope: `if (!out || !Array.isArray(out.hits)) throw new ApiError(502, "malformed response")`). Centralize in `call` so every endpoint benefits.

## 4. `call` always parses JSON — a 204/empty body on a `void` write endpoint throws on success
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: empty-response-204
- **File**: console/src/lib/api.ts:98 (unconditional `res.json()`); reached by `revokeToken` (api.ts:250-252), also `runSweep`/`reviewPromotion`/`approveDocRevision`
- **Scenario**: `revokeToken` is typed `Promise<void>` yet still `await call(...)`, and `call` unconditionally runs `return (await res.json()) as T`. If the server answers a revoke/run/approve with `204 No Content` or an empty `200` body, `res.json()` throws `SyntaxError: Unexpected end of JSON input` even though the mutation succeeded server-side.
- **Root cause**: `call` has one success path that assumes every endpoint returns a JSON document; it has no branch for `204`/empty/`content-length: 0`, and no way for a `void` caller to opt out of parsing.
- **Impact**: A successful token revocation (or sweep-run/approve) is reported to the operator as a failure. Token management is a write surface not wrapped in demo-fallback, so the spurious error surfaces directly — the operator retries a revoke that already happened, or believes it failed.
- **Fix sketch**: In `call`, short-circuit no-content: `if (res.status === 204 || res.headers.get("content-length") === "0") return undefined as T;` — or read `text()` and `return (text ? JSON.parse(text) : undefined) as T`. Have `revokeToken` pass `<void>` explicitly.

## 5. governance-api.ts re-implements api.ts's fetch/error boilerplate and hand-rolls types the schema already generates
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/src/lib/governance-api.ts:10-39 (dup `call`/`post`) and :43-69 (hand-rolled `PromotionMemory`/`PromotionProvenance`/`PromotionQueueItem`)
- **Scenario**: governance-api.ts copies the whole request/error/URL-building shape of api.ts's `call` (bearer header, `cache:"no-store"`, `!res.ok` → `text()` → `ApiError`) into two near-identical functions, and hand-defines `PromotionMemory`/`PromotionProvenance` interfaces that already exist as generated aliases in types.ts:23-24 (`S["PromotionMemory"]`, `S["PromotionProvenance"]`). Both files also expose an endpoint for the same path `/v1/reviews/promotions` (api.ts `pendingPromotions` → `PendingPromotion[]` vs governance-api `promotionQueue` → `PromotionQueueItem[]`).
- **Root cause**: The file was split off "so the richer queue types stay grouped," but it forked the transport instead of importing it, and it predates (or ignores) the generated-schema contract that types.ts is built on. Its copy is also subtly *divergent*: governance-api's error path (:16-19, :34-37) uses raw `text` as the message and does NOT parse the `{error, code}` envelope that api.ts:80-96 unwraps — so the two clients render the same server error differently (one shows `token lacks scope`, the other shows the raw `{"error":"...","code":"..."}` blob).
- **Impact**: Two transports to maintain (the timeout fix in finding 1, the 204 fix in finding 4, and any header/auth change must be made twice and can drift); hand-rolled promotion types can silently diverge from the Rust structs the generated schema tracks — defeating the "cannot drift from the API" guarantee types.ts's header advertises; inconsistent error messages across surfaces.
- **Fix sketch**: Export the `call`/`post` core from api.ts (with a `method` param and optional-body handling) and have governance-api import it, deleting its copies; replace the hand-written `PromotionMemory`/`PromotionProvenance` with the generated aliases; reconcile the two `/v1/reviews/promotions` wrappers to one.
