"use client";

/*
 * Archive — the library catalog, or a channel strip you can mute.
 *
 * Won the 2026-07-15 prototype round; both the console and the public tour
 * render THIS. Every attribute is a channel with a meter; picking one narrows
 * the corpus and every other meter re-reads to what it would cost you next.
 *
 * WHERE THE WORK HAPPENS (rewired 2026-07-17). Search, facets, cross-filtering,
 * sort and paging are the SERVER's now — this component renders one server page
 * (`data.rows`), takes its counts from the server's cross-filtered facet menu
 * (`data.facets`) and its depth from `data.total`, and drives all of it through
 * the URL (a filtered view is shareable and survives refresh). It never holds
 * the corpus. The one corpus-wide thing it does hold is the tiny as-of SKELETON
 * (`data.skeleton` — id + validity window only), and that is deliberate: it is
 * what lets the time scrubber stay instant. The playhead and the "true then"
 * count are computed CLIENT-side from the skeleton as you drag; on release the
 * URL gets `?as_of=` and the visible page re-queries server-side for the rows
 * that were true then. Instant scrub, honest page.
 *
 * The Memory column shows the TITLE, with the claim underneath; a titleless row
 * (title is nullable forever) falls back to its content, rendered as the claim
 * it is. Status and Kind are labelled glyphs. The inspector (L2) is unchanged.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, useReducedMotion } from "framer-motion";
import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";

import {
  band,
  FONT_DISPLAY,
  FONT_MONO,
  INK_DIM as DIM,
  INK_FAINT as FAINT,
  LABEL,
} from "@/design/theme";

import {
  fmtDate,
  timeBounds,
  validAt,
  FACET_KEYS,
  type ArchiveData,
  type ArchiveFacet,
  type FacetKey,
} from "./archive-data";
import { rowView } from "./archive-index";
import ArchiveSearch from "./ArchiveSearch";
import ColumnFilter from "./ColumnFilter";
import MemoryInspector from "./MemoryInspector";
import { KindIcon, StatusIcon } from "./row-icons";
import { useMemoryDetail } from "./useMemoryDetail";

const VIOLET = band("delta");
const VIOLET_GLOW = band("delta", 60, 0.35);
const SEL_EDGE = band("delta", 60, 0.55);
const SEL_FILL = band("delta", 60, 0.09);
const METER_BED = "rgba(233,237,255,0.08)";
const METER_INK = "rgba(233,237,255,0.28)";
const INK_BODY = "rgba(233,237,255,0.8)";

// Tailwind arbitrary values can't read a JS constant, and the accent/focus hue
// must still come from theme.ts rather than a hardcoded twin of it.
const VARS = { "--vio": VIOLET } as CSSProperties;

const DAY_MS = 86_400_000;
/** Long enough to swallow a burst of typing/scrubbing, short enough to feel live. */
const DEBOUNCE_MS = 220;

const CHIP = `${FONT_MONO} shrink-0 rounded-full border px-2.5 py-0.5 text-[11px] uppercase tracking-[0.14em] transition`;

/*
 * status · kind · memory · team · valid — the two glyph columns sized by their
 * own filter headers, `valid` sized to hold a whole span.
 */
const COLS = "md:grid-cols-[80px_68px_minmax(0,1fr)_120px_212px]";

/** The rail's shelves, in order, paired with the facet menu they read from. */
const RAIL: { key: FacetKey; label: string; menu: keyof ArchiveData["facets"] }[] = [
  { key: "team", label: "team", menu: "teams" },
  // Beside team, not replacing it: team answers WHO wrote it, project WHAT
  // it is about. The org-shared shelf ("none") is selectable — a tier, not
  // an absence (PROJECT-PLAN PR2).
  { key: "project", label: "project", menu: "projects" },
  { key: "kind", label: "kind", menu: "kinds" },
  { key: "status", label: "status", menu: "statuses" },
  { key: "visibility", label: "visibility", menu: "visibilities" },
];

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

