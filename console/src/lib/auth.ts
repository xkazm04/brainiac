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
 * for an HMAC-derived session cookie.
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

/** Cookie lifetime. Short enough that a leaked laptop cookie expires. */
export const SESSION_MAX_AGE = 60 * 60 * 24 * 14; // 14 days

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

async function sha256Hex(input: string): Promise<string> {
  const bytes = new TextEncoder().encode(input);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** The cookie value for a given passcode. Never the passcode itself. */
export function sessionToken(passcode: string): Promise<string> {
  return sha256Hex(DOMAIN_SEPARATOR + passcode);
}

/** Length-independent, constant-time-ish comparison. */
export function safeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/** Does this cookie value authorize access to the real-data console? */
export async function isValidSession(cookieValue: string | undefined): Promise<boolean> {
  if (!cookieValue) return false;
  const passcode = configuredPasscode();
  if (!passcode) return false;
  return safeEqual(cookieValue, await sessionToken(passcode));
}
