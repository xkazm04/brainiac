"use server";

// TRUST MODEL (single-operator console): adjudication runs under one shared
// service token (BRAINIAC_API_TOKEN), not a per-user identity, so a 403 is about
// that token's team roles rather than the human. Deliberate single-maintainer
// posture — see reviews/actions.ts and INDEX.md Theme A.

import { revalidatePath } from "next/cache";

import { ApiError, configFromEnv } from "@/lib/api";
import { resolveDispute } from "@/lib/governance-api";

import type { Resolution } from "./disputes-data";

export interface DecisionResult {
  ok: boolean;
  message: string;
}

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 403)
      return "The console's service token is not a maintainer of the owning team (check BRAINIAC_API_TOKEN).";
    if (e.status === 404 || e.status === 409)
      return "Already answered in another session — the bench has been refreshed.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

function refreshBench() {
  revalidatePath("/console/disputes");
  revalidatePath("/console/analytics");
}

const SAID: Record<Resolution, string> = {
  reverified: "Re-verified — validity window extended",
  deprecated: "Deprecated — dropped out of retrieval",
  dismissed: "Reports dismissed — the memory stands",
};

/**
 * `days` applies to `reverified` only — it is the new validity budget, and the
 * server ignores it for the other two (they do not move the boundary). Passing
 * it from anywhere else would be a number with no effect, so the caller sends
 * it only with a re-verification and this asserts that here rather than
 * trusting it.
 */
export async function resolveDisputeAction(
  memoryId: string,
  resolution: Resolution,
  days?: number,
): Promise<DecisionResult> {
  try {
    const budget = resolution === "reverified" ? days : undefined;
    const out = await resolveDispute(configFromEnv(), memoryId, resolution, budget);
    refreshBench();
    const extended = budget ? ` (${budget}d)` : "";
    return {
      ok: true,
      message: `${SAID[resolution]}${extended} · ${out.claims_closed} claim${
        out.claims_closed === 1 ? "" : "s"
      } closed.`,
    };
  } catch (e) {
    // On a lost race (already answered elsewhere) still refresh, so the stale
    // dispute clears instead of tempting a second adjudication.
    if (e instanceof ApiError && (e.status === 404 || e.status === 409)) refreshBench();
    return { ok: false, message: describe(e) };
  }
}
