"use client";

/*
 * Pages — the wiki (KB-PLAN KB2). Replaces DocIndex, the flat list.
 *
 * WHAT WAS WRONG, measured rather than felt. At the bank corpus the old index
 * rendered every page visible to the principal in one column: 436 rows, 6,280
 * DOM nodes, 40,797px — thirty-four screens of scroll, with no slice, no paging
 * and no way to ASK it anything. It was not slow. It was unusable, and it got
 * worse by exactly one row per page the org wrote, which is the shape of a bug
 * that never trips a test.
 *
 * THE FIX IS NOT A CAP. Capping a flat list at 50 rows would have moved the same
 * corpus behind a shorter lie. The corpus is already a hierarchy — composition
 * namespaces every slug `<team>/<page-name>` — so the structure the list was
 * missing was never missing. This renders the tree that was always there (see
 * tree.ts), and the rail costs one node per SPACE (12) instead of one per page
 * (436). Scale stops being linear in the corpus and becomes linear in the number
 * of teams, which is a number that does not run away.
 *
 * THREE RULES IT IS BUILT ON:
 *
 *  - **Bounded, and says so.** Every list that slices prints "showing N of M"
 *    in the same breath. The old module's sin was scale; replacing it with a
 *    quiet truncation would be the worse bug, because a short page looks fine.
 *  - **Reader, not a second reader.** A page opens in DocPage/DocReader, which
 *    already renders markdown with per-claim provenance. A preview pane here
 *    would be a worse copy of it fed by a second fetch, so every leaf is a link
 *    to the real thing.
 *  - **The queue is work.** `needs review` is not a filter chip, it is 84 pages
 *    with a human on the critical path, so it is a tab that carries its count.
 */

import { useMemo, useState } from "react";
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
import type { DocSummary } from "@/lib/types";

import { buildSpaces, toPage, UNFILED, type WikiPage, type WikiSpace } from "./tree";

const GOLD = band("gamma");
const GOLD_GLOW = band("gamma", 60, 0.35);
const REVIEW = band("gamma");
const DIRTY = band("theta");
const CALM = band("alpha");

/** One page of rows. The main pane never holds more than this — ever. */
const PAGE = 25;

const CHIP = `${FONT_MONO} shrink-0 rounded-full border px-2.5 py-0.5 text-[11px] uppercase tracking-[0.14em] transition`;
const META = `${FONT_MONO} text-[11px] uppercase tracking-[0.14em]`;

const when = (iso: string): string =>
  new Date(iso).toLocaleDateString(undefined, { dateStyle: "medium" });

type Tab = "all" | "review" | "dirty";

const TABS: { id: Tab; label: string; accent: string; blurb: string }[] = [
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
  docs: DocSummary[];
}

