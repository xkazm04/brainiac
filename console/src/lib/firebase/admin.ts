import "server-only";

import { getApps, initializeApp, type App } from "firebase-admin/app";
import { getAuth } from "firebase-admin/auth";

/*
 * Server-side verification of a Google sign-in — the ONLY thing that makes the
 * client's claim about who they are worth anything.
 *
 * NO SERVICE-ACCOUNT KEY, deliberately. `verifyIdToken` checks the JWT's
 * signature against Google's PUBLIC certificates and that its `aud` matches the
 * project; none of that needs a credential. So the Admin SDK is initialized with
 * a projectId alone, and this deployment ships no `firebase-admin.json` to leak,
 * rotate, or accidentally commit. (The sibling project that inspired this flow
 * does check one in — that key is on its revoke list, and copying the pattern
 * would have imported the problem.)
 *
 * What verification buys: the browser hands us a token it got from Google. We
 * confirm Google signed it, it was issued for THIS project, it hasn't expired,
 * and it names a uid + email. Only then does the uid reach `/v1/provision`, which
 * has no way of its own to tell a real uid from a made-up string.
 */

export interface VerifiedIdentity {
  /** Firebase uid — stable for the life of the account. The identity. */
  uid: string;
  email: string;
  emailVerified: boolean;
  name?: string;
}

function adminApp(): App | null {
  const existing = getApps()[0];
  if (existing) return existing;
  const projectId =
    process.env.FIREBASE_PROJECT_ID?.trim() ||
    process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID?.trim();
  if (!projectId) return null;
  return initializeApp({ projectId });
}

/**
 * Verify a Firebase ID token. Returns null when the token is bad, expired, for
 * another project, or when Firebase isn't configured — every failure is the same
 * "not signed in" to the caller, because distinguishing them only helps an
 * attacker enumerate.
 */
export async function verifyIdToken(idToken: string): Promise<VerifiedIdentity | null> {
  const app = adminApp();
  if (!app || !idToken) return null;
  try {
    const decoded = await getAuth(app).verifyIdToken(idToken);
    if (!decoded.uid || !decoded.email) return null;
    return {
      uid: decoded.uid,
      email: decoded.email,
      emailVerified: decoded.email_verified === true,
      name: typeof decoded.name === "string" ? decoded.name : undefined,
    };
  } catch {
    return null;
  }
}
