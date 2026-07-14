"use server";

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
    if (e.status === 403) return "You need to be a maintainer of the owning team.";
    if (e.status === 404) return "Memory is gone or already answered.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

const SAID: Record<Resolution, string> = {
  reverified: "Re-verified — validity window extended",
  deprecated: "Deprecated — dropped out of retrieval",
  dismissed: "Reports dismissed — the memory stands",
};

export async function resolveDisputeAction(
  memoryId: string,
  resolution: Resolution,
): Promise<DecisionResult> {
  try {
    const out = await resolveDispute(configFromEnv(), memoryId, resolution);
    revalidatePath("/console/disputes");
    revalidatePath("/console/analytics");
    return {
      ok: true,
      message: `${SAID[resolution]} · ${out.claims_closed} claim${
        out.claims_closed === 1 ? "" : "s"
      } closed.`,
    };
  } catch (e) {
    return { ok: false, message: describe(e) };
  }
}