export default function Archive({
  data,
  demo = false,
}: {
  data: ArchiveData;
  /** The /demo tour renders this with fixture data and no console route under
   *  it, so URL-driven navigation would walk the tour away. In demo mode the
   *  server-round-trip controls are disabled and the as-of scrubber filters the
   *  fixture client-side; selecting a row still opens its record. */
  demo?: boolean;
}) {
  const reduce = !!useReducedMotion();
  const router = useRouter();
  const searchParams = useSearchParams();

  const { live, total, facets, rows, skeleton, filter, page, pageSize, sort, dir, asOf } = data;

  // The time axis, from the skeleton (never Date.now() — the "now" edge is the
  // last instant the corpus knows about, stable across server render/hydration).
  const { min, max } = useMemo(() => timeBounds(skeleton), [skeleton]);
  const minMs = min.getTime();
  const spanMs = max.getTime() - minMs + DAY_MS;

  // The committed as-of, as a fraction of the axis. `frac` is the live playhead
  // during a drag; it resyncs to the committed value once the page re-queries.
  const committedFrac = asOf ? clamp01((new Date(asOf).getTime() - minMs) / spanMs) : 1;
  const [frac, setFrac] = useState(committedFrac);
  useEffect(() => setFrac(committedFrac), [committedFrac]);

  const atMs = minMs + spanMs * frac;
  const atDate = useMemo(() => new Date(atMs), [atMs]);
  const atNow = frac >= 0.999;

  // Client-side, instant: how many of the visible corpus were true at the
  // playhead. Recomputed on every scrub tick — the skeleton is tiny by design.
  const trueThen = useMemo(
    () => skeleton.reduce((n, r) => n + (validAt(r, atDate) ? 1 : 0), 0),
    [skeleton, atDate],
  );
  const notTrueThen = skeleton.length - trueThen;

  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading, error } = useMemoryDetail(selected, live);

  // ── URL as the single source of truth ─────────────────────────────────
  const setParams = useCallback(
    (mutate: (p: URLSearchParams) => void, resetPage = true) => {
      const p = new URLSearchParams(searchParams?.toString() ?? "");
      p.set("m", "memories");
      mutate(p);
      if (resetPage) p.delete("page");
      router.push(`/console?${p.toString()}`, { scroll: false });
    },
    [router, searchParams],
  );

  const toggleFacet = (key: FacetKey, value: string) =>
    setParams((p) => {
      if (p.get(key) === value) p.delete(key);
      else p.set(key, value);
    });
  const clearFacet = (key: FacetKey) => setParams((p) => p.delete(key));

  const gotoPage = (n: number) =>
    setParams((p) => {
      if (n <= 0) p.delete("page");
      else p.set("page", String(n));
    }, false);

  // ── search: local input, debounced write to ?q ────────────────────────
  const [qInput, setQInput] = useState(filter.q ?? "");
  useEffect(() => setQInput(filter.q ?? ""), [filter.q]);
  useEffect(() => {
    if (demo) return;
    const v = qInput.trim();
    if (v === (filter.q ?? "")) return;
    const t = setTimeout(
      () => setParams((p) => (v ? p.set("q", v) : p.delete("q"))),
      DEBOUNCE_MS,
    );
    return () => clearTimeout(t);
  }, [qInput, filter.q, demo, setParams]);

  // ── sort: the Valid header cycles recent → valid_from asc → desc ───────
  const sortActive = sort === "valid_from";
  const SortIcon = !sortActive ? ArrowUpDown : dir === "asc" ? ArrowUp : ArrowDown;
  const cycleSort = () =>
    setParams((p) => {
      if (!sortActive) {
        p.set("sort", "valid_from");
        p.set("dir", "asc");
      } else if (dir === "asc") {
        p.set("sort", "valid_from");
        p.set("dir", "desc");
      } else {
        p.delete("sort");
        p.delete("dir");
      }
    });

  // ── as-of: instant client scrub, debounced server commit on release ────
  const commitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (commitTimer.current) clearTimeout(commitTimer.current);
  }, []);

  const onScrub = (v: number) => {
    setFrac(v);
    if (demo) return; // the fixture filters client-side below — no navigation
    if (commitTimer.current) clearTimeout(commitTimer.current);
    const isNow = v >= 0.999;
    const iso = new Date(minMs + spanMs * v).toISOString();
    commitTimer.current = setTimeout(
      () => setParams((p) => (isNow ? p.delete("as_of") : p.set("as_of", iso))),
      DEBOUNCE_MS,
    );
  };

  const toNow = () => {
    setFrac(1);
    if (demo) return;
    if (commitTimer.current) clearTimeout(commitTimer.current);
    setParams((p) => p.delete("as_of"));
  };

  // ── the visible page ───────────────────────────────────────────────────
  // Live: the server already applied as_of — render as-is. Demo: no server, so
  // scrub filters the fixture page (which is the whole tiny corpus) client-side.
  const shownRows = useMemo(
    () => (demo ? rows.filter((r) => validAt(r, atDate)) : rows),
    [demo, rows, atDate],
  );
  const matchedTotal = demo ? shownRows.length : total;
  const pages = Math.max(1, Math.ceil(matchedTotal / pageSize));
  const start = demo ? 0 : page * pageSize;

  const activeCount =
    FACET_KEYS.reduce((n, k) => n + (filter[k] ? 1 : 0), 0) + (filter.q ? 1 : 0);
  const clearAll = () =>
    setParams((p) => {
      [...FACET_KEYS, "q"].forEach((k) => p.delete(k));
    });

  return (
    <div className="mx-auto max-w-[1560px] px-6 py-6" style={VARS}>
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: VIOLET }}>
            δ · archive · catalog
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            <span style={{ color: VIOLET, textShadow: `0 0 28px ${VIOLET_GLOW}` }}>
              {matchedTotal}
            </span>{" "}
            of {skeleton.length} memories match
          </h1>
        </div>
        <div className={`${FONT_MONO} text-xs`} style={{ color: FAINT }}>
          as of {fmtDate(atDate.toISOString())} · {trueThen} true then
          {notTrueThen > 0 && <> · {notTrueThen} not true then</>}
          {!live && " · demo data"}
        </div>
      </div>

      <div className="mt-5 grid gap-5 lg:grid-cols-[212px_minmax(0,1fr)] xl:grid-cols-[212px_minmax(0,1fr)_384px]">
        {/* the channel strip */}
        <motion.aside
          initial={reduce ? false : { opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3 }}
          className="space-y-4 lg:sticky lg:top-4 lg:self-start"
        >
          {RAIL.map((f) => (
            <FacetBlock
              key={f.key}
              label={f.label}
              options={facets[f.menu]}
              active={filter[f.key]}
              disabled={demo}
              onToggle={(v) => toggleFacet(f.key, v)}
            />
          ))}

          <button
            type="button"
            onClick={clearAll}
            disabled={activeCount === 0 || demo}
            className={`${FONT_MONO} w-full rounded-full border px-3 py-1 text-[11px] uppercase tracking-[0.14em] transition ${
              activeCount === 0
                ? "border-transparent"
                : "border-white/25 text-white hover:border-white/60"
            }`}
            style={activeCount === 0 ? { color: FAINT } : undefined}
          >
            {activeCount === 0 ? "no filters on" : `clear ${activeCount} filter${activeCount > 1 ? "s" : ""}`}
          </button>
        </motion.aside>

        {/* the catalog itself */}
        <div className="min-w-0">
          {!demo && (
            <ArchiveSearch
              value={qInput}
              onChange={setQInput}
              matched={matchedTotal}
              scope={skeleton.length}
            />
          )}

          {/* as-of — the fifth filter, sized like one */}
          <div className="mt-3 rounded-xl border border-white/10 bg-white/[0.015] px-4 py-2.5">
            <div className="flex items-center gap-3">
              <span className={`${LABEL} shrink-0`} style={{ color: FAINT }}>
                as of
              </span>
              <span className={`${FONT_MONO} shrink-0 text-sm tabular-nums`} style={{ color: VIOLET }}>
                {fmtDate(atDate.toISOString())}
              </span>
              <input
                type="range"
                min={0}
                max={1000}
                value={Math.round(frac * 1000)}
                onChange={(e) => onScrub(Number(e.target.value) / 1000)}
                aria-label="As-of date"
                className="h-1 min-w-0 flex-1 accent-[var(--vio)]"
              />
              <button
                type="button"
                onClick={toNow}
                disabled={atNow}
                className={`${CHIP} ${atNow ? "border-transparent" : "border-white/25 text-white hover:border-white/60"}`}
                style={atNow ? { color: FAINT } : undefined}
              >
                {atNow ? "● now" : "→ now"}
              </button>
            </div>
          </div>

          {/* Not overflow-hidden: the header's filter menus have to escape it. */}
          <div className="mt-3 rounded-xl border border-white/10 bg-white/[0.015]">
            <div
              className={`grid grid-cols-[minmax(0,1fr)] items-center gap-x-3 border-b border-white/10 px-3 py-2 ${COLS}`}
            >
              <div className="hidden md:block">
                {demo ? (
                  <span className={LABEL} style={{ color: FAINT }}>status</span>
                ) : (
                  <ColumnFilter
                    label="status"
                    glyphs="status"
                    options={facets.statuses}
                    active={filter.status}
                    onToggle={(v) => toggleFacet("status", v)}
                    onClear={() => clearFacet("status")}
                  />
                )}
              </div>
              <div className="hidden md:block">
                {demo ? (
                  <span className={LABEL} style={{ color: FAINT }}>kind</span>
                ) : (
                  <ColumnFilter
                    label="kind"
                    glyphs="kind"
                    options={facets.kinds}
                    active={filter.kind}
                    onToggle={(v) => toggleFacet("kind", v)}
                    onClear={() => clearFacet("kind")}
                  />
                )}
              </div>
              <span className={LABEL} style={{ color: FAINT }}>
                memory
              </span>
              <div className="hidden md:block">
                {demo ? (
                  <span className={LABEL} style={{ color: FAINT }}>team</span>
                ) : (
                  <ColumnFilter
                    label="team"
                    options={facets.teams}
                    active={filter.team}
                    onToggle={(v) => toggleFacet("team", v)}
                    onClear={() => clearFacet("team")}
                  />
                )}
              </div>
              <div className="hidden md:block">
                {demo ? (
                  <span className={LABEL} style={{ color: FAINT }}>valid</span>
                ) : (
                  <button
                    type="button"
                    onClick={cycleSort}
                    aria-pressed={sortActive}
                    data-sort={sortActive ? dir : "off"}
                    aria-label={`Sort by validity — currently ${
                      !sortActive ? "unsorted" : dir === "asc" ? "oldest first" : "newest first"
                    }. Click to ${!sortActive ? "sort oldest first" : dir === "asc" ? "sort newest first" : "clear the sort"}.`}
                    className={`${LABEL} flex items-center gap-1 rounded px-1 py-0.5 transition hover:text-white`}
                    style={{ color: sortActive ? VIOLET : FAINT }}
                  >
                    <span>valid</span>
                    <SortIcon size={11} strokeWidth={2} aria-hidden className="shrink-0 opacity-70" />
                  </button>
                )}
              </div>
            </div>

            <div className="max-h-[58vh] overflow-y-auto">
              <motion.ul
                initial={reduce ? false : { opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.24 }}
                className="divide-y divide-white/[0.05]"
              >
                {shownRows.map((r) => {
                  const v = rowView(r);
                  const on = selected === r.id;
                  return (
                    <li key={r.id}>
                      <button
                        type="button"
                        onClick={() => setSelected(on ? null : r.id)}
                        className={`grid w-full grid-cols-[minmax(0,1fr)] gap-x-3 px-3 py-2 text-left transition hover:bg-white/[0.03] md:items-center ${COLS}`}
                        style={on ? { background: SEL_FILL } : undefined}
                      >
                        <span className="hidden md:block">
                          <StatusIcon status={r.status} />
                        </span>
                        <span className="hidden md:block">
                          <KindIcon kind={r.kind} />
                        </span>
                        {/* The label, then the claim. A title is a LABEL and the
                            content is the TRUTH; a titleless row shows the
                            excerpt in italic — a claim standing in, not a broken
                            label. */}
                        <span className="min-w-0" title={r.content}>
                          <span
                            className={`${FONT_MONO} block truncate text-sm ${v.titled ? "" : "italic"}`}
                            style={{ color: on ? "#fff" : v.titled ? INK_BODY : DIM }}
                          >
                            {v.label}
                          </span>
                          {v.sub && (
                            <span className={`${FONT_MONO} block truncate text-sm`} style={{ color: FAINT }}>
                              {v.sub}
                            </span>
                          )}
                        </span>
                        <span
                          className={`${FONT_MONO} hidden truncate text-sm md:block`}
                          style={{ color: DIM }}
                        >
                          {r.team}
                        </span>
                        <span
                          className={`${FONT_MONO} hidden truncate text-sm tabular-nums md:block`}
                          style={{ color: FAINT }}
                        >
                          {v.span}
                        </span>
                      </button>
                    </li>
                  );
                })}
              </motion.ul>

              {shownRows.length === 0 && (
                <p className={`${FONT_MONO} px-3 py-12 text-center text-sm`} style={{ color: FAINT }}>
                  {skeleton.length === 0
                    ? "the archive is empty"
                    : trueThen === 0
                      ? "nothing was known yet — scrub forward"
                      : "no memories match — clear a filter, or widen the search"}
                </p>
              )}
            </div>

            {/* showing N of M, and the pager */}
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-white/10 px-3 py-2">
              <span className={`${FONT_MONO} text-xs`} style={{ color: DIM }}>
                {matchedTotal === 0 ? (
                  <>showing 0 of 0 matching · {skeleton.length} in the archive</>
                ) : (
                  <>
                    showing {start + 1}–{start + shownRows.length} of {matchedTotal} matching ·{" "}
                    {skeleton.length} in the archive
                  </>
                )}
              </span>
              {!demo && pages > 1 && (
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => gotoPage(page - 1)}
                    disabled={page <= 0}
                    className={`${CHIP} ${page <= 0 ? "border-transparent" : "border-white/25 text-white hover:border-white/60"}`}
                    style={page <= 0 ? { color: FAINT } : undefined}
                  >
                    ← prev
                  </button>
                  <span className={`${FONT_MONO} text-[11px] uppercase tracking-[0.14em]`} style={{ color: FAINT }}>
                    page {page + 1} / {pages}
                  </span>
                  <button
                    type="button"
                    onClick={() => gotoPage(page + 1)}
                    disabled={page >= pages - 1}
                    className={`${CHIP} ${page >= pages - 1 ? "border-transparent" : "border-white/25 text-white hover:border-white/60"}`}
                    style={page >= pages - 1 ? { color: FAINT } : undefined}
                  >
                    next →
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* the record */}
        <div className="lg:col-span-2 xl:sticky xl:top-4 xl:col-span-1 xl:self-start">
          <div className="min-h-[300px] rounded-xl border border-white/10 bg-white/[0.015] p-5">
            {!selected && (
              <p className={`${FONT_MONO} py-12 text-center text-sm`} style={{ color: FAINT }}>
                select a memory — its lineage, provenance and ledger open here
              </p>
            )}
            {selected && loading && (
              <p className={`${FONT_MONO} text-sm`} style={{ color: DIM }}>
                opening the record…
              </p>
            )}
            {selected && error && (
              <p className={`${FONT_MONO} text-sm`} style={{ color: band("gamma") }}>
                {error}
              </p>
            )}
            {selected && detail && <MemoryInspector detail={detail} onHop={setSelected} />}
          </div>
        </div>
      </div>
    </div>
  );
}

