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
import { bulkReviewPromotions } from "@/lib/governance-api";
import type { ContradictionResolution } from "@/lib/types";

export interface ActionResult {
  ok: boolean;
  message: string;
}

/**
 * The console's one route (app/console/page.tsx). Modules are `?m=` on this
 * path, and revalidatePath keys on the path alone — so this is the whole
 * console's cache key, not just the reviews module's.
 */
const CONSOLE_PATH = "/console";

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    // Shared service token → a 403 is about the token's roles, not "you".
    if (e.status === 403)
      return "The console's service token is not a maintainer of the owning team (check BRAINIAC_API_TOKEN).";
    // 404 (no longer pending) or 409 (lost the atomic approve/reject race) both
    // mean this item was decided before our write landed. We cannot know BY WHOM
    // — the backend authenticates the shared service token, not the human — so
    // the message says what is true (it is already decided) and not who did it.
    if (e.status === 404 || e.status === 409)
      return "Already decided — this item is no longer pending.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

/**
 * Refresh the queues so a decided row actually leaves the rail.
 *
 * ONE path, not two. The console was collapsed to a single route that switches
 * modules on `?m=` (app/console/page.tsx), so `/console` IS the reviews surface
 * AND the analytics surface. The two paths this used to revalidate
 * (`/console/reviews`, `/console/analytics`) have not existed since that
 * collapse, and revalidating a route that does not exist is a silent no-op:
 * every approve/reject left the decided row sitting in the rail, and clicking it
 * again produced an "already decided" note about a race that never happened.
 *
 * revalidatePath takes the PATH only — the query string is not part of it, so
 * this one call covers every module, which is exactly what we want: a promotion
 * approval also moves the analytics counters.
 */
function refreshQueues() {
  revalidatePath(CONSOLE_PATH);
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

/** Per-row outcome of a bulk decision, in the shape the rail renders. */
export interface BulkRow {
  id: string;
  ok: boolean;
  message: string;
}

export interface BulkActionResult extends ActionResult {
  decided: number;
  failed: number;
  rows: BulkRow[];
}

/**
 * Sign a whole selection.
 *
 * The server decides each id independently and returns 200 with a per-item
 * verdict, because a mixed batch is the normal case: a selection can span teams
 * this token maintains and teams it does not. Collapsing that into one
 * ok/not-ok would answer "some of them" with "no", so this keeps the rows and
 * the rail reports them per id.
 */
export async function bulkReviewAction(
  ids: string[],
  action: "approve" | "reject",
): Promise<BulkActionResult> {
  const none = { decided: 0, failed: ids.length, rows: [] as BulkRow[] };
  try {
    const out = await bulkReviewPromotions(configFromEnv(), ids, action);
    // Anything decided changed the queue — refresh even on a partial failure,
    // or the rows that DID land stay on screen looking undecided.
    if (out.decided > 0) refreshQueues();
    const rows: BulkRow[] = out.results.map((r) => ({
      id: r.promotion_id,
      ok: r.ok,
      message: r.ok
        ? `now ${r.memory_status}`
        : (describeStatus(r.status) ?? r.error ?? "failed"),
    }));
    const verb = action === "approve" ? "approved" : "rejected";
    return {
      ok: out.failed === 0,
      decided: out.decided,
      failed: out.failed,
      rows,
      message:
        out.failed === 0
          ? `${out.decided} ${verb}.`
          : `${out.decided} ${verb}, ${out.failed} refused — see the rail.`,
    };
  } catch (e) {
    // A throw here is a malformed or rejected BATCH, not a per-item verdict.
    if (alreadyDecided(e)) refreshQueues();
    return { ok: false, message: describe(e), ...none };
  }
}

/** The per-row reason, in the same voice as the single-item messages. */
function describeStatus(status: number): string | null {
  if (status === 403) return "not a maintainer of the owning team";
  if (status === 404) return "gone — decided already, or out of scope";
  if (status === 409) return "already decided";
  return null;
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
