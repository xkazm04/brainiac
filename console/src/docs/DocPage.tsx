/*
 * The page shell around the reader (KB-PLAN KB2): what this page is, when it
 * was last published, what is waiting on a human, and how it got here.
 *
 * Server component — DocReader (provenance popovers) and ApproveRevision (the
 * publish gate) are the only client islands.
 */

import Link from "next/link";

import {
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  PANEL,
  band,
} from "@/design/theme";
import type { DocDetail, DocRevisionSummary, RevisionPolicy } from "@/lib/types";

import { asPolicy } from "./facets";
import ApproveRevision, { type ApproveRevisionProps } from "./ApproveRevision";
import DocReader from "./DocReader";
import type { SectionEditorProps } from "./SectionEditor";

const when = (iso: string | null): string =>
  iso ? new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : "—";

/** auto_published vs needs_review is the governance fact of a revision. */
const POLICY: Record<RevisionPolicy, { label: string; accent: string; note: string }> = {
  auto_published: {
    label: "auto-published",
    accent: band("beta"),
    note: "additive recompose — every previously published claim survived",
  },
  needs_review: {
    label: "human-approved",
    accent: band("alpha"),
    note: "held for a human: a first revision, or a claim was dropped",
  },
  rejected: {
    label: "rejected",
    accent: MAGENTA,
    note: "a human refused this revision",
  },
};

function Chip({ children, accent }: { children: React.ReactNode; accent: string }) {
  return (
    <span
      className={`${FONT_MONO} rounded-full border px-2.5 py-[2px] text-[10px] uppercase tracking-[0.14em]`}
      style={{ color: accent, borderColor: `${accent}55` }}
    >
      {children}
    </span>
  );
}

export interface DocPageProps {
  detail: DocDetail;
  revisions: DocRevisionSummary[];
  /** The approve server action — omitted offline, where the banner is read-only. */
  approve?: ApproveRevisionProps["approve"];
  /** The section-edit server action (KB4) — omitted offline, same rule. */
  edit?: SectionEditorProps["edit"];
}

export default function DocPage({ detail, revisions, approve, edit }: DocPageProps) {
  const { document: doc, revision, pending, citations, sections } = detail;
  const accent = band("gamma");

  return (
    <main className="mx-auto max-w-7xl px-6 py-10">
      <Link
        href="/docs"
        className={`${FONT_MONO} text-[11px] uppercase tracking-[0.18em]`}
        style={{ color: INK_FAINT }}
      >
        ← pages
      </Link>

      <header className="mt-4 border-b pb-6" style={{ borderColor: BORDER }}>
        <h1 className={`${FONT_DISPLAY} text-4xl font-medium`} style={{ color: INK }}>
          {doc.title}
        </h1>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Chip accent={accent}>{doc.doc_kind.replace("_", " ")}</Chip>
          <Chip accent={doc.visibility === "org" ? band("beta") : band("theta")}>
            {doc.visibility}
          </Chip>
          <Chip accent={doc.status === "published" ? band("alpha") : INK_FAINT}>{doc.status}</Chip>
          <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
            {revision?.published_at
              ? `last published ${when(revision.published_at)}`
              : "never published"}
            {" · "}
            {citations.length} governed {citations.length === 1 ? "memory" : "memories"} behind this
            page
          </span>
        </div>
      </header>

      {pending && (
        <div className="mt-6">
          {approve ? (
            <ApproveRevision
              revisionId={pending.id}
              createdAt={pending.created_at}
              policy={pending.policy_decision}
              approve={approve}
            />
          ) : (
            // Offline: state the fact, but never wire a publish button to demo data.
            <div
              className="rounded-lg border p-5"
              style={{ borderColor: `${accent}40`, background: `${accent}0d` }}
            >
              <span className={LABEL} style={{ color: accent }}>
                revision awaiting review
              </span>
              <p className={`${FONT_MONO} mt-1.5 text-[12px]`} style={{ color: INK_DIM }}>
                Composed {when(pending.created_at)}. Approving is disabled while the console is
                showing demo data.
              </p>
            </div>
          )}
        </div>
      )}

      <div className="mt-10">
        {revision ? (
          <DocReader
            contentMd={revision.content_md}
            citations={citations}
            sections={sections}
            edit={edit}
          />
        ) : pending ? (
          <>
            <p className={`${FONT_MONO} mb-4 text-[12px]`} style={{ color: INK_FAINT }}>
              This page has never been published — what follows is the revision awaiting review.
            </p>
            <DocReader
              contentMd={pending.content_md}
              citations={citations}
              sections={sections}
              draft
            />
          </>
        ) : (
          <p className={`${FONT_MONO} text-[13px]`} style={{ color: INK_DIM }}>
            No revision yet — the page is bound but has not been composed.
          </p>
        )}
      </div>

      <section className="mt-16 border-t pt-8" style={{ borderColor: BORDER }}>
        <h2 className={`${FONT_DISPLAY} text-xl`} style={{ color: INK }}>
          Revision history
        </h2>
        <p className={`${FONT_MONO} mt-1 text-[11px]`} style={{ color: INK_FAINT }}>
          A revision that changed nothing a human had already approved publishes itself; anything
          that drops a published claim waits for one.
        </p>
        <ul className="mt-5 space-y-2">
          {revisions.length === 0 && (
            <li className={`${FONT_MONO} text-[12px]`} style={{ color: INK_DIM }}>
              No revisions recorded.
            </li>
          )}
          {revisions.map((r) => {
            const p = POLICY[asPolicy(r.policy_decision)];
            const isCurrent = revision?.id === r.id;
            return (
              <li
                key={r.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-lg p-4"
                style={{
                  background: PANEL,
                  border: `1px solid ${isCurrent ? `${p.accent}55` : BORDER}`,
                }}
              >
                <div className="flex items-center gap-3">
                  <Chip accent={p.accent}>{p.label}</Chip>
                  {isCurrent && <Chip accent={INK_FAINT}>current</Chip>}
                  <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
                    {p.note}
                  </span>
                </div>
                <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_DIM }}>
                  {r.composed_from.length} memories · composed {when(r.created_at)} ·{" "}
                  {r.published_at ? `published ${when(r.published_at)}` : "unpublished"}
                </span>
              </li>
            );
          })}
        </ul>
      </section>
    </main>
  );
}
