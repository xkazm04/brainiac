"use server";

import { revalidatePath } from "next/cache";

import { ApiError, approveDocRevision, configFromEnv } from "@/lib/api";

export interface ActionResult {
  ok: boolean;
  message: string;
}

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 403) return "You need to be a maintainer of the owning team.";
    if (e.status === 404) return "That revision is gone or already reviewed.";
    return `API error ${e.status}: ${e.message}`;
  }
  return e instanceof Error ? e.message : String(e);
}

/**
 * Publish a pending revision. Only ever reachable from a live console — the
 * page does not pass this action down when it is rendering demo data. `slug`
 * comes first so the page can `.bind(null, slug)` it into the client island.
 */
export async function approveRevisionAction(
  slug: string,
  revisionId: string,
): Promise<ActionResult> {
  try {
    await approveDocRevision(configFromEnv(), revisionId);
    revalidatePath(`/docs/${slug}`);
    revalidatePath("/docs");
    return { ok: true, message: "Revision published — this page now serves it." };
  } catch (e) {
    return { ok: false, message: describe(e) };
  }
}
