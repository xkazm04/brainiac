"use client";

/*
 * Reviews — the triage rail.
 *
 * Metaphor: an editor's clip list. One dense rail of every pending decision,
 * one viewer for the clip under the head, and hands that never leave the
 * keyboard.
 *
 * Won the 2026-07-15 prototype round over "cohorts" (batch-signing), and
 * replaced the flat card queue it was measured against. Both the operator's
 * console and the public tour render THIS — the tour passes inert stamps where
 * the console passes the real controls (app/demo/DemoReviews.tsx).
 *
 * WHY A RAIL AND NOT A LIST. The old queue rendered every promotion at full
 * height, so the maintainer's scroll position IS their place in the queue —
 * there is no way to see the shape of the backlog, only the item in front of
 * you. At four items that is fine. At 480 it means the morning is spent
 * scrolling, and the only reachable strategy is "start at the top and hope".
 * Splitting the surface in two lets the rail answer "what is waiting, and what
 * is on fire" in one screen while the pane answers "what am I actually signing"
 * in full — neither question has to give up space to the other.
 *
 * WHY OLDEST FIRST. Order is a judgement, and the flat queue makes it by
 * accident (insertion order). This one makes it on purpose: the corpus has a
 * long tail past the 48h SLO, and a rail sorted by age puts the breaches where
 * a reviewer starts rather than where they give up.
 *
 * WHY THE KEYSTROKES GO THROUGH onBulk. a/r do not get their own write path —
 * `promotionControls` is the slot that decides whether this surface can mutate
 * anything at all (see ReviewQueue's header), and inventing buttons here to
 * serve a keystroke would route around that decision. So a/r are a selection of
 * one handed to the same bulk channel, and when the host passes no `onBulk`
 * they are honestly dark rather than silently inert.
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import Link from "next/link";
import { motion, useReducedMotion } from "framer-motion";

import {
  band,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  LABEL,
  MAGENTA,
  withAlpha,
} from "@/design/theme";
import { formatAge } from "@/lib/format";
import type { ContradictionStatus, PromotionQueueItem } from "@/lib/governance-api";

import { STATUS_TABS, type ReviewSurfaceProps } from "./review-surface";

const ALPHA = band("alpha");
const DIM = "rgba(233,237,255,0.5)";
const FAINT = "rgba(233,237,255,0.35)";
const HAIR = "rgba(233,237,255,0.12)";

/** The SLO the corpus is lumpy around. Past this, an item is a breach, not work. */
const STALE_SECS = 48 * 3600;
/** The rail never mounts the whole backlog; it mounts a window onto it. */
const RAIL_WINDOW = 60;
const CONTRA_WINDOW = 12;

interface Filters {
  team: string | null;
  kind: string | null;
  rule: string | null;
  stale: boolean;
}

const EMPTY: Filters = { team: null, kind: null, rule: null, stale: false };

/**
 * `ignore` is what makes the chip counts honest: a facet's tally is computed
 * against every OTHER active filter, so a chip reads "what this would leave me
 * with", not "what exists somewhere in the corpus".
 */
const passes = (p: PromotionQueueItem, f: Filters, ignore?: keyof Filters) =>
  (ignore === "team" || f.team === null || p.memory?.team === f.team) &&
  (ignore === "kind" || f.kind === null || p.memory?.kind === f.kind) &&
  (ignore === "rule" || f.rule === null || p.policy_rule === f.rule) &&
  (ignore === "stale" || !f.stale || p.age_secs > STALE_SECS);

function FilterChip({
  label,
  count,
  active,
  tone,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  tone: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`${FONT_MONO} cursor-pointer rounded-full border px-2.5 py-1 text-xs transition hover:bg-white/5`}
      style={
        active
          ? { borderColor: withAlpha(tone, 0.7), background: withAlpha(tone, 0.08), color: tone }
          : { borderColor: HAIR, color: DIM }
      }
    >
      {label} · {count}
    </button>
  );
}

function Key({ children }: { children: ReactNode }) {
  return (
    <kbd
      className={`${FONT_MONO} rounded border px-1 py-px text-[11px] normal-case tracking-normal`}
      style={{ borderColor: HAIR, color: "rgba(233,237,255,0.7)" }}
    >
      {children}
    </kbd>
  );
}

