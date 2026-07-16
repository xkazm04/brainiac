/*
 * Console access gate — v0.
 *
 * The public surface (the pitch at "/" and the fixture-org demo at "/demo")
 * renders example data only and never calls the API. Everything else reads or
 * writes the REAL org knowledge base through a privileged server-side bearer
 * token, so it must not be reachable by an anonymous visitor. Until then the
 * console shipped that data to anyone who could reach the port.
 *
 * WHAT THIS IS: a shared-passcode gate. One secret per deployment, exchanged
 * for an HMAC-signed session cookie carrying a nonce + issued-at, verified
 * server-side on every request. (This doc previously claimed "HMAC-derived"
 * while the implementation used a keyless digest — see `sessionToken`.)
 *
 * CONFIG: `CONSOLE_PASSCODE` is the gate. `CONSOLE_SESSION_SECRET` is optional
 * but recommended in production — it keys the cookie signature independently of
 * the passcode, so a guessed passcode cannot be turned straight into a valid
 * cookie (which would sidestep the login rate limiter).
 *
 * WHAT THIS IS NOT: per-user identity. It authenticates "someone who knows the
 * console passcode", not "Petra". The architecture calls for OIDC/SAML + SCIM
 * (docs/ARCHITECTURE.md §2.1), and the API's own per-principal tokens already
 * carry RLS — this gate sits in front of that, it does not replace it. Do not
 * mistake it for an identity system, and do not build per-user features on it.
 *
 * No secrets reach the browser: the cookie holds a derived digest, never the
 * passcode itself, and it is httpOnly.
 */

export const SESSION_COOKIE = "bx_console";

/**
 * Cookie lifetime — how long "remembered" lasts.
 *
 * Asked for on 2026-07-15 as "store the passcode in localStorage so it is
 * remembered until storage is erased". This is that feature, built the other way
 * round, and the difference is worth stating because it is the whole security
 * posture of this file: the passcode is ONE shared secret that unlocks the live
 * org for everybody, and localStorage is readable by any script on the origin.
 * Putting it there would mean a single XSS — in any dependency, on any console
 * page — exfiltrates the org's key, permanently and for every operator, and no
 * rotation short of changing the passcode helps.
 *
 * The cookie already does what was actually wanted. It is httpOnly (script
 * cannot read it), it carries a signed digest rather than the passcode itself,
 * and it dies when the operator clears site data — the same "until storage is
 * erased" behaviour, minus the secret. So the fix was a number, not a mechanism.
 *
 * 400 days is Chrome's ceiling on cookie lifetime; anything larger is silently
 * clamped, so asking for more would only be a comment that lies. The trade this
 * accepts, deliberately: a stolen laptop stays signed in far longer than the old
 * 14 days. `logout` (the sign-out control) still clears it on demand, and
 * rotating CONSOLE_PASSCODE invalidates every issued cookie at once, because the
 * signature is derived from it.
 */
export const SESSION_MAX_AGE = 60 * 60 * 24 * 400; // 400 days — the browser cap

const DOMAIN_SEPARATOR = "brainiac-console:v1:";

/** The configured passcode, or undefined when the operator hasn't set one. */
export function configuredPasscode(): string | undefined {
  const p = process.env.CONSOLE_PASSCODE?.trim();
  return p ? p : undefined;
}

/**
 * With no passcode configured, the console is open in development (so the UI
 * can be worked on without ceremony) and LOCKED in production.
 *
 * Fail-closed is the only defensible default here: an unconfigured production
 * deployment must not silently serve the org's knowledge base to the internet.
 * The login page explains what to set.
 */
export function isUnlockedByDefault(): boolean {
  return configuredPasscode() === undefined && process.env.NODE_ENV !== "production";
}

/** True when a passcode exists but the deployment is production-locked. */
export function isMisconfigured(): boolean {
  return configuredPasscode() === undefined && process.env.NODE_ENV === "production";
}

function toHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function sha256Hex(input: string): Promise<string> {
  const bytes = new TextEncoder().encode(input);
  return toHex(await crypto.subtle.digest("SHA-256", bytes));
}

/**
 * The key the session cookie is signed with.
 *
 * `CONSOLE_SESSION_SECRET` is the one that matters: with it, a cookie cannot be
 * derived from the passcode at all, so guessing the passcode offline buys nothing
 * — an attacker must go through /login, where the rate limiter applies. Without
 * it we fall back to keying on the passcode: still a real improvement (nonce +
 * expiry + per-session values), but a guessed passcode could be turned into a
 * cookie directly, bypassing the login throttle. Set the secret in production.
 */
function signingKey(passcode: string): string {
  const secret = process.env.CONSOLE_SESSION_SECRET?.trim();
  return secret ? secret : passcode;
}

async function hmacHex(key: string, message: string): Promise<string> {
  const enc = new TextEncoder();
  const k = await crypto.subtle.importKey(
    "raw",
    enc.encode(key),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  return toHex(await crypto.subtle.sign("HMAC", k, enc.encode(message)));
}

/** Short fingerprint of the passcode, embedded so rotating it invalidates every
 * outstanding session (the cookie is no longer a function of the passcode, so
 * rotation would otherwise NOT log anyone out). */
async function passcodeFingerprint(passcode: string): Promise<string> {
  return (await sha256Hex(DOMAIN_SEPARATOR + passcode)).slice(0, 16);
}

/**
 * Mint a session cookie: `issuedAt.nonce.passcodeFingerprint.hmac`.
 *
 * Replaces a keyless, unsalted `sha256(DOMAIN_SEPARATOR + passcode)` that was
 * (a) identical for every user and every session — one leaked cookie
 * authenticated forever, (b) derivable by anyone holding the (public) source and
 * a passcode guess, so /login's throttle could be skipped entirely, and (c)
 * without any server-side expiry: SESSION_MAX_AGE was only a browser attribute,
 * so a copied value stayed valid indefinitely. The nonce makes each session
 * distinct, issuedAt gives a real server-checked lifetime, and the HMAC key adds
 * a rotation lever independent of the passcode.
 */
export async function sessionToken(passcode: string): Promise<string> {
  const issuedAt = Math.floor(Date.now() / 1000);
  const nonce = crypto.randomUUID().replace(/-/g, "");
  const fp = await passcodeFingerprint(passcode);
  const payload = `${issuedAt}.${nonce}.${fp}`;
  return `${payload}.${await hmacHex(signingKey(passcode), payload)}`;
}

/** Length-independent, constant-time-ish comparison. */
export function safeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/**
 * Does this cookie value authorize access to the real-data console?
 *
 * Verifies the signature, that the session has not aged out (server-side — not
 * merely the browser's cookie attribute), and that it was issued against the
 * CURRENT passcode, so a rotation logs everyone out.
 */
export async function isValidSession(cookieValue: string | undefined): Promise<boolean> {
  if (!cookieValue) return false;
  const passcode = configuredPasscode();
  if (!passcode) return false;

  const parts = cookieValue.split(".");
  if (parts.length !== 4) return false;
  const [iatRaw, nonce, fp, sig] = parts;
  if (!iatRaw || !nonce || !fp || !sig) return false;

  const payload = `${iatRaw}.${nonce}.${fp}`;
  const expected = await hmacHex(signingKey(passcode), payload);
  if (!safeEqual(sig, expected)) return false;

  const issuedAt = Number(iatRaw);
  if (!Number.isFinite(issuedAt)) return false;
  const age = Math.floor(Date.now() / 1000) - issuedAt;
  // Reject the future too: a clock-skewed or hand-crafted iat must not buy extra
  // lifetime. (Signature-checked, so this only matters for our own bad clocks.)
  if (age < -60 || age > SESSION_MAX_AGE) return false;

  return safeEqual(fp, await passcodeFingerprint(passcode));
}
