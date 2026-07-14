/*
 * The words the section editor is allowed to say (KB-PLAN KB4).
 *
 * A composed section is not a wiki paragraph. It is a *projection of the org's
 * memories*: the maintainer's text is never written into the page. It is sent
 * through extraction, becomes candidate memories, waits for the same human
 * review gate as every other candidate, and the section recomposes only once
 * they land. The server refuses to call that "saved", and so does this file —
 * telling someone their edit was saved when it is in fact a proposal is the
 * single most damaging lie this surface could tell.
 *
 * So the copy lives in one table, keyed by the two facts the UI has: the
 * section's mode (known BEFORE typing — the warning has to arrive before the
 * keystrokes, not after) and the server's returned outcome.
 */

import type { DocSectionMode, EditOutcome } from "@/lib/types";

export interface EditIntent {
  /** Above the textarea, before a word is typed. */
  label: string;
  /** What will happen to what you are about to write. */
  warning: string;
  /** The submit button. */
  cta: string;
  cta_pending: string;
}

/** The promise made BEFORE the maintainer types. */
export const INTENT: Record<DocSectionMode, EditIntent> = {
  pinned: {
    label: "pinned section — your prose",
    warning:
      "This section is human-owned. What you write is written to the page verbatim, and " +
      "regeneration never touches it.",
    cta: "save this prose",
    cta_pending: "saving…",
  },
  composed: {
    label: "composed section — compiled from memories",
    warning:
      "This section is compiled from the org's memories, so your edit does not become the " +
      "page — it becomes proposed knowledge. It goes through extraction, arrives as candidate " +
      "memories, and waits for the same human review as everything else. The section rewrites " +
      "itself once they land.",
    cta: "propose this change",
    cta_pending: "capturing…",
  },
};

export interface EditOutcomeCopy {
  /** Short status word. Never "Saved" for a captured edit. */
  status: string;
  /** What happens next, in order. */
  next: string;
  /** Green (a terminal, finished action) vs amber (queued, still owed work). */
  tone: "done" | "queued";
}

/** The truth AFTER the server answers. */
export const OUTCOME: Record<EditOutcome, EditOutcomeCopy> = {
  saved: {
    status: "Saved to the page",
    next: "The page recomposes so the published markdown carries your prose. Nothing regenerates over it.",
    tone: "done",
  },
  captured: {
    status: "Captured as proposed knowledge",
    next: "Queued for extraction → candidate memories → human review. Your text was not written into the page; the section will recompose on its own once the memories are approved.",
    tone: "queued",
  },
};

/** The wire type is a bare string. Anything we do not recognise is treated as
 *  `captured` — the conservative read: it claims less. */
export const asOutcome = (s: string): EditOutcome => (s === "saved" ? "saved" : "captured");

export const asMode = (s: string): DocSectionMode => (s === "pinned" ? "pinned" : "composed");