export default function ReviewWorklist({
  promotions,
  contradictions,
  counts,
  cstatus,
  statusHref,
  onStatusChange,
  promotionControls,
  contradictionControls,
  onBulk,
}: ReviewSurfaceProps) {
  const reduced = useReducedMotion();
  const [filters, setFilters] = useState<Filters>(EMPTY);
  const [limit, setLimit] = useState(RAIL_WINDOW);
  const [focus, setFocus] = useState(0);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [clim, setClim] = useState(CONTRA_WINDOW);
  const railRef = useRef<HTMLDivElement>(null);

  // Triage order, decided once: oldest first, id as a stable tie-break so the
  // rail cannot reshuffle under a keystroke.
  const ordered = useMemo(
    () => [...promotions].sort((a, b) => b.age_secs - a.age_secs || a.id.localeCompare(b.id)),
    [promotions],
  );
  const restricted = useMemo(() => ordered.filter((p) => p.memory === null).length, [ordered]);
  const matched = useMemo(() => ordered.filter((p) => passes(p, filters)), [ordered, filters]);

  const facets = useMemo(() => {
    const tally = (dim: "team" | "kind" | "rule") => {
      const m = new Map<string, number>();
      for (const p of ordered) {
        if (!passes(p, filters, dim)) continue;
        const v =
          dim === "team" ? p.memory?.team : dim === "kind" ? p.memory?.kind : p.policy_rule;
        if (!v) continue;
        m.set(v, (m.get(v) ?? 0) + 1);
      }
      return [...m.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    };
    return {
      team: tally("team"),
      kind: tally("kind"),
      rule: tally("rule"),
      stale: ordered.filter((p) => passes(p, filters, "stale") && p.age_secs > STALE_SECS).length,
    };
  }, [ordered, filters]);

  // Focus is an index, clamped at read time — a filter that shrinks the rail
  // under the cursor must not leave the pane pointing at nothing.
  const focusIdx = Math.min(focus, Math.max(0, matched.length - 1));
  const active: PromotionQueueItem | undefined = matched[focusIdx];
  // Walking past the window widens it. No separate bookkeeping, no way for the
  // cursor to escape what is mounted.
  const rows = matched.slice(0, Math.max(limit, focusIdx + 1));

  useEffect(() => {
    railRef.current
      ?.querySelector<HTMLElement>('[data-focused="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [focusIdx]);

  const reset = () => {
    setLimit(RAIL_WINDOW);
    setFocus(0);
  };
  const setDim = (k: "team" | "kind" | "rule", v: string) => {
    setFilters((f) => {
      const next: Filters = { ...f };
      next[k] = f[k] === v ? null : v;
      return next;
    });
    reset();
  };
  const toggleStale = () => {
    setFilters((f) => ({ ...f, stale: !f.stale }));
    reset();
  };
  const clearAll = () => {
    setFilters(EMPTY);
    reset();
  };

  const toggleSel = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const bulk = (action: "approve" | "reject") => {
    if (!onBulk || selected.size === 0) return;
    onBulk([...selected], action);
    setSelected(new Set());
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    // Never steal a keystroke from something being typed into.
    const t = e.target as HTMLElement;
    if (t.isContentEditable || /^(input|textarea|select)$/i.test(t.tagName)) return;
    const k = e.key.toLowerCase();
    if (k === "j" || e.key === "ArrowDown") {
      e.preventDefault();
      setFocus(Math.min(focusIdx + 1, matched.length - 1));
    } else if (k === "k" || e.key === "ArrowUp") {
      e.preventDefault();
      setFocus(Math.max(focusIdx - 1, 0));
    } else if (k === "x" && active) {
      e.preventDefault();
      toggleSel(active.id);
    } else if ((k === "a" || k === "r") && active && onBulk) {
      e.preventDefault();
      onBulk([active.id], k === "a" ? "approve" : "reject");
      setFocus(Math.min(focusIdx + 1, matched.length - 1));
    }
  };

  const countOf = (key: ContradictionStatus) =>
    key === "all"
      ? counts.reduce((a, c) => a + c.count, 0)
      : (counts.find((c) => c.status === key)?.count ?? 0);

  const tabStyle = (on: boolean) =>
    on
      ? { borderColor: withAlpha(ALPHA, 0.7), background: withAlpha(ALPHA, 0.08), color: ALPHA }
      : { borderColor: HAIR, color: DIM };

  const filtering = filters.team || filters.kind || filters.rule || filters.stale;

  return (
    <div
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className={`${FONT_DISPLAY} mx-auto max-w-7xl px-6 py-8 pb-24 outline-none`}
    >
      <div className={LABEL} style={{ color: ALPHA }}>
        α · reviews · triage rail
      </div>
      <h1 className="mt-1 text-3xl font-semibold tracking-tight text-white">
        Sign what the org will remember.
      </h1>
      <p className={`${FONT_MONO} mt-2 max-w-2xl text-sm leading-relaxed text-[#e9edff]/55`}>
        {promotions.length} promotions waiting · {facets.stale} past the 48h SLO
        {restricted > 0 && <> · {restricted} outside your scope</>}. Every decision here is
        ledgered and signed.
      </p>

      {/* ── filters ────────────────────────────────────────────────────── */}
      <div className="mt-6 flex flex-wrap items-center gap-1.5">
        <span className={`${LABEL} mr-1`} style={{ color: FAINT }}>
          team
        </span>
        {facets.team.map(([v, n]) => (
          <FilterChip
            key={v}
            label={v}
            count={n}
            active={filters.team === v}
            tone={ALPHA}
            onClick={() => setDim("team", v)}
          />
        ))}
        {facets.team.length === 0 && (
          <span className={`${FONT_MONO} text-xs`} style={{ color: FAINT }}>
            no visible teams under this filter
          </span>
        )}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <span className={`${LABEL} mr-1`} style={{ color: FAINT }}>
          kind
        </span>
        {facets.kind.map(([v, n]) => (
          <FilterChip
            key={v}
            label={v}
            count={n}
            active={filters.kind === v}
            tone={ALPHA}
            onClick={() => setDim("kind", v)}
          />
        ))}
        {facets.kind.length === 0 && (
          <span className={`${FONT_MONO} text-xs`} style={{ color: FAINT }}>
            none visible
          </span>
        )}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <span className={`${LABEL} mr-1`} style={{ color: FAINT }}>
          rule
        </span>
        {facets.rule.map(([v, n]) => (
          <FilterChip
            key={v}
            label={v}
            count={n}
            active={filters.rule === v}
            tone={ALPHA}
            onClick={() => setDim("rule", v)}
          />
        ))}
        {facets.rule.length === 0 && (
          <span className={`${FONT_MONO} text-xs`} style={{ color: FAINT }}>
            none
          </span>
        )}
        <span className="mx-1 h-4 w-px" style={{ background: HAIR }} />
        <FilterChip
          label="stale >48h"
          count={facets.stale}
          active={filters.stale}
          tone={MAGENTA}
          onClick={toggleStale}
        />
        {filtering && (
          <button
            type="button"
            onClick={clearAll}
            className={`${FONT_MONO} ml-1 cursor-pointer text-xs underline underline-offset-2 transition hover:text-white`}
            style={{ color: FAINT }}
          >
            clear
          </button>
        )}
      </div>

      {/* ── the bench: rail + pane ─────────────────────────────────────── */}
      <div className="mt-5 grid gap-5 lg:grid-cols-[minmax(0,26rem)_minmax(0,1fr)]">
        {/* rail */}
        <div className="rounded-xl border border-white/10 bg-white/[0.02]">
          <div
            className="flex items-center justify-between gap-2 border-b px-3 py-2"
            style={{ borderColor: "rgba(233,237,255,0.08)" }}
          >
            <span className={LABEL} style={{ color: FAINT }}>
              {matched.length} of {promotions.length}
            </span>
            <button
              type="button"
              onClick={() => setSelected(new Set(matched.map((p) => p.id)))}
              disabled={matched.length === 0}
              className={`${FONT_MONO} cursor-pointer text-xs transition hover:text-white disabled:cursor-not-allowed disabled:opacity-40`}
              style={{ color: DIM }}
            >
              select all {matched.length}
            </button>
          </div>

          {selected.size > 0 && (
            <div
              className="sticky top-0 z-10 flex flex-wrap items-center gap-2 border-b px-3 py-2 backdrop-blur"
              style={{ borderColor: withAlpha(GOLD, 0.2), background: "rgba(12,11,18,0.92)" }}
            >
              <span className={LABEL} style={{ color: GOLD }}>
                {selected.size} selected
              </span>
              <button
                type="button"
                disabled={!onBulk}
                onClick={() => bulk("approve")}
                className={`${FONT_MONO} cursor-pointer rounded-full border px-3 py-1 text-xs transition hover:bg-white/5 disabled:cursor-not-allowed disabled:opacity-40`}
                style={{ borderColor: withAlpha(GOLD, 0.4), color: GOLD }}
              >
                approve {selected.size}
              </button>
              <button
                type="button"
                disabled={!onBulk}
                onClick={() => bulk("reject")}
                className={`${FONT_MONO} cursor-pointer rounded-full border px-3 py-1 text-xs transition hover:bg-white/5 disabled:cursor-not-allowed disabled:opacity-40`}
                style={{ borderColor: withAlpha(MAGENTA, 0.4), color: MAGENTA }}
              >
                reject {selected.size}
              </button>
              <button
                type="button"
                onClick={() => setSelected(new Set())}
                className={`${FONT_MONO} ml-auto cursor-pointer text-xs transition hover:text-white`}
                style={{ color: FAINT }}
              >
                clear
              </button>
              {!onBulk && (
                <span className={`${FONT_MONO} w-full text-xs`} style={{ color: FAINT }}>
                  this surface cannot sign in bulk — decide in the pane
                </span>
              )}
            </div>
          )}

          <div
            ref={railRef}
            tabIndex={0}
            role="group"
            aria-label="Promotion triage rail"
            className="max-h-[62vh] overflow-y-auto outline-none focus-visible:ring-1"
            style={{ scrollbarWidth: "thin" }}
          >
            {rows.length === 0 ? (
              <div className={`${FONT_MONO} px-4 py-12 text-center text-sm text-[#e9edff]/45`}>
                {promotions.length === 0 ? (
                  <>
                    <span className="block" style={{ color: GOLD }}>
                      ◉ in phase
                    </span>
                    <span className="mt-1.5 block">
                      Promotion queue clear — nothing waiting on a maintainer.
                    </span>
                  </>
                ) : (
                  "Nothing under this filter."
                )}
              </div>
            ) : (
              rows.map((p, i) => {
                const focused = i === focusIdx;
                const sel = selected.has(p.id);
                const stale = p.age_secs > STALE_SECS;
                return (
                  <div
                    key={p.id}
                    data-focused={focused}
                    className="flex items-stretch border-l-2 transition"
                    style={{
                      borderLeftColor: focused ? ALPHA : "transparent",
                      background: focused
                        ? withAlpha(ALPHA, 0.07)
                        : sel
                          ? "rgba(233,237,255,0.045)"
                          : "transparent",
                    }}
                  >
                    <button
                      type="button"
                      aria-pressed={sel}
                      aria-label={`Select promotion ${p.id}`}
                      onClick={() => toggleSel(p.id)}
                      className="grid w-7 shrink-0 cursor-pointer place-items-center transition hover:bg-white/5"
                    >
                      <span
                        className="block h-3 w-3 rounded-[3px] border transition"
                        style={{
                          borderColor: sel ? GOLD : "rgba(233,237,255,0.25)",
                          background: sel ? GOLD : "transparent",
                        }}
                      />
                    </button>
                    <button
                      type="button"
                      onClick={() => setFocus(i)}
                      aria-current={focused ? "true" : undefined}
                      className="flex min-w-0 flex-1 cursor-pointer items-baseline gap-2 py-1.5 pr-2.5 text-left transition hover:bg-white/5"
                    >
                      <span
                        className={`${FONT_MONO} w-9 shrink-0 text-xs tabular-nums`}
                        style={{ color: stale ? MAGENTA : FAINT }}
                      >
                        {formatAge(p.age_secs)}
                      </span>
                      <span
                        className={`${FONT_MONO} w-14 shrink-0 truncate text-xs`}
                        style={{ color: FAINT }}
                      >
                        {p.memory?.team ?? "—"}
                      </span>
                      <span
                        className="min-w-0 flex-1 truncate text-sm"
                        style={{
                          color: p.memory ? "rgba(233,237,255,0.85)" : "rgba(233,237,255,0.4)",
                        }}
                      >
                        {p.memory?.content ?? "restricted — claim not visible to you"}
                      </span>
                      <span
                        className={`${FONT_MONO} w-8 shrink-0 text-right text-xs tabular-nums`}
                        style={{ color: p.memory?.confidence != null ? GOLD : "transparent" }}
                      >
                        {p.memory?.confidence != null
                          ? `${(p.memory.confidence * 100).toFixed(0)}%`
                          : "·"}
                      </span>
                    </button>
                  </div>
                );
              })
            )}
          </div>

          {rows.length < matched.length && (
            <button
              type="button"
              onClick={() => setLimit((l) => l + RAIL_WINDOW)}
              className={`${FONT_MONO} w-full cursor-pointer border-t px-3 py-2 text-xs transition hover:bg-white/5`}
              style={{ borderColor: "rgba(233,237,255,0.08)", color: DIM }}
            >
              show {Math.min(RAIL_WINDOW, matched.length - rows.length)} more ·{" "}
              {rows.length}/{matched.length} mounted
            </button>
          )}
        </div>

        {/* pane */}
        <div className="lg:sticky lg:top-6 lg:self-start">
          {active ? (
            <motion.article
              key={active.id}
              initial={reduced ? false : { opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.22 }}
              className="rounded-xl border border-white/10 bg-white/[0.02] p-5"
            >
              <div className={`${LABEL} flex flex-wrap items-center gap-x-3 gap-y-1`} style={{ color: FAINT }}>
                <span style={{ color: active.age_secs > STALE_SECS ? MAGENTA : FAINT }}>
                  waiting {formatAge(active.age_secs)}
                  {active.age_secs > STALE_SECS && " · past SLO"}
                </span>
                <span>
                  {active.from_status} → {active.to_status}
                </span>
                {active.policy_rule && <span style={{ color: ALPHA }}>{active.policy_rule}</span>}
                <span className="normal-case tracking-normal">{active.memory_id}</span>
              </div>

              {active.memory ? (
                <p className="mt-3 text-[15px] leading-relaxed text-[#e9edff]/90">
                  {active.memory.content}
                </p>
              ) : (
                <p className={`${FONT_MONO} mt-3 text-sm leading-relaxed text-[#e9edff]/45`}>
                  memory not visible to you
                  <span className="mt-2 block text-sm" style={{ color: ALPHA }}>
                    row-level security is doing this, not the UI — the reviewer can see that a
                    promotion exists without being shown a claim outside their scope.
                  </span>
                </p>
              )}

              <div className="mt-3 flex flex-wrap items-center gap-2">
                {active.memory?.kind && (
                  <span
                    className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-xs`}
                    style={{ borderColor: HAIR, color: DIM }}
                  >
                    {active.memory.kind}
                  </span>
                )}
                {active.memory?.team && (
                  <span
                    className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-xs`}
                    style={{ borderColor: HAIR, color: DIM }}
                  >
                    team {active.memory.team}
                  </span>
                )}
                {active.memory?.confidence != null && (
                  <span
                    className={`${FONT_MONO} rounded-full border px-2.5 py-0.5 text-xs`}
                    style={{ borderColor: withAlpha(GOLD, 0.33), color: GOLD }}
                  >
                    {(active.memory.confidence * 100).toFixed(0)}% confidence
                  </span>
                )}
              </div>

              {active.provenance ? (
                <p className={`${FONT_MONO} mt-3 text-sm leading-relaxed text-[#e9edff]/45`}>
                  via {active.provenance.actor_kind} {active.provenance.actor_id}
                  {active.provenance.model_ref && <> · {active.provenance.model_ref}</>}
                  {active.provenance.source_kind && (
                    <>
                      {" · from "}
                      {active.provenance.source_kind}
                      {active.provenance.source_ref && <>: {active.provenance.source_ref}</>}
                    </>
                  )}
                </p>
              ) : (
                <p className={`${FONT_MONO} mt-3 text-sm text-[#e9edff]/35`}>
                  provenance not visible to you
                </p>
              )}

              <div className="mt-4 border-t border-white/[0.06] pt-4">
                {promotionControls(active)}
              </div>
            </motion.article>
          ) : (
            <div className="rounded-xl border border-white/10 bg-white/[0.02] p-5">
              <p className={`${FONT_MONO} text-sm text-[#e9edff]/45`}>
                Nothing focused — the rail is empty under this filter.
              </p>
            </div>
          )}

          {/* legend */}
          <div
            className={`${LABEL} mt-3 flex flex-wrap items-center gap-x-3 gap-y-1.5`}
            style={{ color: FAINT }}
          >
            <span className="flex items-center gap-1">
              <Key>j</Key>
              <Key>k</Key> move
            </span>
            <span className="flex items-center gap-1">
              <Key>x</Key> select
            </span>
            <span className="flex items-center gap-1" style={{ opacity: onBulk ? 1 : 0.4 }}>
              <Key>a</Key> approve
            </span>
            <span className="flex items-center gap-1" style={{ opacity: onBulk ? 1 : 0.4 }}>
              <Key>r</Key> reject
            </span>
            {!onBulk && <span>· a/r need a signing surface</span>}
          </div>
        </div>
      </div>

      {/* ── contradictions ─────────────────────────────────────────────── */}
      <section aria-labelledby="wl-contra-h" className="mt-10">
        <div className="flex items-baseline justify-between">
          <h2 id="wl-contra-h" className="scroll-mt-6 text-lg font-semibold text-white">
            Contradictions
          </h2>
          <span className={LABEL} style={{ color: FAINT }}>
            the dark seams
          </span>
        </div>

        <nav
          aria-label="Contradiction status filter"
          className={`${FONT_MONO} mt-3 flex flex-wrap items-center gap-2 text-xs`}
        >
          {STATUS_TABS.map((t) => {
            const on = t.key === cstatus;
            const label = `${t.label} · ${countOf(t.key)}`;
            return statusHref ? (
              <Link
                key={t.key}
                href={statusHref(t.key)}
                aria-current={on ? "true" : undefined}
                className="rounded-full border px-3 py-1 transition"
                style={tabStyle(on)}
              >
                {label}
              </Link>
            ) : (
              <button
                key={t.key}
                type="button"
                aria-current={on ? "true" : undefined}
                onClick={() => {
                  onStatusChange?.(t.key);
                  setClim(CONTRA_WINDOW);
                }}
                className="cursor-pointer rounded-full border px-3 py-1 transition"
                style={tabStyle(on)}
              >
                {label}
              </button>
            );
          })}
        </nav>

        {contradictions.length === 0 ? (
          <div className="mt-3 flex flex-col items-center gap-1.5 rounded-xl border border-white/10 bg-white/[0.02] py-10 text-center">
            <span className={FONT_MONO} style={{ color: GOLD }}>
              ◉ constructive
            </span>
            <p className={`${FONT_MONO} text-sm text-[#e9edff]/55`}>
              {cstatus === "open"
                ? "No open contradictions — every source in phase."
                : "Nothing under this filter."}
            </p>
          </div>
        ) : (
          <div className="mt-3 divide-y rounded-xl border border-white/10 bg-white/[0.02]" style={{ borderColor: HAIR }}>
            {contradictions.slice(0, clim).map((c) => {
              const open = c.status === "open";
              return (
                <div key={c.id} className="px-4 py-3" style={{ borderColor: "rgba(233,237,255,0.07)" }}>
                  <div className="grid gap-1.5 md:grid-cols-2 md:gap-4">
                    <p className="min-w-0 truncate text-sm text-[#e9edff]/85">
                      {c.memory_a.content ?? (
                        <span className="text-[#e9edff]/40">(not visible to you)</span>
                      )}
                    </p>
                    <p
                      className="min-w-0 truncate text-sm text-[#e9edff]/85 md:border-l md:pl-4"
                      style={{ borderColor: withAlpha(MAGENTA, 0.18) }}
                    >
                      {c.memory_b.content ?? (
                        <span className="text-[#e9edff]/40">(not visible to you)</span>
                      )}
                    </p>
                  </div>
                  <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                    <span className={LABEL} style={{ color: FAINT }}>
                      {formatAge(c.age_secs)} old · {c.detected_by}
                      {!open && <> · {c.status}</>}
                      {c.suggested_resolution && (
                        <span style={{ color: ALPHA }}> · suggested {c.suggested_resolution}</span>
                      )}
                    </span>
                    {open && contradictionControls(c)}
                  </div>
                </div>
              );
            })}
            {clim < contradictions.length && (
              <button
                type="button"
                onClick={() => setClim((n) => n + CONTRA_WINDOW)}
                className={`${FONT_MONO} w-full cursor-pointer px-3 py-2 text-xs transition hover:bg-white/5`}
                style={{ color: DIM }}
              >
                show more · {clim}/{contradictions.length} mounted
              </button>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
