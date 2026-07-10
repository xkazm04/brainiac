"use server";

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
    if (e.status === 403) return "You need to be a maintainer of the owning team.";
    if (e.status === 404) return "Item is gone or already reviewed.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

export async function reviewPromotionAction(
  id: string,
  action: "approve" | "reject",
): Promise<ActionResult> {
  try {
    const out = await reviewPromotion(configFromEnv(), id, action);
    revalidatePath("/reviews");
    revalidatePath("/analytics");
    return { ok: true, message: `Memory is now ${out.memory_status}.` };
  } catch (e) {
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
    revalidatePath("/reviews");
    revalidatePath("/analytics");
    return { ok: true, message: `Contradiction ${out.status}.` };
  } catch (e) {
    return { ok: false, message: describe(e) };
  }
}
