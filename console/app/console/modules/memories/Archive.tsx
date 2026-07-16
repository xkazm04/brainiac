"use client";

/*
 * Archive — the library catalog, or a channel strip you can mute.
 *
 * Won the 2026-07-15 prototype round over "strata" (the corpus as sediment over
 * time), and replaced the flat scrubber list it was measured against. Both the
 * console and the public tour render THIS.
 *
 * The old archive was a story with no index: one hero scrubber over a flat
 * list. That read beautifully at sixteen rows and was unusable at five hundred,
 * because there is no way to ASK it anything — no search, no attributes, no
 * route to a memory you already know exists. This variant inverts the emphasis.
 * Every attribute is a channel with a meter; opening one narrows the corpus and
 * every other meter re-reads to what it would cost you next. Counts that ignore
 * the other active filters are worse than no counts — they promise 88 results
 * and hand you three.
 *
 * Time travel is kept but demoted from hero to instrument: when you are hunting
 * a known memory the question is "which one", not "when", so the as-of control
 * sits with the other filters and behaves like one.
 *
 * What the 2026-07-15 rebuild changed, at org scale (660 memories, 12 teams):
 *
 *  - The Memory column shows the TITLE — a label you can scan — and keeps the
 *    claim underneath it. `memories.title` is nullable forever, so a titleless
 *    row falls back to its content, rendered as the claim it is rather than as
 *    a broken label (see memoryLabel).
 *  - Status and Kind became glyphs. They are four-value enums; as text they
 *    were spending 176px on words the Memory column needed.
 *  - Every enum header is its own filter, sharing one selection with the rail.
 *  - The rail's search moved above the table and grew suggestions.
 *  - The fetch pages (Module.tsx). It used to stop at 200 and say "of 200"
 *    as though it meant "of the archive".
 *
 * The table draws one page and says so in the same breath. The bug being
 * designed out was never the cap — a cap is honest engineering. It was the
 * silence around the cap.
 */

import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { motion, useReducedMotion } from "framer-motion";
import { ArrowDown, ArrowUp, ArrowUpDown, TriangleAlert } from "lucide-react";

import {
  band,
  FONT_DISPLAY,
  FONT_MONO,
  INK_DIM as DIM,
  INK_FAINT as FAINT,
  LABEL,
  withAlpha,
} from "@/design/theme";
import type { MemoryRow } from "@/lib/types";

import { fmtDate, timeBounds, type ArchiveData } from "./archive-data";
import {
  buildScope,
  indexRows,
  liveAt,
  sortByValid,
  suggest,
  type Indexed,
  type SortDir,
  type Suggestion,
} from "./archive-index";
import ArchiveSearch from "./ArchiveSearch";
import ColumnFilter from "./ColumnFilter";
import MemoryInspector from "./MemoryInspector";
import { KindIcon, StatusIcon } from "./row-icons";
import { useMemoryDetail } from "./useMemoryDetail";

const VIOLET = band("delta");
const VIOLET_GLOW = band("delta", 60, 0.35);
const SEL_EDGE = band("delta", 60, 0.55);
const SEL_FILL = band("delta", 60, 0.09);
const GOLD = band("gamma");
const METER_BED = "rgba(233,237,255,0.08)";
const METER_INK = "rgba(233,237,255,0.28)";
const INK_BODY = "rgba(233,237,255,0.8)";

// Tailwind arbitrary values can't read a JS constant, and the accent/focus hues
// must still come from theme.ts rather than a hardcoded twin of it.
const VARS = { "--vio": VIOLET } as CSSProperties;

/** One screenful of catalog. The DOM never holds more rows than this — ever. */
const PAGE = 80;

/** Long enough to swallow a burst of typing, short enough to feel live. */
const DEBOUNCE_MS = 120;

const FACETS = [
  { key: "team", label: "team" },
  { key: "kind", label: "kind" },
  // `rejected` is a value on this shelf rather than a silent exclusion the way
  // the flat list drops it: a catalog that hides a shelf is a broken catalog.
  { key: "status", label: "status" },
  { key: "visibility", label: "visibility" },
] as const;

type FacetKey = (typeof FACETS)[number]["key"];
const FACET_KEYS: FacetKey[] = FACETS.map((f) => f.key);

const CHIP = `${FONT_MONO} shrink-0 rounded-full border px-2.5 py-0.5 text-[11px] uppercase tracking-[0.14em] transition`;

