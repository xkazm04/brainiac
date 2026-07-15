# Fix Wave 8 — Console Session & Route Auth

> 1 commit, 4 findings resolved (all High) — the console's only real-data gate.
> Gates green: console `tsc` 0 · vitest **58/58** (was 50; +8 new auth tests).
> Console-only, so the Rust workspace is untouched.

## ⚠ Deployment notes (read before shipping)

1. **Every existing console session is invalidated.** The cookie format changed
   from a bare digest to `issuedAt.nonce.fingerprint.hmac`; old cookies no longer
   parse. Operators re-login once. This is a one-time cost and also the point —
   the old values were unrevocable.
2. **Set `CONSOLE_SESSION_SECRET` in production.** Optional (it falls back to
   keying on the passcode), but with it set, a guessed passcode can no longer be
   turned into a valid cookie, so the new login throttle cannot be sidestepped.
3. **The login throttle is in-memory** — per server instance, lost on restart. Sized
   for the single-operator console this is; replicas or a serverless deploy need a
   shared store.

## What was fixed

1. **The session cookie was keyless, unsalted, and identical for everyone (C15#1).**
   `sessionToken` was `sha256("brainiac-console:v1:" + passcode)` — while the
   module doc claimed an "HMAC-derived session cookie". Consequences:
   - **Same value for every user and every session** ⇒ one leaked cookie
     authenticated forever, and `logout()` only cleared it locally.
   - **No server-side expiry** — `SESSION_MAX_AGE` was only a browser cookie
     attribute, so a copied value stayed valid indefinitely.
   - **Derivable from the source + a passcode guess** ⇒ an attacker could set
     `bx_console` directly and never touch `/login`, sidestepping any throttle.

   Now `issuedAt.nonce.passcodeFingerprint.hmac`: a per-session nonce (distinct
   values), a lifetime checked server-side, and a fingerprint that makes a passcode
   rotation invalidate outstanding sessions. `CONSOLE_SESSION_SECRET` keys the
   signature independently of the passcode. The doc's claim is now true.

2. **Open redirect (C15#2).** `safeNext` allowed anything starting with `/` but not
   `//`, so `/login?next=/\evil.com` passed both checks — and per WHATWG URL
   parsing the browser normalizes `\` → `/`, making the emitted Location
   `//evil.com` → `http://evil.com`. The victim is an operator who has *just*
   proved they hold the console passcode, which is exactly who a phishing landing
   page wants. Now rejects backslashes, encoded slashes and control chars, and
   resolves the target to confirm it stays same-origin.

3. **No brute-force throttle (C15#3).** `login()` compared the single shared secret
   with no counter, no backoff, no lockout — a script could POST the action
   thousands of times a minute, and one hit unlocks the entire live org knowledge
   base for everyone. Added a per-IP sliding window (8 attempts / 15 min) plus a
   constant floor delay on every attempt (blunting brute force and timing signal),
   with a "throttled" message on the login page.

4. **`/api` was not always gated (C04#1).** The middleware matcher's
   `.svg|.png|…|.txt$` exclusion applies to *every* path, so `GET
   /api/memories/<id>.txt` skipped the middleware entirely and reached the
   privileged-token proxy with no session check — leaving the route's own hex-id
   regex as the only thing between an anonymous caller and live org data (a regex
   that was never meant to be load-bearing for auth). Matchers are OR'd, so
   `"/api/:path*"` is now listed unconditionally. Real static assets are served
   from `/public` and never live under `/api`.

## Tests

`console/src/lib/auth.test.ts` is new — the module had **no test at all** despite
being the access gate. 8 cases: round-trip, per-session distinctness, tampered
issued-at rejection, server-side expiry (fake timers), rotation invalidation,
malformed + legacy bare-digest cookies, unconfigured lockout, and signing-secret
rotation.

## Patterns established (catalogue items 19–20)

19. **A doc comment claiming a security property is a bug report until tested.**
    Both this wave's cookie ("HMAC-derived" — it was a keyless digest) and Wave 7's
    redaction ("bearer … secrets" — no bearer rule existed) shipped the *claim*
    without the mechanism. Where a comment asserts a guarantee, assert it in a test.
20. **A path-shaped allow/deny rule must never be able to swallow an auth boundary.**
    The asset-extension exclusion was written for `/public`-style assets but applied
    to every route, silently un-gating `/api/*.txt`. Gate security-relevant prefixes
    explicitly and first, rather than trusting a negative lookahead.

## What remains

~44 findings fixed across Waves 1–8 + R01#1. Wave 9 (correctness edge cases:
`as_of` degradation, DISTINCT-ON picking by UUID, SectionBinding default, divergence
crash, reduced-motion hero, RAF leak, silent 40-row truncation) + the ~48 M/L
refactor tail (dead `NavStatus.tsx`, drifted `MODULE_BAND`, duplicated
`vector_literal`/`cosine`/TRUNCATE lists, god-components).
