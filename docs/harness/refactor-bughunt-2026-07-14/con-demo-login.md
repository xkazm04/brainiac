> Context: Console: Demo Mode + Login
> Total: 5 (Critical: 0, High: 3, Medium: 1, Low: 1)

## 1. Session cookie is a keyless, unsalted SHA-256 of the passcode — offline-forgeable, unrevocable, no real expiry
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: weak-session-token
- **File**: console/app/login/actions.ts:34 (value produced by `sessionToken` in console/src/lib/auth.ts:62-64)
- **Scenario**: On success the cookie is set to `await sessionToken(passcode)`, which is `sha256Hex("brainiac-console:v1:" + passcode)` — a plain digest with **no server-side secret / no HMAC key**, no per-session nonce, and no embedded issued-at/expiry. The domain-separator prefix is a hard-coded public constant. An attacker who obtains the source (public repo) can brute-force the passcode → cookie offline at GPU speed, then set `bx_console` directly and skip `/login` entirely (rate-limiting never applies). The cookie value is also **identical for every user and every session**, so a single leaked cookie authenticates forever.
- **Root cause**: The comment at auth.ts:15 claims an "HMAC-derived session cookie," but `sessionToken` uses a keyless `crypto.subtle.digest("SHA-256", …)`. Security rests entirely on the passcode's entropy plus an advisory "use a long random string" hint on the login page.
- **Impact**: No revocation — `logout()` only clears the local cookie; the same value keeps working until the shared passcode is rotated for everyone. `SESSION_MAX_AGE` (14d) is only a browser cookie attribute; a captured/copied value is valid indefinitely server-side. Weak passcodes are offline-crackable with zero online friction.
- **Fix sketch**: Introduce a distinct `CONSOLE_SESSION_SECRET` and make the token an HMAC over `{issuedAt, random-nonce}` (`crypto.subtle.sign("HMAC", …)`); store `payload.signature` in the cookie and verify signature **and** `issuedAt + maxAge > now` in `isValidSession`. This adds a keyed secret (defeats offline precomputation), real server-side expiry, and a rotation lever independent of the passcode.

## 2. Open redirect: `safeNext` blocks `//` but not backslash / normalization bypasses
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: open-redirect
- **File**: console/app/login/actions.ts:15-18, 42
- **Scenario**: `safeNext` allows any value where `next.startsWith("/") && !next.startsWith("//")`, then `login()` calls `redirect(next)`. A crafted `/login?next=/\evil.com` (or `/%2F%2Fevil.com`, `/\/\evil.com`) passes both checks — it starts with `/` and not with `//`. The login page renders it verbatim into `<input hidden name="next">` (page.tsx:63), and on successful auth the server emits `Location: /\evil.com`. Per WHATWG URL parsing for HTTP(S), the browser normalizes `\` to `/`, yielding `//evil.com` → `http://evil.com`.
- **Root cause**: The allow-check is a string-prefix heuristic that only anticipates the literal `//` protocol-relative form; it never parses the target or rejects backslashes/encoded slashes/control chars.
- **Impact**: A freshly-authenticated operator is bounced to an attacker-controlled origin — credible phishing/token-relay landing page immediately after they proved they hold the console passcode. Exploitable regardless of passcode strength.
- **Fix sketch**: Reject any `next` containing `\`, control chars, or `%2f/%5c` (decode first); or resolve `new URL(next, "https://placeholder.local")` and confirm `url.origin === "https://placeholder.local"` and `url.pathname === next`. Better: restrict to a known route allow-list (reuse `isPublicSurface`/module ids) and fall back to `/console`.

## 3. No rate-limiting or lockout on passcode attempts
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: brute-force
- **File**: console/app/login/actions.ts:20-31
- **Scenario**: `login()` reads the passcode, compares once, and on failure `redirect("/login?err=bad&…")`. There is no per-IP counter, no exponential backoff, no lockout, and no artificial delay. A script can POST the server action thousands of times per minute against the single shared secret.
- **Root cause**: The gate was written as a stateless one-secret check ("v0" per auth.ts) with no attempt-tracking store; throttling was deferred.
- **Impact**: The console's only real-data gate can be online-brute-forced. Because the secret is shared (one per deployment, not per user), a single guessed value unlocks the entire live org knowledge base for everyone.
- **Fix sketch**: Add a per-IP (and global) sliding-window limiter keyed in the existing SQLite/store before the `safeEqual` check — e.g. block after N failures/window with increasing delay — and add a small constant floor delay on every attempt to blunt both brute force and timing.

## 4. `DEMO_MODULE_IDS` is exported but unused; the demo module list is duplicated and drift-prone in next.config.ts
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: dead-code / duplication
- **File**: console/app/demo/DemoConsole.tsx:108
- **Scenario**: `export const DEMO_MODULE_IDS = MODULES.map((m) => m.id)` has no importer anywhere in the tree (grep-confirmed). It was evidently created to be the single source of truth for demo module slugs, but the `/demo/<module>` redirect rules in next.config.ts:25 hardcode a separate literal `DEMO_MODULES = ["reviews","disputes","graph","memories","health","divergence"]` instead. The two lists must be kept in sync by hand — add a module tab to `MODULES` and its old-path redirect silently won't exist.
- **Root cause**: Leftover from the seven-routes → one-page tab refactor: the intended shared constant was exported but never wired into the config that still owns the redirect map (and a `next.config.ts` cannot easily import a client component, so it forked the list).
- **Impact**: Dead export plus a real drift hazard between the tab bar and the compatibility redirects; misleads a reader into thinking there is one source of truth when there are two.
- **Fix sketch**: Either delete `DEMO_MODULE_IDS`, or hoist the id list into a tiny config-importable module (e.g. `demo/modules.ts`) and have both `MODULES` and next.config.ts's redirect map derive from it, dropping the duplicated literal.

## 5. `DEMO_COUNTS` fixture is exported but never consumed
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: dead-code
- **File**: console/app/demo/demo-reviews-data.ts:141-146
- **Scenario**: `export const DEMO_COUNTS = [{status:"open",count:1}, …]` is imported by nothing (grep-confirmed). page.tsx pulls only `DEMO_PROMOTIONS` and `DEMO_CONTRADICTIONS`, and `ReviewGate` derives its own counts from `promotions.length` / `contradictions`. It is orphaned scaffolding from an earlier per-route reviews summary that no longer exists.
- **Root cause**: When the tour collapsed into one page and `ReviewGate` was lifted verbatim, the old counts strip that fed off `DEMO_COUNTS` was dropped but its fixture wasn't.
- **Impact**: Minor cruft; a stale hand-maintained array (its numbers can silently diverge from the actual `DEMO_CONTRADICTIONS`) that a reader may mistake for live wiring.
- **Fix sketch**: Delete the `DEMO_COUNTS` export.
