"use client";

/*
 * Pages — the wiki (KB-PLAN KB2), server-driven edition.
 *
 * WHAT WAS WRONG, measured rather than felt. The first cut of this module fixed
 * the DOM — the flat 436-row / 40,797px index became a tree — but it still
 * FETCHED the whole visible corpus in one trip and built the tree, the tab
 * counts, the search and the pagination in the browser over that array. That is
 * an O(corpus) transfer whose only job is to be mostly discarded, and it grows
 * by one summary per page the org writes: the exact shape of a bug that never
 * trips a test, one layer down from the one already fixed.
 *
 * THE FIX MOVES THE WORK TO THE SERVER. GET /v1/docs is now paginated and
 * faceted, so the browser holds only what it paints: the space directory is the
 * server's `facets.spaces` (cross-filtered — a dimension never shrinks its own
 * menu, so the rail always lists every space and stays browsable), the tab
 * counts are `facets.needs_review` / `facets.dirty` / the facet total, and the
 * pane is one server-windowed page of rows. The URL is the single source of
 * truth (`?m=docs&space=payments&tab=review&q=…&page=2`), so every view is
 * shareable and survives a refresh, and every space/tab/search is a server
 * round trip — never a client `.slice` over a corpus that is no longer here.
 *
 * The three rules it was built on still hold: bounded and says so ("showing N of
 * M · T in the wiki"); the leaf is a link to the real reader, never a second
 * one; and `needs review` is work, so it is a tab that carries its count.
 */

import { useCallback, useRef } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, useReducedMotion } from "framer-motion";
import Link from "next/link";

import {
  band,
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  INK,
  INK_DIM as DIM,
  INK_FAINT as FAINT,
  LABEL,
  PANEL,
  withAlpha,
} from "@/design/theme";
import type { DocFacet, DocSummary } from "@/lib/types";

import { leafName, spaceLabel } from "./tree";
import type { WikiData, WikiTab } from "./wiki-data";

const GOLD = band("gamma");
const GOLD_GLOW = band("gamma", 60, 0.35);
const REVIEW = band("gamma");
const DIRTY = band("theta");

const CHIP = `${FONT_MONO} shrink-0 rounded-full border px-2.5 py-0.5 text-[11px] uppercase tracking-[0.14em] transition`;
const META = `${FONT_MONO} text-[11px] uppercase tracking-[0.14em]`;

const when = (iso: string): string =>
  new Date(iso).toLocaleDateString(undefined, { dateStyle: "medium" });

const TABS: { id: WikiTab; label: string; accent: string; blurb: string }[] = [
  { id: "all", label: "all pages", accent: GOLD, blurb: "" },
  {
    id: "review",
    label: "needs review",
    accent: REVIEW,
    blurb:
      "A recomposed revision is held back from publication until someone approves it. Every page here has work stopped on a human.",
  },
  {
    id: "dirty",
    label: "recomposing",
    accent: DIRTY,
    blurb:
      "A memory these pages depend on moved and the recompose is queued. Nobody has to do anything — but what you read now is knowingly behind its sources.",
  },
];

export interface DocWikiProps {
  data: WikiData;
}

