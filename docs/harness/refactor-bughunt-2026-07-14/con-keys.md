> Context: Console: API Keys Management
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. Failed revoke is swallowed and looks successful — key stays active
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: console/app/console/(modules)/keys/Keys.tsx:25-30 (specifically :27, and the reload swallow :23)
- **Scenario**: Operator clicks "revoke" → "sure?" on a key they need to kill (leaked, or an ex-contractor's integration). The upstream POST `/api/keys/{id}/revoke` returns 403 (bearer lacks `admin` scope), 502, or the network drops. `await revokeKey(id).catch(() => undefined)` discards the rejection; execution continues to `setConfirming(null)` and `reload()` unconditionally.
- **Root cause**: The mutation is treated as fire-and-forget — no `state`/`error` branch exists for revoke (unlike mint, which at least sets `state === "error"`). The `.catch(() => undefined)` was written to keep the UI from throwing, not to report the outcome.
- **Impact**: The confirm affordance collapses back to the resting state exactly as it does on success, so the operator believes a security-critical key was revoked when it is still live server-side. `reload()` is itself swallowed (`refreshTokens().catch(() => undefined)`), so even the corrective re-read (which would redraw the row as still-active) can silently fail, leaving a revoked-looking-but-active key. A compromised credential the admin thinks is dead keeps reading the org's memories.
- **Fix sketch**: Give revoke a per-id in-flight/error state. Await the response, and on rejection surface it (toast / inline "revoke failed — still active") and keep `confirming` set so the operator sees it did not take. Only clear `confirming` and reload on a confirmed 2xx; optionally optimistically mark the row revoked and roll back on failure.

## 2. "✓ copied" is shown even when the clipboard write never happened — one-time secret is then lost
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: console/app/console/(modules)/keys/KeyShared.tsx:219-228 (the write+flag at :221-222)
- **Scenario**: `onClick={() => { void navigator.clipboard?.writeText(minted.token); setCopied(true); }}`. In an insecure context (HTTP / some embedded webviews) `navigator.clipboard` is `undefined`, so `?.` short-circuits the write — but `setCopied(true)` still runs and the button flips to "✓ copied". Or `writeText` rejects (permissions policy / not focused); the rejection is discarded by `void`. Trusting the ✓, the operator clicks "done — I stored it", which runs `setMinted(null)`.
- **Root cause**: The success flag is set optimistically and unconditionally, decoupled from the async result of the copy. Optional chaining and `void` were used to avoid an unhandled rejection rather than to gate the UI on the actual outcome.
- **Impact**: The plaintext secret is shown exactly once and is not retrievable (`CreatedTokenResponse.token` — "never retrievable again"). A false copy-confirmation on a non-recoverable value means the operator dismisses the dialog believing the secret is on their clipboard when it is not, permanently losing it — forcing a revoke + re-mint, and worse, a paste of stale clipboard contents into an integration config.
- **Fix sketch**: `await navigator.clipboard?.writeText(...)` inside try/catch; only `setCopied(true)` on resolve. If `navigator.clipboard` is absent or the write rejects, show a copy-failed state and keep the dialog open so the visible secret can be selected manually (`select-all` already present).

## 3. Copied secret is never cleared from the OS clipboard
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: credential-hygiene
- **File**: console/app/console/(modules)/keys/KeyShared.tsx:220-221
- **Scenario**: Operator clicks "copy secret"; the full `brk_…` token is written to the shared OS clipboard. Nothing ever clears it — not on "done", not on dialog unmount, not on a timer. The secret sits in the clipboard until the user happens to copy something else.
- **Root cause**: The flow's whole point is "shown exactly once then dropped" (state is nulled on dismiss), but the clipboard — the one place the secret leaves the component — is outside that lifecycle and gets no cleanup. Auto-clear was never part of the mint dialog.
- **Impact**: A live API credential lingers in a globally-readable buffer indefinitely, exposed to any other app, any subsequently-visited web page that reads the clipboard on paste, clipboard-sync/history tools, and shoulder-paste mistakes — directly contradicting the "not stored — copy it now" promise the dialog makes at :213.
- **Fix sketch**: After a successful copy, schedule a clear (e.g. `setTimeout(() => navigator.clipboard?.writeText(""), 30_000)`, cancelled/re-armed appropriately) and/or clear on dialog dismiss; reset the "✓ copied" label when it clears so the UI reflects that the clipboard no longer holds the secret.

## 4. Mint error detail is discarded — operator sees only a generic "mint failed"
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: console/app/console/(modules)/keys/KeyShared.tsx:110-121 (catch at :117-120)
- **Scenario**: `mint()` calls `mintKey`, which deliberately extracts the server's reason: `throw new Error((await r.json())?.error ?? String(r.status))` (keys-data.ts:22) — e.g. "scopes outside read|write|admin", 401 "Missing or unknown bearer token", 403 "Token lacks the admin scope". The `catch {` block in `mint()` binds nothing and throws that message away, setting only `state === "error"` → button reads "mint failed" for 3s, then resets.
- **Root cause**: Error handling was reduced to a single boolean UI state; the informative message the API layer went out of its way to surface is dropped at the last hop.
- **Impact**: On a security-critical operation the operator cannot distinguish a fixable validation error (bad scope) from an auth/permission problem from a transient upstream outage. They retry blindly (re-triggering create attempts), or give up, with no actionable signal — poor diagnosability and a nudge toward duplicate-create attempts.
- **Fix sketch**: `catch (e) { setError(e instanceof Error ? e.message : "mint failed"); setState("error"); }` and render the message near the button. Keep the 3s auto-reset for the transient styling only.

## 5. `BlastRings` is a dead export and KeyShared's "shared-across-variants" premise is stale
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/app/console/(modules)/keys/KeyShared.tsx:28 (and header :3-8)
- **Scenario**: The file header documents KeyShared as hoisted "per the /prototype skill" so multiple key variants (the comments in Keys.tsx name "Keyring", "Blast Radius", "Ground Control") could share the mint flow and rings. Only one variant survives: `Keys.tsx` imports just `{ fmtAgo, MintPanel }`. `BlastRings` is `export`ed but its only reference is internal — `MintPanel` at :173. No other module imports it (verified across the app, excluding build artifacts).
- **Root cause**: Leftover surface from the multi-variant prototype consolidation noted in the Keys.tsx header ("consolidated from the 2026-07-13 prototype round"); the export boundary was never tightened after the losing variants were deleted.
- **Impact**: Misleading public API — `BlastRings` reads as a reusable shared component when it is a private helper of `MintPanel`; the "variants frame these differently" comment describes consumers that no longer exist, costing reader time and inviting accidental reuse of an internal. (Note: the `// see fmtDate` comment at :18 points at a genuinely separate absolute-date helper in memories/MemoryInspector.tsx — different formatting, so not true duplication and not flagged as such.)
- **Fix sketch**: Drop the `export` on `BlastRings` (keep it module-private), and trim the header comment to describe the single surviving surface rather than the retired variant set.
