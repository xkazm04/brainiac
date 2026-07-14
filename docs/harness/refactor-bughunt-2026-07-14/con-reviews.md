> Context: Console: Promotion Review Queue
> Total: 5 (Critical: 1, High: 2, Medium: 2, Low: 0)

## 1. Governance gate has no per-maintainer authorization — every passcode holder approves as the shared server principal
- **Severity**: Critical
- **Lens**: bug-hunter
- **Category**: authz-bypass
- **File**: console/app/console/(modules)/reviews/actions.ts:27-54
- **Scenario**: A person who is NOT a maintainer of the owning team, but who knows the single deployment console passcode, opens `/console/reviews` and clicks `approve`. `reviewPromotionAction` calls `reviewPromotion(configFromEnv(), id, "approve")`, which authenticates with `BRAINIAC_API_TOKEN` — one privileged server token shared by all console users (see api.ts:57-62 and the `authorization: Bearer ${cfg.token}` in reviewPromotion). The API's maintainer check (the `403` handled at describe() line 20) is evaluated against *that shared principal*, not the human clicking. If the server token is a maintainer (it must be, or the whole surface 403s and is dead), then every passcode holder can approve/reject every promotion.
- **Root cause**: `lib/auth.ts` is explicit that the gate is a shared passcode — "it authenticates 'someone who knows the console passcode', not 'Petra'... Do not build per-user features on it." The reviews page builds exactly a per-maintainer, human-in-the-loop feature on top of it ("Sign what the org will remember... Every decision here is ledgered and signed."). The action layer adds zero console-side identity or maintainer assertion.
- **Impact**: The human maintainer requirement is bypassed to anyone with the passcode. Worse for a governance/audit surface: the ledger's actor is the shared server token, so "signed" cannot say *which* human approved — the audit trail's core claim is false, and non-repudiation is impossible.
- **Fix sketch**: Short term, since the token is shared, drop the "signed by you" framing — this is org-level, not per-maintainer, authorization; don't advertise individual signatures. Real fix: carry the logged-in principal (OIDC/SCIM per auth.ts §2.1) and either mint/forward a per-user API token or pass an actor id the server records as the reviewer, and enforce maintainer membership server-side against *that* identity, not the shared token.

## 2. No optimistic-concurrency guard — concurrent maintainers produce a conflicting / lost-update decision
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: lost-update-race
- **File**: console/app/console/(modules)/reviews/actions.ts:27-32 (and 41-47)
- **Scenario**: Two maintainers load the same pending promotion. A clicks `approve` (memory canonicalized); B, on a page rendered before A's decision, clicks `reject`. Both actions carry only the item `id` — no version, etag, or expected-current-status. B's POST reaches `/v1/reviews/promotions/{id}/reject` with nothing asserting "I expect this to still be pending." The only backstop is the server returning `404` (describe() line 21 → "Item is gone or already reviewed"), which requires the server to strictly single-decision the item; the console neither guarantees nor detects it. Same shape for two `resolveContradictionAction` calls picking different winners.
- **Root cause**: The action signature is `(id, action)` — it treats the decision as unconditional. The queue item (`PromotionQueueItem`) carries no revision/version token to send back as a compare-and-set precondition.
- **Impact**: On a governance gate this is the crown-jewel failure: an already-approved (canonicalized) memory can be flipped to rejected, or two contradictory resolutions land, silently corrupting the ledger. If the server is lenient the second write wins (lost update); if strict, B sees a confusing 404 with no indication A decided.
- **Fix sketch**: Thread an `expected_version`/`updated_at`/decision-nonce from the queue item through the action into the API call and have the server reject on mismatch (`409`), then surface "already decided by someone else — reloading" and force a refresh. At minimum, map the 404/409 to a distinct "someone else already decided" state rather than "gone."

