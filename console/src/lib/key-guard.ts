import "server-only";

import { getOrgUsers, type ApiConfig } from "./api";
import { isScope, isUuid, SCOPES, type Scope } from "./scopes";

/*
 * Authorization for the key-minting proxy.
 *
 * WHY THIS EXISTS. `/api/keys` is a proxy: the browser posts a body, the route
 * forwards it to `POST /v1/tokens` using the deployment's PRIVILEGED bearer
 * token. Everything the backend authorizes, it authorizes as the operator — so
 * every field the browser is allowed to choose is a field the browser gets to
 * decide with the operator's authority. Authentication (`middleware.ts`) proves
 * only that the caller holds the console passcode; it says nothing about what
 * that caller may ask for. Without this module the route forwarded
 * caller-supplied `user_id` and `scopes` verbatim, so a request could mint a key
 * that ACTS AS ANY UUID, including identities that belong to nobody and can
 * therefore never be recognised, revoked-by-owner, or audited to a person.
 *
 * WHAT IT ENFORCES, and why these two and not more:
 *
 *   1. `scopes` ⊆ the console's own vocabulary (src/lib/scopes.ts). The backend
 *      re-validates against `auth::SCOPES`, so this is defence in depth, not the
 *      only gate — but it means the console answers 400 for a scope it never
 *      offered instead of relaying a mystery upstream error, and it keeps the
 *      chip row and the guard provably in step.
 *
 *   2. `user_id` names a REAL member of this org, checked against
 *      `GET /v1/org/users` at mint time. This is the load-bearing one. The
 *      backend's `create_token` takes `body.user_id` on trust
 *      (`unwrap_or(caller)`) with no membership check of its own, so the console
 *      is the last place this can be caught. The mint UI already only offers
 *      users from this list; the guard makes that a rule rather than a habit.
 *
 * WHAT IT DELIBERATELY DOES NOT DO. It does not introduce per-user identity.
 * The console gate is one shared passcode (see lib/auth.ts) and this module must
 * not be mistaken for the OIDC/SCIM layer the architecture calls for — a caller
 * who holds the passcode IS the operator and can still mint an `admin` key,
 * exactly as the UI offers. What changes is that the operator can no longer be
 * tricked (or trick themselves) into minting authority for a principal that does
 * not exist.
 */

export interface GuardFailure {
  status: 400 | 403 | 502;
  error: string;
}

export interface MintRequest {
  name: string;
  /** Undefined = the backend defaults to the calling principal. */
  userId?: string;
  scopes: Scope[];
}

/** Longest key name we relay. Names are display-only labels in the key list. */
const MAX_NAME = 120;

function fail(status: GuardFailure["status"], error: string): GuardFailure {
  return { status, error };
}

export function isGuardFailure(v: unknown): v is GuardFailure {
  return typeof v === "object" && v !== null && "status" in v && "error" in v;
}

/**
 * Is `userId` a member of the org the privileged token belongs to?
 *
 * Fails CLOSED: if the org roster cannot be read, no identity can be asserted,
 * so nothing is minted. An upstream outage must not degrade into "skip the
 * check" — that is the shape of every authorization bypass worth having.
 */
export async function assertOrgMember(
  cfg: ApiConfig,
  userId: string,
): Promise<GuardFailure | null> {
  let users: Awaited<ReturnType<typeof getOrgUsers>>;
  try {
    users = await getOrgUsers(cfg);
  } catch {
    return fail(502, "could not verify org membership");
  }
  if (!users.some((u) => u.id === userId)) {
    return fail(403, "user_id must name a member of this org");
  }
  return null;
}

/**
 * Validate a mint request body. Shape/vocabulary only — the membership round
 * trip is `assertOrgMember`, kept separate so a malformed body is rejected
 * without touching the backend at all.
 */
export function parseMintRequest(body: unknown): MintRequest | GuardFailure {
  if (typeof body !== "object" || body === null) return fail(400, "bad body");
  const b = body as Record<string, unknown>;

  if (typeof b.name !== "string" || !b.name.trim()) return fail(400, "name required");
  const name = b.name.trim();
  if (name.length > MAX_NAME) return fail(400, `name must be ${MAX_NAME} chars or fewer`);

  if (b.user_id !== undefined && b.user_id !== null && !isUuid(b.user_id)) {
    return fail(400, "user_id must be a uuid");
  }
  const userId = isUuid(b.user_id) ? b.user_id : undefined;

  // An absent `scopes` is not a blank cheque: the backend's own default is
  // ["read"], and saying so here keeps the minted authority visible in the
  // request that asked for it rather than implied by a remote default.
  const rawScopes = b.scopes === undefined || b.scopes === null ? ["read"] : b.scopes;
  if (!Array.isArray(rawScopes) || rawScopes.length === 0) {
    return fail(400, "scopes must be a non-empty array");
  }
  const bad = rawScopes.filter((s) => !isScope(s));
  if (bad.length > 0) {
    return fail(400, `scopes must be a subset of ${JSON.stringify(SCOPES)}`);
  }
  // De-duplicate: ["admin","admin"] is one grant, and the key list should say so.
  const scopes = Array.from(new Set(rawScopes as Scope[]));

  return { name, userId, scopes };
}
