"use server";

import { ApiError, configFromEnv } from "@/lib/api";
import { verifyIdToken } from "@/lib/firebase/admin";

/*
 * Google sign-in → your one project → a device key.
 *
 * TRUST CHAIN, end to end, because this is the whole security of the free tier:
 *   browser  gets an ID token from Google (a CLAIM — worth nothing on its own)
 *   here     verifies it against Google's public certs + this project's audience
 *   backend  /v1/provision records the VERIFIED uid and mints the key
 *
 * The verification step is load-bearing: `/v1/provision` cannot tell a real uid
 * from a made-up string, so if the token were passed through unverified, anyone
 * who could reach this action could provision a project onto any uid they liked.
 *
 * This is a SEPARATE way in from the operator passcode. It does not touch the
 * `bx_console` session, and signing in with Google grants no access to the
 * operator console — it gets you a project and a key, nothing more.
 */

export interface SignupResult {
  ok: boolean;
  message: string;
  /** Present exactly once, on the response that mints it. Never retrievable. */
  apiKey?: string;
  projectId?: string;
  /** False when this account already had its project — not an error. */
  created?: boolean;
}

/** Shape of POST /v1/provision. Kept local: this is the only caller. */
interface ProvisionResponse {
  org_id: string;
  user_id: string;
  team_id: string;
  created: boolean;
  api_key: string | null;
  api_key_prefix: string | null;
}

export async function claimProjectWithGoogle(
  idToken: string,
  issueKey = false,
): Promise<SignupResult> {
  const identity = await verifyIdToken(idToken);
  if (!identity) {
    return { ok: false, message: "That sign-in could not be verified. Try again." };
  }
  // Google verifies its own accounts' addresses, so an unverified email here is
  // an oddity worth refusing rather than provisioning around: the email is what a
  // human will later use to recognise their project.
  if (!identity.emailVerified) {
    return { ok: false, message: "Your Google account's email is not verified." };
  }

  const cfg = configFromEnv();
  if (!cfg.token) {
    return {
      ok: false,
      message: "This deployment has no BRAINIAC_API_TOKEN configured, so it cannot create projects.",
    };
  }

  try {
    const res = await fetch(`${cfg.baseUrl}/v1/provision`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${cfg.token}`,
        "content-type": "application/json",
      },
      body: JSON.stringify({
        provider: "google",
        subject: identity.uid,
        email: identity.email,
        project_name: identity.name || identity.email.split("@")[0],
        issue_key: issueKey,
      }),
      cache: "no-store",
      signal: AbortSignal.timeout(15_000),
    });
    if (!res.ok) {
      const text = await res.text().catch(() => "");
      return { ok: false, message: `Could not create your project (${res.status}). ${text}`.trim() };
    }
    const out = (await res.json()) as ProvisionResponse;
    return {
      ok: true,
      created: out.created,
      projectId: out.org_id,
      apiKey: out.api_key ?? undefined,
      message: out.created
        ? "Your project is ready."
        : "You already have a project — one per account for now.",
    };
  } catch (e) {
    if (e instanceof ApiError) return { ok: false, message: e.message };
    return {
      ok: false,
      message: e instanceof Error ? e.message : "The service did not respond.",
    };
  }
}
