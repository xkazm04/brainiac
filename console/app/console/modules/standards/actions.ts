"use server";

// Same trust model as the reviews actions (single-operator console, one
// shared service token): a 403 means the TOKEN lacks lib:publish, not the
// human at the keyboard. The backend is the authority on the attribution
// rule — a 409 here is the database refusing an evidence-free adoption, and
// the message tells the maintainer exactly what signing a decree means.

import { revalidatePath } from "next/cache";

import {
  adoptStandard,
  ApiError,
  configFromEnv,
  deprecateStandard,
  rejectStandard,
  updateStandard,
} from "@/lib/api";
import type { StandardUpdate } from "@/lib/types";

export interface ActionResult {
  ok: boolean;
  message: string;
  /** The backend refused a plain adoption for lack of evidence; the UI may
   *  re-offer the action as an explicit signed decree. */
  needsDecree?: boolean;
}

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 403)
      return "The console's service token does not carry lib:publish (check BRAINIAC_API_TOKEN).";
    if (e.status === 404) return "Already decided in another session — the board has been refreshed.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

function refresh() {
  revalidatePath("/console");
}

export async function adoptStandardAction(id: string, decree: boolean): Promise<ActionResult> {
  try {
    await adoptStandard(configFromEnv(), id, decree);
    refresh();
    return {
      ok: true,
      message: decree ? "Adopted by decree — signed with the token's name." : "Adopted.",
    };
  } catch (e) {
    if (e instanceof ApiError && e.status === 409) {
      return {
        ok: false,
        needsDecree: true,
        message:
          "This rule carries no evidence. Adopting it means signing for it by name — a decree, visible on the rule forever.",
      };
    }
    if (e instanceof ApiError && e.status === 404) refresh();
    return { ok: false, message: describe(e) };
  }
}

/**
 * Narrow, re-file, or re-word a rule before (or after) the gate. The message
 * is deliberately specific about which of the two happened, because they are
 * different acts: re-filing moves a shelf, re-wording changes what the org
 * asks people to do and mints a revision that stays on the rule forever.
 */
export async function updateStandardAction(
  id: string,
  patch: StandardUpdate,
): Promise<ActionResult> {
  try {
    const out = await updateStandard(configFromEnv(), id, patch);
    refresh();
    return {
      ok: true,
      message:
        out.rev != null
          ? `Re-worded — revision ${out.rev}. The old wording stays in the history, and the ${out.stack} standards page will recompose.`
          : `Filed under ${out.stack} / ${out.category}.`,
    };
  } catch (e) {
    if (e instanceof ApiError && (e.status === 404 || e.status === 409)) refresh();
    return { ok: false, message: describe(e) };
  }
}

export async function deprecateStandardAction(id: string): Promise<ActionResult> {
  try {
    await deprecateStandard(configFromEnv(), id);
    refresh();
    return { ok: true, message: "Retired — in the open, not by neglect." };
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) refresh();
    return { ok: false, message: describe(e) };
  }
}

export async function rejectStandardAction(id: string): Promise<ActionResult> {
  try {
    await rejectStandard(configFromEnv(), id);
    refresh();
    return {
      ok: true,
      message: "Rejected — and remembered: the mining sweep will not re-propose this signal.",
    };
  } catch (e) {
    if (e instanceof ApiError && e.status === 404) refresh();
    return { ok: false, message: describe(e) };
  }
}
