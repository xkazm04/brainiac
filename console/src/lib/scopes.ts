/**
 * The token scope vocabulary — the ONE console-side mirror of the server's
 * `brainiac_server::auth::SCOPES`.
 *
 * It lives here rather than beside the mint UI because two surfaces need it and
 * they must never disagree: the chip row a human toggles, and the server-side
 * validation on `POST /api/keys` that decides what a request is allowed to ask
 * for. A mint form that offers a scope the guard rejects is a dead button; a
 * guard that accepts a scope the form never shows is an authorization hole the
 * UI cannot show you. One list, imported by both.
 *
 * When `auth::SCOPES` changes, change this. The backend re-validates
 * independently (400 on anything outside its own list), so a drift here fails
 * closed — it can only ever be narrower than the truth, never wider.
 */
export const SCOPES = [
  // memory layer
  "read",
  "write",
  // knowledge base (KB-PLAN D6)
  "kb:read",
  "kb:publish",
  // library (LIBRARY-PLAN L7)
  "lib:read",
  "lib:propose",
  "lib:publish",
  // implies all of the above
  "admin",
] as const;

export type Scope = (typeof SCOPES)[number];

export function isScope(value: unknown): value is Scope {
  return typeof value === "string" && (SCOPES as readonly string[]).includes(value);
}

/** Canonical UUID shape — the only thing a `user_id` may ever look like. */
export const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_RE.test(value);
}
