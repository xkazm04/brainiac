"use client";

/*
 * The publish gate. An unpublished revision is the one thing on the document
 * layer that is waiting on a human, so it gets a banner, not a menu item.
 *
 * The button is only ever rendered when the console is live (app/docs/[slug]
 * decides): a mutating control wired to demo data would be a lie with
 * consequences.
 */

import { useState, useTransition } from "react";

import {
  band,
  BORDER,
  FONT_MONO,
  INK,
  INK_DIM,
  LABEL,
  withAlpha,
} from "@/design/theme";

export interface ApproveRevisionProps {
  revisionId: string;
  createdAt: string;
  policy: string;
  /** Server action: POST /v1/docs/revisions/{id}/approve, then revalidate. */
  approve: (id: string) => Promise<{ ok: boolean; message: string }>;
}

export default function ApproveRevision({
  revisionId,
  createdAt,
  policy,
  approve,
}: ApproveRevisionProps) {
  const [pending, start] = useTransition();
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const accent = band("gamma");

  return (
    <div
      className="rounded-lg border p-5"
      style={{ borderColor: withAlpha(accent, 0.33), background: withAlpha(accent, 0.06) }}
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <span className={LABEL} style={{ color: accent }}>
            revision awaiting review
          </span>
          <p className="mt-1.5 text-[14px]" style={{ color: INK }}>
            A recomposed version of this page is held back from publication.
          </p>
          <p className={`${FONT_MONO} mt-1 text-[11px]`} style={{ color: INK_DIM }}>
            composed {new Date(createdAt).toLocaleString()} · policy {policy} · the reader below
            still shows the published revision
          </p>
        </div>
        <button
          type="button"
          disabled={pending || result?.ok === true}
          onClick={() =>
            start(async () => {
              setResult(await approve(revisionId));
            })
          }
          className={`${FONT_MONO} rounded-full border px-5 py-2 text-sm transition hover:bg-white/5 disabled:opacity-50`}
          style={{ borderColor: withAlpha(accent, 0.47), color: accent }}
        >
          {pending ? "publishing…" : result?.ok ? "published" : "approve & publish"}
        </button>
      </div>
      {result && (
        <p
          className={`${FONT_MONO} mt-3 border-t pt-3 text-[12px]`}
          style={{ borderColor: BORDER, color: result.ok ? band("beta") : "#ff5da2" }}
        >
          {result.message}
        </p>
      )}
    </div>
  );
}