export default function DocWiki({ docs }: DocWikiProps) {
  const reduce = !!useReducedMotion();

  const pages = useMemo(() => docs.map(toPage), [docs]);
  const totals = useMemo(() => {
    const names = new Set<string>();
    for (const p of pages) names.add(p.space);
    return {
      all: pages.length,
      review: pages.filter((p) => p.doc.pending_review).length,
      dirty: pages.filter((p) => p.doc.dirty).length,
      // The org's shape, taken from the WHOLE corpus — it does not change
      // because someone typed in the search box.
      spaces: names.size,
    };
  }, [pages]);

  const [tab, setTab] = useState<Tab>("all");
  const [q, setQ] = useState("");
  const [open, setOpen] = useState<string | null>(null);

  const query = q.trim().toLowerCase();

  const matched = useMemo(() => {
    const byTab =
      tab === "review"
        ? pages.filter((p) => p.doc.pending_review)
        : tab === "dirty"
          ? pages.filter((p) => p.doc.dirty)
          : pages;
    return query ? byTab.filter((p) => p.hay.includes(query)) : byTab;
  }, [pages, tab, query]);

  // The rail is built from what the tab and the search left, so a space that
  // has nothing to review simply is not on the review rail — it does not sit
  // there reading zero and inviting a click that lands on an empty pane.
  const spaces = useMemo(() => buildSpaces(matched), [matched]);

  // The open space may not survive a tab change or a search. Resolve it against
  // the CURRENT rail every render rather than storing a name that can go stale.
  const active = open ? (spaces.find((s) => s.name === open) ?? null) : null;

  // The landing is the space directory: 436 rows flat is the bug, and a wiki's
  // front door is its spaces. A search or a queue tab is a question, though, and
  // a question deserves results — those answer flat, across every space.
  const directory = tab === "all" && !active && !query;
  const rows = active ? active.pages : matched;

  const scopeKey = `${tab}|${query}|${open ?? ""}`;
  const [pager, setPager] = useState({ key: scopeKey, offset: 0 });
  // A new question is answered from the top, not from 200 rows into the last
  // one. React's documented adjust-during-render path — an effect here would
  // paint the stale page first and then flinch.
  if (pager.key !== scopeKey) setPager({ key: scopeKey, offset: 0 });
  const start = pager.key === scopeKey ? pager.offset : 0;
  const shown = rows.slice(start, start + PAGE);
  const lastStart = rows.length === 0 ? 0 : Math.floor((rows.length - 1) / PAGE) * PAGE;

  const pick = (name: string) => {
    setOpen((cur) => (cur === name ? null : name));
  };

  const tabMeta = TABS.find((t) => t.id === tab);

  return (
    <div className="mx-auto max-w-[1560px] px-6 py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: GOLD }}>
            γ · pages · wiki
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight`} style={{ color: INK }}>
            <span style={{ color: GOLD, textShadow: `0 0 28px ${GOLD_GLOW}` }}>{totals.all}</span>{" "}
            {totals.all === 1 ? "page" : "pages"} across{" "}
            <span style={{ color: GOLD }}>{totals.spaces}</span>{" "}
            {totals.spaces === 1 ? "space" : "spaces"}
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
          const n = totals[t.id];
          const on = tab === t.id;
          return (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
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
        {/* ── the rail: one node per space, not one per page ─────────────── */}
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
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="gateway, chargeback, psp…"
            className={`${FONT_MONO} mt-1.5 w-full rounded-lg border border-white/10 bg-white/[0.02] px-2.5 py-1.5 text-sm outline-none transition placeholder:text-[#e9edff]/25 focus:border-[var(--gold)]`}
            style={{ color: INK, "--gold": GOLD } as React.CSSProperties}
          />

          <div className="mt-4 flex items-baseline justify-between gap-2">
            <span className={LABEL} style={{ color: FAINT }}>
              spaces
            </span>
            {active && (
              <button
                type="button"
                onClick={() => setOpen(null)}
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
                no space matches
              </p>
            )}
            <ul className="space-y-0.5">
              {spaces.map((s) => (
                <Folder key={s.name} space={s} open={active?.name === s.name} onPick={() => pick(s.name)} />
              ))}
            </ul>
          </nav>
        </motion.aside>

        {/* ── the pane ───────────────────────────────────────────────────── */}
        <div className="min-w-0">
          {directory ? (
            <Directory spaces={spaces} onPick={pick} reduce={reduce} />
          ) : (
            <div className="overflow-hidden rounded-xl border border-white/10 bg-white/[0.015]">
              <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 border-b border-white/10 px-4 py-2.5">
                <span className={`${FONT_DISPLAY} text-lg`} style={{ color: INK }}>
                  {active ? (active.name === UNFILED ? "unfiled" : active.name) : tabMeta?.label}
                </span>
                <span className={META} style={{ color: FAINT }}>
                  {active && tab !== "all" && `${tabMeta?.label} · `}
                  {query && `matching “${q.trim()}” · `}
                  {rows.length} {rows.length === 1 ? "page" : "pages"}
                </span>
              </div>

              <motion.ul
                initial={reduce ? false : { opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.24 }}
                className="divide-y divide-white/[0.05]"
              >
                {shown.map((p) => (
                  <PageRow key={p.doc.id} p={p} showSpace={!active} />
                ))}
              </motion.ul>

              {rows.length === 0 && <Empty tab={tab} query={query} total={totals.all} />}

              {/* the line the flat list never had */}
              <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-white/10 px-4 py-2">
                <span className={`${FONT_MONO} text-sm`} style={{ color: DIM }}>
                  {rows.length === 0 ? (
                    <>showing 0 of 0 · {totals.all} in the wiki</>
                  ) : (
                    <>
                      showing {start + 1}–{start + shown.length} of {rows.length} ·{" "}
                      {totals.all} in the wiki
                    </>
                  )}
                </span>
                {rows.length > PAGE && (
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={() => setPager({ key: scopeKey, offset: Math.max(0, start - PAGE) })}
                      disabled={start === 0}
                      className={`${CHIP} ${start === 0 ? "border-transparent" : "border-white/25 hover:border-white/60"}`}
                      style={{ color: start === 0 ? FAINT : INK }}
                    >
                      ← prev
                    </button>
                    <span className={META} style={{ color: FAINT }}>
                      page {start / PAGE + 1} / {Math.ceil(rows.length / PAGE)}
                    </span>
                    <button
                      type="button"
                      onClick={() => setPager({ key: scopeKey, offset: Math.min(lastStart, start + PAGE) })}
                      disabled={start >= lastStart}
                      className={`${CHIP} ${start >= lastStart ? "border-transparent" : "border-white/25 hover:border-white/60"}`}
                      style={{ color: start >= lastStart ? FAINT : INK }}
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
 * One folder in the rail.
 *
 * Its leaves render ONLY while it is the open space — that single condition is
 * what keeps the rail at 12 nodes instead of 436, and it is why the tree is
 * collapsed by default. At most one space's leaves are ever in the DOM.
 */
function Folder({ space, open, onPick }: { space: WikiSpace; open: boolean; onPick: () => void }) {
  const label = space.name === UNFILED ? "unfiled" : space.name;
  return (
    <li>
      <button
        type="button"
        onClick={onPick}
        aria-expanded={open}
        className={`flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left transition ${
          open ? "" : "border-transparent hover:border-white/15 hover:bg-white/[0.03]"
        }`}
        style={open ? { borderColor: withAlpha(GOLD, 0.55), background: withAlpha(GOLD, 0.09) } : undefined}
      >
        <span className={`${FONT_MONO} w-2 shrink-0 text-sm`} style={{ color: open ? GOLD : FAINT }}>
          {open ? "▾" : "▸"}
        </span>
        <span
          className={`${FONT_MONO} min-w-0 flex-1 truncate text-sm`}
          style={{ color: open ? INK : DIM }}
        >
          {label}
        </span>
        {space.review > 0 && (
          <span className={`${META} tabular-nums`} style={{ color: REVIEW }} title="waiting on a human">
            {space.review}◆
          </span>
        )}
        <span className={`${META} w-6 shrink-0 text-right tabular-nums`} style={{ color: FAINT }}>
          {space.pages.length}
        </span>
      </button>

      {open && (
        <ul className="mt-0.5 mb-1 ml-3 space-y-px border-l pl-2" style={{ borderColor: BORDER }}>
          {space.pages.map((p) => (
            <li key={p.doc.id}>
              <Link
                href={`/console/docs/${p.doc.slug}`}
                className="flex items-center gap-2 rounded px-2 py-1 transition hover:bg-white/[0.04]"
              >
                <span
                  className={`${FONT_MONO} min-w-0 flex-1 truncate text-sm`}
                  style={{ color: p.doc.pending_review ? REVIEW : DIM }}
                  title={p.doc.title}
                >
                  {p.doc.title}
                </span>
                {p.doc.dirty && (
                  <span className={META} style={{ color: DIRTY }} title="recomposing">
                    ~
                  </span>
                )}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

/** The front door: every space, what is in it, what it owes a human. */
function Directory({
  spaces,
  onPick,
  reduce,
}: {
  spaces: WikiSpace[];
  onPick: (name: string) => void;
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
        <li key={s.name}>
          <button
            type="button"
            onClick={() => onPick(s.name)}
            className="w-full rounded-xl border p-4 text-left transition hover:border-white/25 hover:bg-white/[0.03]"
            style={{ background: PANEL, borderColor: s.review > 0 ? withAlpha(REVIEW, 0.33) : BORDER }}
          >
            <div className="flex items-baseline justify-between gap-2">
              <span className={`${FONT_DISPLAY} truncate text-lg`} style={{ color: INK }}>
                {s.name === UNFILED ? "unfiled" : s.name}
              </span>
              <span className={`${FONT_MONO} shrink-0 text-lg tabular-nums`} style={{ color: GOLD }}>
                {s.pages.length}
              </span>
            </div>
            <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1">
              {s.review > 0 && (
                <span className={META} style={{ color: REVIEW }}>
                  {s.review} awaiting review
                </span>
              )}
              {s.dirty > 0 && (
                <span className={META} style={{ color: DIRTY }}>
                  {s.dirty} recomposing
                </span>
              )}
              {s.review === 0 && s.dirty === 0 && (
                <span className={META} style={{ color: CALM }}>
                  all current
                </span>
              )}
            </div>
          </button>
        </li>
      ))}
    </motion.ul>
  );
}

/** A page as a row. The whole row is the link — it opens the reader. */
function PageRow({ p, showSpace }: { p: WikiPage; showSpace: boolean }) {
  const d = p.doc;
  return (
    <li>
      <Link
        href={`/console/docs/${d.slug}`}
        className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 px-4 py-2.5 transition hover:bg-white/[0.03]"
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm" style={{ color: INK }}>
            {d.title}
          </span>
          <span className={`${FONT_MONO} mt-0.5 block truncate text-sm`} style={{ color: FAINT }}>
            /{showSpace ? d.slug : p.leaf}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-2">
          {d.pending_review && (
            <span
              className={`${META} rounded-full border px-2.5 py-0.5`}
              style={{ color: REVIEW, borderColor: withAlpha(REVIEW, 0.4), background: withAlpha(REVIEW, 0.07) }}
            >
              awaiting review
            </span>
          )}
          {d.dirty && (
            <span
              className={`${META} rounded-full border px-2.5 py-0.5`}
              style={{ color: DIRTY, borderColor: withAlpha(DIRTY, 0.33) }}
            >
              recomposing
            </span>
          )}
          {/* A page that is bound but never composed is a real state, not a
              blank row — the reader says so too. */}
          {d.status !== "published" && (
            <span className={META} style={{ color: FAINT }}>
              {d.status}
            </span>
          )}
          <span className={`${META} hidden md:block`} style={{ color: FAINT }}>
            {d.doc_kind.replace("_", " ")}
          </span>
          <span
            className={`${META} hidden w-28 shrink-0 text-right whitespace-nowrap lg:block`}
            style={{ color: FAINT }}
          >
            {when(d.updated_at)}
          </span>
        </span>
      </Link>
    </li>
  );
}

function Empty({ tab, query, total }: { tab: Tab; query: string; total: number }) {
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
