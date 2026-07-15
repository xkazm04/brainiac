"use server";

// TRUST MODEL (single-operator console): every operator authenticates with one
// shared passcode and the console calls the backend with one shared service token
// (BRAINIAC_API_TOKEN). The console has no per-user identity, so the backend's
// per-team maintainer authorization is enforced against that token, not the human
// at the keyboard. A 403 here therefore means the TOKEN lacks the role, and the
// audit trail names the token — not the person. This is a deliberate deployment
// posture (one trusted maintainer per console); see
// docs/harness/refactor-bughunt-2026-07-14/INDEX.md Theme A.

import { revalidatePath } from "next/cache";

import {
  ApiError,
  configFromEnv,
  resolveContradiction,
  reviewPromotion,
} from "@/lib/api";
import type { ContradictionResolution } from "@/lib/types";

export interface ActionResult {
  ok: boolean;
  message: string;
}

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    // Shared service token → a 403 is about the token's roles, not "you".
    if (e.status === 403)
      return "The console's service token is not a maintainer of the owning team (check BRAINIAC_API_TOKEN).";
    // 404 (no longer pending) or 409 (lost the atomic approve/reject race) both
    // mean another session already decided this item — see the refresh below.
    if (e.status === 404 || e.status === 409)
      return "Already decided in another session — the queue has been refreshed.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

/** Refresh the queues so a phantom (already-decided) row clears for this client too. */
function refreshQueues() {
  revalidatePath("/console/reviews");
  revalidatePath("/console/analytics");
}

/** True when the failure means the item was already decided elsewhere. */
function alreadyDecided(e: unknown): boolean {
  return e instanceof ApiError && (e.status === 404 || e.status === 409);
}

export async function reviewPromotionAction(
  id: string,
  action: "approve" | "reject",
): Promise<ActionResult> {
  try {
    const out = await reviewPromotion(configFromEnv(), id, action);
    refreshQueues();
    return { ok: true, message: `Memory is now ${out.memory_status}.` };
  } catch (e) {
    // Optimistic-concurrency guard: on a lost race, still revalidate so the
    // stale row disappears here instead of inviting a second doomed click.
    if (alreadyDecided(e)) refreshQueues();
    return { ok: false, message: describe(e) };
  }
}

export async function resolveContradictionAction(
  id: string,
  resolution: ContradictionResolution,
  winnerMemoryId?: string,
): Promise<ActionResult> {
  try {
    const out = await resolveContradiction(configFromEnv(), id, resolution, winnerMemoryId);
    refreshQueues();
    return { ok: true, message: `Contradiction ${out.status}.` };
  } catch (e) {
    if (alreadyDecided(e)) refreshQueues();
    return { ok: false, message: describe(e) };
  }
}