/** One channel: every value on the shelf, its meter, and what it costs. */
function FacetBlock({
  label,
  options,
  active,
  disabled,
  onToggle,
}: {
  label: string;
  options: ArchiveFacet[];
  active: string | undefined;
  disabled?: boolean;
  onToggle: (v: string) => void;
}) {
  const peak = Math.max(1, ...options.map((o) => o.count));

  return (
    <div>
      <div className="flex items-baseline justify-between gap-2">
        <span className={LABEL} style={{ color: FAINT }}>
          {label}
        </span>
        {active !== undefined && (
          <span className={`${FONT_MONO} text-[11px] uppercase tracking-[0.14em]`} style={{ color: VIOLET }}>
            1 on
          </span>
        )}
      </div>
      <ul className="mt-1.5 space-y-0.5">
        {options.length === 0 && (
          <li className={`${FONT_MONO} text-sm`} style={{ color: FAINT }}>
            no values
          </li>
        )}
        {options.map((o) => {
          const on = active === o.value;
          return (
            <li key={o.value}>
              <button
                type="button"
                onClick={() => onToggle(o.value)}
                disabled={disabled}
                aria-pressed={on}
                className={`flex w-full items-center gap-2 rounded-md border px-2 py-1 text-left transition ${
                  disabled
                    ? "cursor-default border-transparent"
                    : on
                      ? ""
                      : "border-transparent hover:border-white/15 hover:bg-white/[0.03]"
                }`}
                style={on ? { borderColor: SEL_EDGE, background: SEL_FILL } : undefined}
              >
                <span
                  className={`${FONT_MONO} min-w-0 flex-1 truncate text-sm`}
                  style={{ color: on ? "#fff" : DIM }}
                >
                  {o.label}
                </span>
                {/* the count made physical — a meter reads faster than a number */}
                <span
                  aria-hidden
                  className="h-1 w-7 shrink-0 overflow-hidden rounded-full"
                  style={{ background: METER_BED }}
                >
                  <span
                    className="block h-full rounded-full"
                    style={{ width: `${(o.count / peak) * 100}%`, background: on ? VIOLET : METER_INK }}
                  />
                </span>
                <span
                  className={`${FONT_MONO} w-7 shrink-0 text-right text-[11px] uppercase tracking-[0.1em] tabular-nums`}
                  style={{ color: o.count === 0 ? FAINT : DIM }}
                >
                  {o.count}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