## 3. `revalidatePath` refreshes only the acting client — other maintainers keep acting on phantom items
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: stale-state
- **File**: console/app/console/(modules)/reviews/actions.ts:33-34 (and 48-49); console/app/console/(modules)/reviews/review-buttons.tsx:21-27
- **Scenario**: `revalidatePath("/console/reviews")` invalidates the Next data cache and refreshes the RSC tree *for the client that invoked the action*. A second maintainer with the queue already open receives no invalidation — there is no `revalidateTag` broadcast, no polling, no SSE. `force-dynamic` (page.tsx:15) only guarantees freshness on a *new* navigation, not for an already-mounted page. So the second maintainer's queue keeps showing items that were already approved/rejected, and clicking them feeds directly into finding #2. On the acting client, after success the buttons re-enable (`disabled={pending}` only, review-buttons.tsx:47/55) and the `ResultNote` lingers, widening the window before the refresh repaints and the card drops.
- **Root cause**: Path-based revalidation is per-request/per-client; the queue is shared mutable state with multiple concurrent viewers, but nothing pushes invalidation to the others.
- **Impact**: Maintainers routinely see and act on a queue that no longer reflects reality — every stale click is a candidate lost-update. The "queue clear / N in queue" counts (page.tsx:147) are also stale for everyone but the last actor.
- **Fix sketch**: After a decision, have the client optimistically remove/lock the decided card and re-fetch (or `router.refresh()` on completion). For cross-maintainer freshness, add short-interval revalidation or an SSE/`revalidateTag` signal on the reviews queue so other open sessions drop decided items.

## 4. Rejected server action is swallowed — a failed decision shows no error (or blows up the whole queue)
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: console/app/console/(modules)/reviews/review-buttons.tsx:24-25
- **Scenario**: `const run = (fn) => startTransition(async () => setResult(await fn()));`. The server actions normally catch API errors and return `{ ok: false, message }`, so those *are* surfaced (good). But if the action *rejects* — network drop reaching the Next server, an RSC serialization error, or any throw before its own try — `await fn()` rejects, `setResult` never runs, and there is no `.catch`. The transition ends, `pending` flips back to false, and the maintainer sees the buttons simply re-enable with no note. Best case the failed approve/reject looks like a no-op the maintainer may not retry; worst case React escalates the rejected transition to the route error boundary (error.tsx) and unmounts the entire reviews page over a transient blip.
- **Root cause**: The helper assumes the action always resolves to an `ActionResult`; there is no rejection path, so infrastructure-level failures have no inline surface.
- **Impact**: On the governance path, "did my rejection go through?" must never be ambiguous. A silently dropped decision leaves a bad memory pending while the maintainer believes they acted; a boundary escalation loses the whole queue over a network hiccup.
- **Fix sketch**: Wrap the call: `startTransition(async () => { try { setResult(await fn()); } catch (e) { setResult({ ok: false, message: "Couldn't reach the server — nothing was changed. Try again." }); } })`.

## 5. Approve/reject action scaffolding is copy-pasted across reviews and disputes
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: console/app/console/(modules)/reviews/actions.ts:13-25 (also 31-38 / 46-53)
- **Scenario**: `ActionResult` (13-16) is structurally identical to disputes/actions.ts `DecisionResult` (10-13); `describe(e)` (18-25) is a near-verbatim copy of disputes/actions.ts `describe` (15-22) — same 403/404/fallback ladder, differing only in a noun ("Item"/"Memory"). The try / `revalidatePath("/console/…")` + `revalidatePath("/console/analytics")` / catch→`describe` envelope is repeated three times here and again in disputes. On the client, `useAction`+`ResultNote` (review-buttons.tsx:21-40) duplicate DecisionBar.tsx's `useDecision`+result-span.
- **Root cause**: Each governance module grew its own action file independently; the comment in review-buttons.tsx even notes the styling "matches the disputes DecisionBar," but the shared logic was never extracted.
- **Impact**: The 403/404 wording and the revalidate targets must be edited in lockstep across files; they have already drifted ("Item is gone" vs "Memory is gone"). A fix like #4's rejection handling or #2's 409 mapping now needs applying in every copy — the duplication actively raises the cost of the other findings.
- **Fix sketch**: Extract a shared `describe(e)` + `ActionResult` and a `runGovernanceAction(fn, { revalidate: string[] })` wrapper into a `lib/governance-actions.ts`, and a shared `useDecision`/result-note hook for the client bars. Reviews and disputes then supply only their endpoint call and success message.
