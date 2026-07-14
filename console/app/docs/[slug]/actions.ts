"use server";

import { revalidatePath } from "next/cache";

import { ApiError, approveDocRevision, configFromEnv, editDocSection } from "@/lib/api";
import type { EditResult } from "@/docs/SectionEditor";

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

/**
 * Edit one section (KB4). The server decides what an edit *means* — a pinned
 * section is saved, a composed section is captured as proposed knowledge — so
 * the action returns the `outcome` untouched rather than deciding for it, and
 * the UI's copy is keyed off it (src/docs/edit-copy.ts).
 *
 * Revalidates the page either way: a pinned edit recomposes it, and a captured
 * edit at minimum changes what is pending review.
 */
export async function editSectionAction(
  slug: string,
  sectionId: string,
  content: string,
  note: string,
): Promise<EditResult> {
  try {
    const res = await editDocSection(configFromEnv(), slug, {
      section_id: sectionId,
      content,
      note: note.length > 0 ? note : null,
    });
    revalidatePath(`/docs/${slug}`);
    revalidatePath("/docs");
    return { ok: true, outcome: res.outcome, message: res.message };
  } catch (e) {
    return { ok: false, message: describe(e) };
  }
}
