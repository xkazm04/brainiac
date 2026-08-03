"use server";

// The last step of the skill loop, from the console instead of raw SQL.
// Same trust model as the standards actions (single-operator console, one
// shared service token): a 403 means the TOKEN lacks lib:publish, not the
// human at the keyboard. The backend is the authority on what "publish"
// means — which draft gets signed, and whether a second click is a no-op.

import { revalidatePath } from "next/cache";

import { ApiError, configFromEnv, publishSkill } from "@/lib/api";

export interface SkillActionResult {
  ok: boolean;
  message: string;
}

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 403)
      return "The console's service token does not carry lib:publish (check BRAINIAC_API_TOKEN).";
    if (e.status === 404)
      return "Nothing left to sign — this skill has no draft version. The shelf has been refreshed.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

/** Publish a skill's pending draft. Idempotent at the backend, so a double
 *  click reports the version already serving rather than failing. */
export async function publishSkillAction(slug: string): Promise<SkillActionResult> {
  try {
    const out = await publishSkill(configFromEnv(), slug);
    revalidatePath("/console");
    return {
      ok: true,
      message: out.newly_published
        ? `Published v${out.semver} — signed by name; agents can pull it now.`
        : `Already serving v${out.semver} — nothing new was signed.`,
    };
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) revalidatePath("/console");
    return { ok: false, message: describe(e) };
  }
}