export default function DocWiki({ data }: DocWikiProps) {
  const reduce = !!useReducedMotion();
  const router = useRouter();
  const searchParams = useSearchParams();

  const { documents, total, spaces, tabCounts, filter, page, pageSize } = data;
  const { tab, space, q } = filter;

  const pages = Math.max(1, Math.ceil(total / pageSize));
  const activeSpace = space !== undefined;

  // ── URL as the single source of truth ─────────────────────────────────
  const setParams = useCallback(
    (mutate: (p: URLSearchParams) => void, resetPage = true) => {
      const p = new URLSearchParams(searchParams.toString());
      p.set("m", "docs");
      mutate(p);
      if (resetPage) p.delete("page");
      router.push(`/console?${p.toString()}`, { scroll: false });
    },
    [router, searchParams],
  );

  const pickTab = (t: WikiTab) =>
    setParams((p) => {
      if (t === "all") p.delete("tab");
      else p.set("tab", t);
    });

  const pickSpace = (value: string) =>
    setParams((p) => {
      if (space === value) p.delete("space");
      else p.set("space", value);
    });

  const clearSpace = () => setParams((p) => p.delete("space"));

  const gotoPage = (n: number) =>
    setParams((p) => {
      if (n <= 0) p.delete("page");
      else p.set("page", String(n));
    }, false);

  // Search re-fetches server-side. Debounced so a burst of keystrokes is one
  // round trip; the input is uncontrolled and keyed to the URL's q, so it seeds
  // correctly and never fights the navigation it triggers.
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onSearch = useCallback(
    (v: string) => {
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        setParams((p) => {
          const t = v.trim();
          if (t) p.set("q", t);
          else p.delete("q");
        });
      }, 250);
    },
    [setParams],
  );

  const tabMeta = TABS.find((t) => t.id === tab);
  // The front door is the space directory — but a search or a queue tab is a
  // question, and a question answers flat, across every space.
  const directory = tab === "all" && !activeSpace && !q;
  const start = page * pageSize;

  return (
    <div className="mx-auto max-w-[1560px] px-6 py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: GOLD }}>
            γ · pages · wiki
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight`} style={{ color: INK }}>
            <span style={{ color: GOLD, textShadow: `0 0 28px ${GOLD_GLOW}` }}>{tabCounts.all}</span>{" "}
            {tabCounts.all === 1 ? "page" : "pages"} across{" "}
            <span style={{ color: GOLD }}>{spaces.length}</span>{" "}
            {spaces.length === 1 ? "space" : "spaces"}
          </h1>
        </div>
        <p className={`${FONT_MONO} max-w-md text-sm leading-relaxed`} style={{ color: FAINT }}>
          Every page is a projection over canonical memories — compiled, cited sentence by sentence,
          recomposed when a memory it depends on changes. None of it is hand-written prose that can
          quietly go stale.
        </p>
      </div>

      {/* view mode */}
      <div className="mt-5 flex flex-wrap items-center gap-2">
        {TABS.map((t) => {
          const n = tabCounts[t.id];
          const on = tab === t.id;
          return (
            <button
              key={t.id}
              type="button"
              onClick={() => pickTab(t.id)}
              aria-pressed={on}
              className={`${CHIP} ${on ? "" : "border-white/15 hover:border-white/40"}`}
              style={
                on
                  ? { color: t.accent, borderColor: withAlpha(t.accent, 0.55), background: withAlpha(t.accent, 0.1) }
                  : { color: n === 0 ? FAINT : DIM }
              }
            >
              {t.label}
              {t.id !== "all" && <span className="ml-2 tabular-nums">{n}</span>}
            </button>
          );
        })}
      </div>

      {tabMeta?.blurb && (
        <p className={`${FONT_MONO} mt-3 max-w-3xl text-sm leading-relaxed`} style={{ color: DIM }}>
          {tabMeta.blurb}
        </p>
      )}

      <div className="mt-5 grid gap-5 lg:grid-cols-[264px_minmax(0,1fr)]">
        {/* ── the rail: one node per space, from the server's facet menu ──── */}
        <motion.aside
          initial={reduce ? false : { opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3 }}
          className="lg:sticky lg:top-4 lg:self-start"
        >
          <label className={LABEL} style={{ color: FAINT }} htmlFor="wiki-q">
            search pages
          </label>
          <input
            id="wiki-q"
            key={q ?? ""}
            defaultValue={q ?? ""}
            onChange={(e) => onSearch(e.target.value)}
            placeholder="gateway, chargeback, psp…"
            className={`${FONT_MONO} mt-1.5 w-full rounded-lg border border-white/10 bg-white/[0.02] px-2.5 py-1.5 text-sm outline-none transition placeholder:text-[#e9edff]/25 focus:border-[var(--gold)]`}
            style={{ color: INK, "--gold": GOLD } as React.CSSProperties}
          />

          <div className="mt-4 flex items-baseline justify-between gap-2">
            <span className={LABEL} style={{ color: FAINT }}>
              spaces
            </span>
            {activeSpace && (
              <button
                type="button"
                onClick={clearSpace}
                className={`${META} underline underline-offset-4 transition hover:brightness-125`}
                style={{ color: GOLD }}
              >
                all spaces
              </button>
            )}
          </div>

          <nav className="mt-1.5 max-h-[62vh] overflow-y-auto pr-1">
            {spaces.length === 0 && (
              <p className={`${FONT_MONO} py-6 text-sm`} style={{ color: FAINT }}>
                no spaces yet
              </p>
            )}
            <ul className="space-y-0.5">
              {spaces.map((s) => (
                <Folder key={s.value} space={s} open={space === s.value} onPick={() => pickSpace(s.value)} />
              ))}
            </ul>
          </nav>
        </motion.aside>

        {/* ── the pane ───────────────────────────────────────────────────── */}
        <div className="min-w-0">
          {directory ? (
            <Directory spaces={spaces} onPick={pickSpace} reduce={reduce} />
          ) : (
            <div className="overflow-hidden rounded-xl border border-white/10 bg-white/[0.015]">
              <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 border-b border-white/10 px-4 py-2.5">
                <span className={`${FONT_DISPLAY} text-lg`} style={{ color: INK }}>
                  {activeSpace ? spaceLabel(space) : tabMeta?.label}
                </span>
                <span className={META} style={{ color: FAINT }}>
                  {activeSpace && tab !== "all" && `${tabMeta?.label} · `}
                  {q && `matching “${q}” · `}
                  {total} {total === 1 ? "page" : "pages"}
                </span>
              </div>

              <motion.ul
                key={`${space ?? ""}|${tab}|${q ?? ""}|${page}`}
                initial={reduce ? false : { opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.24 }}
                className="divide-y divide-white/[0.05]"
              >
                {documents.map((d) => (
                  <PageRow key={d.id} doc={d} showSpace={!activeSpace} />
                ))}
              </motion.ul>

              {documents.length === 0 && <Empty tab={tab} query={q} total={tabCounts.all} />}

              {/* the line the flat list never had */}
              <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-white/10 px-4 py-2">
                <span className={`${FONT_MONO} text-sm`} style={{ color: DIM }}>
                  {total === 0 ? (
                    <>showing 0 of 0 · {tabCounts.all} in the wiki</>
                  ) : (
                    <>
                      showing {start + 1}–{start + documents.length} of {total} ·{" "}
                      {tabCounts.all} in the wiki
                    </>
                  )}
                </span>
                {pages > 1 && (
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={() => gotoPage(page - 1)}
                      disabled={page <= 0}
                      className={`${CHIP} ${page <= 0 ? "border-transparent" : "border-white/25 hover:border-white/60"}`}
                      style={{ color: page <= 0 ? FAINT : INK }}
                    >
                      ← prev
                    </button>
                    <span className={META} style={{ color: FAINT }}>
                      page {page + 1} / {pages}
                    </span>
                    <button
                      type="button"
                      onClick={() => gotoPage(page + 1)}
                      disabled={page >= pages - 1}
                      className={`${CHIP} ${page >= pages - 1 ? "border-transparent" : "border-white/25 hover:border-white/60"}`}
                      style={{ color: page >= pages - 1 ? FAINT : INK }}
                    >
                      next →
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * One space in the rail. It is a LINK to that space's page, not an inline
 * expander: the space's leaves live in the main pane (the server's page fetch),
 * so the rail costs exactly one node per space whatever the corpus does.
 */
function Folder({ space, open, onPick }: { space: DocFacet; open: boolean; onPick: () => void }) {
  return (
    <li>
      <button
        type="button"
        onClick={onPick}
        aria-pressed={open}
        className={`flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left transition ${
          open ? "" : "border-transparent hover:border-white/15 hover:bg-white/[0.03]"
        }`}
        style={open ? { borderColor: withAlpha(GOLD, 0.55), background: withAlpha(GOLD, 0.09) } : undefined}
      >
        <span className={`${FONT_MONO} w-2 shrink-0 text-sm`} style={{ color: open ? GOLD : FAINT }}>
          {open ? "▸" : "·"}
        </span>
        <span className={`${FONT_MONO} min-w-0 flex-1 truncate text-sm`} style={{ color: open ? INK : DIM }}>
          {spaceLabel(space.value)}
        </span>
        <span className={`${META} w-6 shrink-0 text-right tabular-nums`} style={{ color: FAINT }}>
          {space.count}
        </span>
      </button>
    </li>
  );
}

/** The front door: every space and its page count, from the server's directory. */
function Directory({
  spaces,
  onPick,
  reduce,
}: {
  spaces: DocFacet[];
  onPick: (value: string) => void;
  reduce: boolean;
}) {
  if (spaces.length === 0) {
    return (
      <div className="rounded-xl border border-white/10 bg-white/[0.015] px-4 py-16">
        <p className={`${FONT_MONO} text-center text-sm`} style={{ color: FAINT }}>
          No pages yet — pages scaffold themselves once an entity carries enough canonical memories.
        </p>
      </div>
    );
  }
  return (
    <motion.ul
      initial={reduce ? false : { opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.24 }}
      className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3"
    >
      {spaces.map((s) => (
        <li key={s.value}>
          <button
            type="button"
            onClick={() => onPick(s.value)}
            className="w-full rounded-xl border p-4 text-left transition hover:border-white/25 hover:bg-white/[0.03]"
            style={{ background: PANEL, borderColor: BORDER }}
          >
            <div className="flex items-baseline justify-between gap-2">
              <span className={`${FONT_DISPLAY} truncate text-lg`} style={{ color: INK }}>
                {spaceLabel(s.value)}
              </span>
              <span className={`${FONT_MONO} shrink-0 text-lg tabular-nums`} style={{ color: GOLD }}>
                {s.count}
              </span>
            </div>
            <div className="mt-2">
              <span className={META} style={{ color: FAINT }}>
                {s.count === 1 ? "page" : "pages"}
              </span>
            </div>
          </button>
        </li>
      ))}
    </motion.ul>
  );
}

/** A page as a row. The whole row is the link — it opens the reader. */
function PageRow({ doc, showSpace }: { doc: DocSummary; showSpace: boolean }) {
  return (
    <li>
      <Link
        href={`/console/docs/${doc.slug}`}
        className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 px-4 py-2.5 transition hover:bg-white/[0.03]"
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm" style={{ color: INK }}>
            {doc.title}
          </span>
          <span className={`${FONT_MONO} mt-0.5 block truncate text-sm`} style={{ color: FAINT }}>
            /{showSpace ? doc.slug : leafName(doc.slug)}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-2">
          {doc.pending_review && (
            <span
              className={`${META} rounded-full border px-2.5 py-0.5`}
              style={{ color: REVIEW, borderColor: withAlpha(REVIEW, 0.4), background: withAlpha(REVIEW, 0.07) }}
            >
              awaiting review
            </span>
          )}
          {doc.dirty && (
            <span
              className={`${META} rounded-full border px-2.5 py-0.5`}
              style={{ color: DIRTY, borderColor: withAlpha(DIRTY, 0.33) }}
            >
              recomposing
            </span>
          )}
          {/* A page that is bound but never composed is a real state, not a
              blank row — the reader says so too. */}
          {doc.status !== "published" && (
            <span className={META} style={{ color: FAINT }}>
              {doc.status}
            </span>
          )}
          <span className={`${META} hidden md:block`} style={{ color: FAINT }}>
            {doc.doc_kind.replace("_", " ")}
          </span>
          <span
            className={`${META} hidden w-28 shrink-0 text-right whitespace-nowrap lg:block`}
            style={{ color: FAINT }}
          >
            {when(doc.updated_at)}
          </span>
        </span>
      </Link>
    </li>
  );
}

function Empty({ tab, query, total }: { tab: WikiTab; query: string | undefined; total: number }) {
  const msg =
    total === 0
      ? "No pages yet — pages scaffold themselves once an entity carries enough canonical memories."
      : query
        ? "No page matches that search — try a shorter term, or clear the queue tab."
        : tab === "review"
          ? "Nothing is waiting on a human. Every composed revision is published."
          : tab === "dirty"
            ? "No page is behind its sources — every recompose has landed."
            : "This space has no pages.";
  return (
    <p className={`${FONT_MONO} px-4 py-16 text-center text-sm`} style={{ color: FAINT }}>
      {msg}
    </p>
  );
}
