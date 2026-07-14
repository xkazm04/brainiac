> Context: Console: Disputes Bench
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. Resolve action mutates as a single shared token — the "maintainer of the owning team" check never sees the acting person
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: auth-scoping
- **File**: console/app/console/(modules)/disputes/actions.ts:35 (and the misleading 403 copy at :19-20)
- **Scenario**: Anyone who knows the deployment's `CONSOLE_PASSCODE` opens `/console/disputes`, selects any team's disputed memory (payments, data, platform…) and clicks "they're right" → `resolveDisputeAction` runs `resolveDispute(configFromEnv(), memoryId, resolution)`, which posts with the single privileged `BRAINIAC_API_TOKEN`. The middleware gate (auth.ts / middleware.ts) authenticates "someone who knows the passcode", not "Petra" — there is no per-user identity at the console tier.
- **Root cause**: The action performs a privileged, org-wide mutation with zero authorization logic of its own, delegating the "maintainer of the owning team" decision entirely to the API's RLS. But that RLS runs against the *shared* server token, not the human clicking. So per-team maintainer scoping is enforced against a constant, not the actor — a documented v0 property (auth.ts header) that this write surface silently inherits.
- **Impact**: Team-level authorization is effectively collapsed for every dispute resolution: a person entitled only to payments can deprecate/re-verify a data-team memory. The 403 handler's reassurance ("You need to be a maintainer of the owning team") is misleading — that message can only ever appear from a *deployment-token* misconfiguration, never as a real per-user guard, so operators are lulled into thinking maintainer scoping is enforced when it is not.
- **Fix sketch**: Until OIDC/SCIM lands, either (a) narrow the console token's team scopes and stop advertising per-team maintainer enforcement in the UI copy, or (b) forward an acting-principal header derived from the (future) per-user session so the API can scope the mutation to the real actor. Minimum: correct the 403 copy so it doesn't imply per-user authorization the console cannot provide.

## 2. Concurrent adjudicators can silently overwrite each other's verdict — no optimistic-concurrency token
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: concurrency-lost-write
- **File**: console/app/console/(modules)/disputes/actions.ts:30-47; guard is only DecisionBar.tsx:55 (`disabled={pending || !live}`)
- **Scenario**: Two maintainers both have the bench open on the same hot memory. One clicks "still true" (reverified), the other clicks "they're right" (deprecated) within the same second. `disabled={pending}` only suppresses a second click *in the same tab*; across two sessions both `resolveDisputeAction` calls fire. `resolveDispute` sends only `{resolution}` — `FlaggedMemory` carries no version/etag and the POST has no `If-Match`/`expected_status`.
- **Root cause**: The client's double-submit protection is per-component `useTransition` pending state; there is no shared/optimistic-concurrency mechanism, and the payload has nothing the server could use to reject a stale second write. The console can only detect a collision through the server's 404 mapping.
- **Impact**: If the server applies the last write, a "reverified" is silently overwritten by "deprecated" (or vice-versa) — the contradiction/claim record ends on a verdict neither maintainer individually chose. Because `revalidatePath` succeeds for both, **both** adjudicators see a green success toast; neither learns theirs was overwritten. This corrupts the trust signal the whole bench exists to protect.
- **Fix sketch**: Thread the memory's current version (or the `oldest_claim_secs`/claim-set fingerprint) into `resolveDispute` as an optimistic token; treat a 409/404 not as generic "gone" but as "another maintainer already answered — here is their verdict, re-review" and re-fetch instead of reporting success.

## 3. Disputes is a write surface but uses `withDemoFallback`, violating the documented reviews invariant
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: demo-safety-invariant
- **File**: console/app/console/(modules)/disputes/page.tsx:17-27
- **Scenario**: When the brainiac server is unreachable, the page swaps to `DEMO_DISPUTES` (fabricated memory ids like `d1e0a5c2-…`) via `withDemoFallback` and renders the real `DecisionBar` over them. demo-fallback.ts's own header calls out reviews as a "Deliberate exception… a write surface (approve / reject / resolve), so a fabricated queue wired to real actions would be dangerous. It does NOT use this helper — it hard-stops with `<ApiOffline />`." Disputes is the *same* kind of write surface yet takes the demo path.
- **Root cause**: The single thing preventing fabricated ids from reaching the live action is `disabled={pending || !live}` in DecisionBar — one client-side boolean. There is no action-side backstop (the action can't tell demo from live), so the invariant that write surfaces hard-stop offline is broken here and enforced only by presentation.
- **Impact**: Any future refactor that renders a DecisionBar without threading `live`, or a transient `live` flip mid-session, would wire demo ids to the real resolve endpoint. Today the blast radius is small (demo ids 404), but the surface violates a stated safety invariant and relies entirely on a single prop for correctness — exactly the drift demo-fallback.ts was written to prevent.
- **Fix sketch**: Match reviews — hard-stop the disputes page with `<ApiOffline />` (or an equivalent) on fetch failure instead of `withDemoFallback`, or make the action itself reject non-live/demo ids so safety isn't solely presentational.

## 4. `describe()` / result type / revalidate boilerplate duplicated between disputes and reviews actions
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/app/console/(modules)/disputes/actions.ts:10-22,37-38 (twin of reviews/actions.ts:13-25)
- **Scenario**: `DecisionResult {ok, message}` is byte-identical to reviews' `ActionResult`; `describe(e)` differs only in the 404 string ("Memory is gone or already answered." vs "Item is gone or already reviewed."); and every action repeats the same `try { …; revalidatePath("/console/<surface>"); revalidatePath("/console/analytics"); return ok } catch { return describe }` envelope.
- **Root cause**: The two governance action files grew in parallel and each re-implemented the shared server-action envelope rather than importing it. The comment in review-buttons.tsx ("every governance surface answers the same way") confirms the surfaces are meant to be uniform, but the action-layer plumbing was copied, not shared.
- **Impact**: Divergence risk — a fix to error mapping or an added revalidate target has to be made in N places and has already drifted (the 404 wording differs where it likely shouldn't). Pure maintenance cost, no behavior change.
- **Fix sketch**: Extract a shared `ActionResult`, a `describeApiError(e, notFoundMsg)` helper, and a `runGovernanceAction(fn, revalidate: string[])` wrapper into a common module (e.g. under `@/lib`); have both disputes and reviews actions import them.

## 5. Dead `size` / `onDone` props (and the `size==="sm"` branch) on DecisionBar
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/app/console/(modules)/disputes/DecisionBar.tsx:23-46 (`onDone` at 23,30,39,44,46; `size` at 42,47)
- **Scenario**: DecisionBar's only caller is DisputeBench.tsx:206, `<DecisionBar memoryId={active.memory_id} live={data.live} />` — neither `size` nor `onDone` is ever passed. `size` therefore always defaults to `"md"`, making the `"sm"` pad branch (`px-2.5 py-1 text-xs`) unreachable, and `onDone` is always undefined, so `if (r.ok) onDone?.()` (and the `onDone` param threaded through `useDecision`) is permanently a no-op.
- **Root cause**: Speculative generality — the component was built with sizing and a completion callback for reuse that never materialized (grep confirms a single call site).
- **Impact**: Misleads the next reader into thinking selection auto-advances on success via `onDone` (it doesn't — the board only refreshes through `revalidatePath`), and carries an unexercised style branch. Minor cruft, not a runtime bug.
- **Fix sketch**: Drop `size`/`onDone` from the props, `useDecision`, and the JSX until a second caller actually needs them; inline the `md` padding. If a completion hook is wanted later, reintroduce it wired to a real caller.
