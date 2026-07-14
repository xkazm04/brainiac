/*
 * The page index (KB-PLAN KB2).
 *
 * Ordering is editorial: a revision awaiting review is work stopped on a
 * human, so those pages sort to the top and get their own section. Dirty pages
 * come next — the anti-rot machinery has already noticed a bound memory moved
 * and queued a recompose; nobody has to do anything, but the page you are about
 * to read is knowingly behind its sources, and the reader should be told.
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
  PANEL,
  band,
} from "@/design/theme";
import type { DocSummary } from "@/lib/types";

const when = (iso: string): string =>
  new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

function Row({ d, pendingReview }: { d: DocSummary; pendingReview: boolean }) {
  const accent = pendingReview ? band("gamma") : d.dirty ? band("theta") : band("alpha");
  return (
    <li>
      <Link
        href={`/docs/${d.slug}`}
        className="flex flex-wrap items-center justify-between gap-3 rounded-lg p-4 transition hover:bg-white/[0.04]"
        style={{ background: PANEL, border: `1px solid ${pendingReview ? `${accent}55` : BORDER}` }}
      >
        <div className="min-w-0">
          <span className={`${FONT_DISPLAY} text-lg`} style={{ color: INK }}>
            {d.title}
          </span>
          <span
            className={`${FONT_MONO} mt-1 flex flex-wrap gap-x-3 text-[11px]`}
            style={{ color: INK_FAINT }}
          >
            <span>/{d.slug}</span>
            <span>{d.doc_kind.replace("_", " ")}</span>
            <span>{d.visibility}</span>
            <span>{d.status}</span>
            <span>updated {when(d.updated_at)}</span>
          </span>
        </div>
        <div className="flex items-center gap-2">
          {pendingReview && (
            <span
              className={`${FONT_MONO} rounded-full border px-3 py-1 text-[10px] uppercase tracking-[0.14em]`}
              style={{ color: accent, borderColor: `${accent}66`, background: `${accent}12` }}
            >
              awaiting review
            </span>
          )}
          {d.dirty && (
            <span
              className={`${FONT_MONO} rounded-full border px-3 py-1 text-[10px] uppercase tracking-[0.14em]`}
              style={{ color: band("theta"), borderColor: `${band("theta")}55` }}
            >
              recomposing
            </span>
          )}
        </div>
      </Link>
    </li>
  );
}

export interface DocIndexProps {
  docs: DocSummary[];
}

export default function DocIndex({ docs }: DocIndexProps) {
  // `pending_review` comes straight off the list row — no per-page fan-out.
  const waiting = docs.filter((d) => d.pending_review);
  const rest = docs.filter((d) => !d.pending_review);

  return (
    <main className="mx-auto max-w-5xl px-6 py-10">
      <h1 className={`${FONT_DISPLAY} text-4xl font-medium`} style={{ color: INK }}>
        Pages
      </h1>
      <p className="mt-3 max-w-2xl text-[14px] leading-relaxed" style={{ color: INK_DIM }}>
        Every page here is a projection over canonical memories — compiled, cited sentence by
        sentence, and recomposed when a memory it depends on changes. None of it is hand-written
        prose that can quietly go stale.
      </p>

      {waiting.length > 0 && (
        <section className="mt-10">
          <span className={LABEL} style={{ color: band("gamma") }}>
            waiting on a human · {waiting.length}
          </span>
          <p className={`${FONT_MONO} mt-1 text-[11px]`} style={{ color: INK_FAINT }}>
            A recomposed revision is held back from publication until someone approves it.
          </p>
          <ul className="mt-4 space-y-2">
            {waiting.map((d) => (
              <Row key={d.id} d={d} pendingReview />
            ))}
          </ul>
        </section>
      )}

      <section className="mt-10">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          all pages · {docs.length}
        </span>
        <ul className="mt-4 space-y-2">
          {rest.map((d) => (
            <Row key={d.id} d={d} pendingReview={false} />
          ))}
          {docs.length === 0 && (
            <li className={`${FONT_MONO} text-[12px]`} style={{ color: INK_DIM }}>
              No pages yet — pages scaffold themselves once an entity carries enough canonical
              memories.
            </li>
          )}
        </ul>
      </section>
    </main>
  );
}