/*
 * status · kind · memory · team · valid.
 *
 * The two glyph columns are sized by their own headers — which are filter
 * buttons, so they need the word and a chevron — not by the 15px icons. `valid`
 * is sized to hold a whole span ("2026-01-30 → 2028-01-06") because a validity
 * window truncated to "2026-01-30 → 20…" is not a date, it is a rumour.
 */
const COLS = "md:grid-cols-[80px_68px_minmax(0,1fr)_96px_212px]";

const SORT_NEXT: Record<SortDir, SortDir> = { off: "asc", asc: "desc", desc: "off" };

export default function Archive({ data }: { data: ArchiveData }) {
  const reduce = !!useReducedMotion();
  const { min, max } = useMemo(() => timeBounds(data.rows), [data.rows]);

  // The corpus parsed, labelled and term-extracted exactly once. Everything
  // downstream is number compares and pre-rendered strings.
  const index = useMemo<Indexed[]>(() => indexRows(data.rows), [data.rows]);

  const [frac, setFrac] = useState(1); // 1 = the latest instant on record
  const [q, setQ] = useState("");
  // The debounced twin actually drives the filter. `q` is what you see in the
  // box; `dq` is what the corpus is asked, at most once per DEBOUNCE_MS.
  const [dq, setDq] = useState("");
  const [sel, setSel] = useState<Record<FacetKey, string[]>>({
    team: [],
    kind: [],
    status: [],
    visibility: [],
  });
  const [sort, setSort] = useState<SortDir>("off");
  const [selected, setSelected] = useState<string | null>(null);
  const { detail, loading, error } = useMemoryDetail(selected, data.live);

  useEffect(() => {
    const t = setTimeout(() => setDq(q), DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [q]);

  // "Now" is the last instant the corpus knows about — never Date.now(), which
  // would disagree with itself across the server render and hydration.
  const atMs = min.getTime() + (max.getTime() - min.getTime() + 86_400_000) * frac;
  const atNow = frac >= 0.999;

  // One as-of pass; every count on the page is built from it.
  const asOf = useMemo(() => index.filter((ix) => liveAt(ix, atMs)), [index, atMs]);

  const sets = useMemo(
    () =>
      ({
        team: new Set(sel.team),
        kind: new Set(sel.kind),
        status: new Set(sel.status),
        visibility: new Set(sel.visibility),
      }) as Record<FacetKey, Set<string>>,
    [sel],
  );

  const passes = (row: MemoryRow, except: FacetKey | null) =>
    FACET_KEYS.every((k) => k === except || sets[k].size === 0 || sets[k].has(row[k]));

  const query = dq.trim().toLowerCase();
  const base = useMemo(
    () => (query ? asOf.filter((ix) => ix.hay.includes(query)) : asOf),
    [asOf, query],
  );

  // Facet values in a stable order, taken from the whole corpus so a shelf never
  // vanishes mid-narrowing — it just reads zero and goes quiet.
  const domains = useMemo(() => {
    const out = {} as Record<FacetKey, string[]>;
    for (const k of FACET_KEYS) {
      const tally = new Map<string, number>();
      for (const ix of index) tally.set(ix.row[k], (tally.get(ix.row[k]) ?? 0) + 1);
      out[k] = [...tally.entries()]
        .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
        .map(([v]) => v);
    }
    return out;
  }, [index]);

  // Cross-filtered: a shelf's count is what you'd get by clicking it, so it is
  // measured against every OTHER facet but never against itself. The header
  // menus and the rail read the same map — one selection, one set of counts.
  const counts = useMemo(() => {
    const out = {} as Record<FacetKey, Map<string, number>>;
    for (const k of FACET_KEYS) {
      const m = new Map<string, number>();
      for (const ix of base) {
        if (!passes(ix.row, k)) continue;
        m.set(ix.row[k], (m.get(ix.row[k]) ?? 0) + 1);
      }
      out[k] = m;
    }
    return out;
  }, [base, sets]);

  const matched = useMemo(() => base.filter((ix) => passes(ix.row, null)), [base, sets]);
  const view = useMemo(() => sortByValid(matched, sort), [matched, sort]);

  /*
   * What the suggestions are measured against: the as-of + facet scope, WITHOUT
   * the query. Excluding the query is what keeps this off the keystroke path —
   * the tally only rebuilds when the scrubber or a facet moves — and it is also
   * the honest scope, since a suggestion's job is to answer "where would this
   * take me from here", not "how big is the archive".
   */
  const scoped = useMemo(() => asOf.filter((ix) => passes(ix.row, null)), [asOf, sets]);
  const scope = useMemo(() => buildScope(scoped), [scoped]);
  const suggestions = useMemo(() => suggest(scope, dq), [scope, dq]);

  const resurrected = matched.filter((ix) => ix.row.status === "deprecated").length;
  const activeCount = FACET_KEYS.reduce((n, k) => n + sel[k].length, 0) + (query ? 1 : 0);

  const filterKey = `${query}|${FACET_KEYS.map((k) => sel[k].join(",")).join("|")}`;
  const [pager, setPager] = useState({ key: filterKey, offset: 0 });
  // A narrowed question deserves answering from the top, not from 240 rows into
  // the last one. React's documented adjust-during-render path — an effect here
  // would paint the stale page first and then flinch.
  if (pager.key !== filterKey) setPager({ key: filterKey, offset: 0 });

  // Scrubbing shrinks `matched` without touching filterKey, so the offset is
  // clamped on read rather than stored back — no state churn per scrub tick.
  const lastPageStart = view.length === 0 ? 0 : Math.floor((view.length - 1) / PAGE) * PAGE;
  const start = Math.min(pager.key === filterKey ? pager.offset : 0, lastPageStart);
  const shown = useMemo(() => view.slice(start, start + PAGE), [view, start]);

  const toggle = (k: FacetKey, v: string) =>
    setSel((s) => ({
      ...s,
      [k]: s[k].includes(v) ? s[k].filter((x) => x !== v) : [...s[k], v],
    }));

  const clearFacet = (k: FacetKey) => setSel((s) => ({ ...s, [k]: [] }));

  const setQuery = (v: string) => {
    setQ(v);
    // Picking a suggestion is a decision, not typing — it should not wait out
    // the debounce it never triggered.
    if (v === "") setDq("");
  };

  const pick = (s: Suggestion) => {
    if (s.kind === "memory") {
      setSelected(s.value);
      return;
    }
    if (s.kind === "term") {
      setQ(s.value);
      setDq(s.value);
      return;
    }
    // A facet suggestion answers the query by other means: the text that found
    // it would now filter for the same thing twice, so it goes.
    if (!sel[s.kind].includes(s.value)) toggle(s.kind, s.value);
    setQ("");
    setDq("");
  };

  const clearAll = () => {
    setSel({ team: [], kind: [], status: [], visibility: [] });
    setQ("");
    setDq("");
  };

  const loaded = index.length;
  const cycleSort = () => setSort((s) => SORT_NEXT[s]);
  const SortIcon = sort === "asc" ? ArrowUp : sort === "desc" ? ArrowDown : ArrowUpDown;

  return (
    <div className="mx-auto max-w-[1560px] px-6 py-6" style={VARS}>
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: VIOLET }}>
            δ · archive · catalog
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            <span style={{ color: VIOLET, textShadow: `0 0 28px ${VIOLET_GLOW}` }}>
              {matched.length}
            </span>{" "}
            of {loaded} memories match
          </h1>
        </div>
        <div className={`${FONT_MONO} text-xs`} style={{ color: FAINT }}>
          as of {fmtDate(new Date(atMs).toISOString())}
          {loaded - asOf.length > 0 && <> · {loaded - asOf.length} not true then</>}
          {resurrected > 0 && <> · {resurrected} since superseded</>}
          {!data.live && " · demo data"}
        </div>
      </div>

      {/*
       * The fetch cap, out loud. Module.tsx pages the corpus until it is
       * exhausted, so this is normally absent — but if a corpus ever outgrows
       * MAX_ROWS, every count above is a count of a prefix, and the page says so
       * rather than letting the number pass for the whole org.
       */}
      {data.capped && (
        <div
          className="mt-3 flex items-center gap-2 rounded-lg border px-3 py-2"
          style={{ borderColor: withAlpha(GOLD, 0.35), background: withAlpha(GOLD, 0.07) }}
        >
          <TriangleAlert size={15} strokeWidth={1.75} color={GOLD} aria-hidden className="shrink-0" />
          <p className={`${FONT_MONO} text-sm`} style={{ color: GOLD }}>
            fetch capped: loaded the {loaded.toLocaleString()} most recent of{" "}
            {data.total.toLocaleString()} memories. every count on this page is of those{" "}
            {loaded.toLocaleString()} — not of the archive.
          </p>
        </div>
      )}

      <div className="mt-5 grid gap-5 lg:grid-cols-[212px_minmax(0,1fr)] xl:grid-cols-[212px_minmax(0,1fr)_384px]">
        {/* the channel strip */}
        <motion.aside
          initial={reduce ? false : { opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3 }}
          className="space-y-4 lg:sticky lg:top-4 lg:self-start"
        >
          {FACETS.map((f) => (
            <FacetBlock
              key={f.key}
              label={f.label}
              values={domains[f.key]}
              counts={counts[f.key]}
              chosen={sel[f.key]}
              onToggle={(v) => toggle(f.key, v)}
            />
          ))}

          <button
            type="button"
            onClick={clearAll}
            disabled={activeCount === 0}
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
          <ArchiveSearch
            value={q}
            onChange={setQuery}
            suggestions={suggestions}
            onPick={pick}
            matched={matched.length}
            scope={scoped.length}
          />

          {/* as-of — the fifth filter, sized like one */}
          <div className="mt-3 rounded-xl border border-white/10 bg-white/[0.015] px-4 py-2.5">
            <div className="flex items-center gap-3">
              <span className={`${LABEL} shrink-0`} style={{ color: FAINT }}>
                as of
              </span>
              <span className={`${FONT_MONO} shrink-0 text-sm tabular-nums`} style={{ color: VIOLET }}>
                {fmtDate(new Date(atMs).toISOString())}
              </span>
              <input
                type="range"
                min={0}
                max={1000}
                value={Math.round(frac * 1000)}
                onChange={(e) => setFrac(Number(e.target.value) / 1000)}
                aria-label="As-of date"
                className="h-1 min-w-0 flex-1 accent-[var(--vio)]"
              />
              <button
                type="button"
                onClick={() => setFrac(1)}
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
                <ColumnFilter
                  label="status"
                  glyphs="status"
                  values={domains.status}
                  counts={counts.status}
                  chosen={sel.status}
                  onToggle={(v) => toggle("status", v)}
                  onClear={() => clearFacet("status")}
                />
              </div>
              <div className="hidden md:block">
                <ColumnFilter
                  label="kind"
                  glyphs="kind"
                  values={domains.kind}
                  counts={counts.kind}
                  chosen={sel.kind}
                  onToggle={(v) => toggle("kind", v)}
                  onClear={() => clearFacet("kind")}
                />
              </div>
              <span className={LABEL} style={{ color: FAINT }}>
                memory
              </span>
              <div className="hidden md:block">
                <ColumnFilter
                  label="team"
                  values={domains.team}
                  counts={counts.team}
                  chosen={sel.team}
                  onToggle={(v) => toggle("team", v)}
                  onClear={() => clearFacet("team")}
                />
              </div>
              <div className="hidden md:block">
                <button
                  type="button"
                  onClick={cycleSort}
                  aria-pressed={sort !== "off"}
                  data-sort={sort}
                  aria-label={`Sort by validity — currently ${
                    sort === "off" ? "unsorted" : sort === "asc" ? "oldest first" : "newest first"
                  }. Click to ${sort === "off" ? "sort oldest first" : sort === "asc" ? "sort newest first" : "clear the sort"}.`}
                  className={`${LABEL} flex items-center gap-1 rounded px-1 py-0.5 transition hover:text-white`}
                  style={{ color: sort === "off" ? FAINT : VIOLET }}
                >
                  <span>valid</span>
                  <SortIcon size={11} strokeWidth={2} aria-hidden className="shrink-0 opacity-70" />
                </button>
              </div>
            </div>

            <div className="max-h-[58vh] overflow-y-auto">
              <motion.ul
                initial={reduce ? false : { opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.24 }}
                className="divide-y divide-white/[0.05]"
              >
                {shown.map((ix) => {
                  const r = ix.row;
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
                        {/*
                         * The label, then the claim. A title is a LABEL and the
                         * content is the TRUTH, so the truth stays on the row —
                         * one line of it — and the record has all of it.
                         *
                         * When there is no title the excerpt takes the top line
                         * in italic: it is not a label that failed to load, it
                         * is the claim standing in for one.
                         */}
                        <span className="min-w-0" title={r.content}>
                          <span
                            className={`${FONT_MONO} block truncate text-sm ${ix.titled ? "" : "italic"}`}
                            style={{ color: on ? "#fff" : ix.titled ? INK_BODY : DIM }}
                          >
                            {ix.label}
                          </span>
                          {ix.sub && (
                            <span className={`${FONT_MONO} block truncate text-sm`} style={{ color: FAINT }}>
                              {ix.sub}
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
                          {ix.span}
                        </span>
                      </button>
                    </li>
                  );
                })}
              </motion.ul>

              {shown.length === 0 && (
                <p className={`${FONT_MONO} px-3 py-12 text-center text-sm`} style={{ color: FAINT }}>
                  {loaded === 0
                    ? "the archive is empty"
                    : asOf.length === 0
                      ? "nothing was known yet — scrub forward"
                      : "no memories match these facets — clear one, or widen the search"}
                </p>
              )}
            </div>

            {/* the line the shipped list never had */}
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-white/10 px-3 py-2">
              <span className={`${FONT_MONO} text-xs`} style={{ color: DIM }}>
                {view.length === 0 ? (
                  <>showing 0 of 0 matching · {loaded} in the archive</>
                ) : (
                  <>
                    showing {start + 1}–{start + shown.length} of {view.length} matching ·{" "}
                    {loaded} in the archive
                  </>
                )}
              </span>
              {view.length > PAGE && (
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => setPager({ key: filterKey, offset: Math.max(0, start - PAGE) })}
                    disabled={start === 0}
                    className={`${CHIP} ${start === 0 ? "border-transparent" : "border-white/25 text-white hover:border-white/60"}`}
                    style={start === 0 ? { color: FAINT } : undefined}
                  >
                    ← prev
                  </button>
                  <span className={`${FONT_MONO} text-[11px] uppercase tracking-[0.14em]`} style={{ color: FAINT }}>
                    page {start / PAGE + 1} / {Math.ceil(view.length / PAGE)}
                  </span>
                  <button
                    type="button"
                    onClick={() =>
                      setPager({ key: filterKey, offset: Math.min(lastPageStart, start + PAGE) })
                    }
                    disabled={start >= lastPageStart}
                    className={`${CHIP} ${start >= lastPageStart ? "border-transparent" : "border-white/25 text-white hover:border-white/60"}`}
                    style={start >= lastPageStart ? { color: FAINT } : undefined}
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
  values,
  counts,
  chosen,
  onToggle,
}: {
  label: string;
  values: string[];
  counts: Map<string, number>;
  chosen: string[];
  onToggle: (v: string) => void;
}) {
  const peak = Math.max(1, ...values.map((v) => counts.get(v) ?? 0));

  return (
    <div>
      <div className="flex items-baseline justify-between gap-2">
        <span className={LABEL} style={{ color: FAINT }}>
          {label}
        </span>
        {chosen.length > 0 && (
          <span className={`${FONT_MONO} text-[11px] uppercase tracking-[0.14em]`} style={{ color: VIOLET }}>
            {chosen.length} on
          </span>
        )}
      </div>
      <ul className="mt-1.5 space-y-0.5">
        {values.length === 0 && (
          <li className={`${FONT_MONO} text-sm`} style={{ color: FAINT }}>
            no values
          </li>
        )}
        {values.map((v) => {
          const n = counts.get(v) ?? 0;
          const on = chosen.includes(v);
          const mute = n === 0 && !on;
          return (
            <li key={v}>
              <button
                type="button"
                onClick={() => onToggle(v)}
                disabled={mute}
                aria-pressed={on}
                className={`flex w-full items-center gap-2 rounded-md border px-2 py-1 text-left transition ${
                  mute
                    ? "cursor-default border-transparent opacity-40"
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
                  {v}
                </span>
                {/* the count made physical — a meter reads faster than a number */}
                <span
                  aria-hidden
                  className="h-1 w-7 shrink-0 overflow-hidden rounded-full"
                  style={{ background: METER_BED }}
                >
                  <span
                    className="block h-full rounded-full"
                    style={{ width: `${(n / peak) * 100}%`, background: on ? VIOLET : METER_INK }}
                  />
                </span>
                <span
                  className={`${FONT_MONO} w-7 shrink-0 text-right text-[11px] uppercase tracking-[0.1em] tabular-nums`}
                  style={{ color: n === 0 ? FAINT : DIM }}
                >
                  {n}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
