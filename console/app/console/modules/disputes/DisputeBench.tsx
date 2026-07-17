"use client";

/*
 * Disputes — the decay bench (winner of the 2026-07-13 prototype round;
 * Interference and Testimony were cut).
 *
 * Mental model: the corpus is radioactive. Every memory already has a
 * half-life (the validity window its kind was given); a reader's claim is
 * evidence it is decaying faster than the clock says. The bench reads the
 * corpus against the decay axis — already dark on the left, still hot on the
 * right — so the question stops being "what got flagged" and becomes "what is
 * dying before anyone re-verified it".
 *
 * WHY THIS IS NOT A SCATTER ANYMORE. The prototype plotted each disputed memory
 * as a nucleus on a shared axis. That won on five rows and is unreadable on
 * fifty: a 240px box gives 3.8px of vertical pitch to 12-28px nuclei, an
 * overlapping smear where a click selects whatever z-orders last. So the axis
 * kept its meaning and changed its form: at the top it is a DISTRIBUTION strip
 * (the whole backlog, by decay band — the axis, aggregated, legible at any N
 * and doubling as a filter); in each row it is a per-row decay TRACK with one
 * marker (independent, so N rows never collide). Same axis, honest at scale.
 *
 * Filters, paging and ordering are the SERVER's (Module drives them off the
 * URL). This component renders one ordered page and never re-sorts it.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, useReducedMotion } from "framer-motion";

import { band, bandGlow, FONT_MONO, LABEL, MAGENTA, withAlpha } from "@/design/theme";

import type { DecisionResult } from "./actions";
import DecisionBar from "./DecisionBar";
import {
  ageLabel,
  bandOf,
  claimCount,
  DECAY_BANDS,
  daysLeft,
  pos,
  reporterLabel,
  type DecayBand,
  type DisputeData,
  type DisputedMemory,
} from "./disputes-data";

const THETA = band("theta");
const GAMMA = band("gamma");

/** Band → colour, tracing the decay: magenta (dead/expiring) → theta (fresh) →
 *  grey (no clock). One scale, used by the strip and the row tracks alike. */
const BAND_TONE: Record<DecayBand, string> = {
  past: MAGENTA,
  d30: "#ff8a5d",
  d90: THETA,
  d180: THETA,
  far: withAlpha(THETA, 0.6),
  none: "rgba(233,237,255,0.35)",
};

const MIN_CLAIMS_CHOICES = [2, 3, 5];
const MIN_AGE_CHOICES = [
  { hours: 24, label: "≥1d" },
  { hours: 24 * 7, label: "≥7d" },
  { hours: 24 * 30, label: "≥30d" },
];

export default function DisputeBench({
  data,
  demo = false,
}: {
  data: DisputeData;
  /** The /demo tour renders this with static fixture data and no console route
   *  under it — so URL-driven filtering/paging would navigate the tour away.
   *  In demo mode the controls that depend on a server round trip are hidden;
   *  selecting a row and reading the evidence still works. */
  demo?: boolean;
}) {
  const reduced = useReducedMotion();
  const router = useRouter();
  const searchParams = useSearchParams();

  // The page is already ordered and windowed by the server — render as-is.
  const rows = data.flagged;
  const { total, facets, filter, page, pageSize } = data;
  const pages = Math.max(1, Math.ceil(total / pageSize));

  // Selection is an id, reconciled against the CURRENT page every render.
  // Nothing is auto-selected: an armed DecisionBar over a memory the maintainer
  // never chose is the module's most dangerous failure, so `active` is null
  // until a row is clicked, and becomes null again the instant that row leaves
  // the page (answered, filtered or paged away) rather than snapping to row 0.
  const [selected, setSelected] = useState<string | null>(null);
  const active: DisputedMemory | null =
    rows.find((m) => m.memory_id === selected) ?? null;

  // The receipt lives HERE, not in the DecisionBar: answering revalidates, the
  // answered row leaves `rows`, the bar unmounts — and `claims_closed`, the one
  // number proving the write landed, would go with it. The bench outlives the row.
  const [receipt, setReceipt] = useState<DecisionResult | null>(null);

  // ── URL as the filter's single source of truth ────────────────────────
  const setParams = useCallback(
    (mutate: (p: URLSearchParams) => void, resetPage = true) => {
      const p = new URLSearchParams(searchParams.toString());
      p.set("m", "disputes");
      mutate(p);
      if (resetPage) p.delete("page");
      router.push(`/console?${p.toString()}`, { scroll: false });
    },
    [router, searchParams],
  );

  const toggle = (key: string, value: string) =>
    setParams((p) => {
      if (p.get(key) === value) p.delete(key);
      else p.set(key, value);
    });

  const gotoPage = useCallback(
    (n: number) =>
      setParams((p) => {
        if (n <= 0) p.delete("page");
        else p.set("page", String(n));
      }, false),
    [setParams],
  );

  const activeFilterCount =
    Number(!!filter.kind) +
    Number(!!filter.teamId) +
    Number(!!filter.project) +
    Number(!!filter.band) +
    Number(filter.minClaims !== undefined) +
    Number(filter.minAgeHours !== undefined);

  // ── keyboard: move selection within the page, page with [ ] ───────────
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const idx = rows.findIndex((m) => m.memory_id === selected);
      if (e.key === "ArrowDown" || e.key === "j") {
        e.preventDefault();
        const next = rows[idx < 0 ? 0 : Math.min(rows.length - 1, idx + 1)];
        if (next) setSelected(next.memory_id);
      } else if (e.key === "ArrowUp" || e.key === "k") {
        e.preventDefault();
        const prev = rows[idx <= 0 ? 0 : idx - 1];
        if (prev) setSelected(prev.memory_id);
      } else if (e.key === "[" && page > 0) {
        gotoPage(page - 1);
      } else if (e.key === "]" && page < pages - 1) {
        gotoPage(page + 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [rows, selected, page, pages, gotoPage]);

  const bandTotal = useMemo(
    () => facets.bands.reduce((n, b) => n + b.count, 0) || 1,
    [facets.bands],
  );

  return (
    <div className="mx-auto max-w-6xl px-6 py-8">
      <div className={LABEL} style={{ color: THETA }}>
        θ · disputes · half-life
      </div>

      {/* the bench */}
      <div className="mt-4 rounded-xl border border-white/10 bg-white/[0.02] p-5">
        <div className={`${LABEL} flex items-center justify-between`} style={{ color: "rgba(233,237,255,0.35)" }}>
          <span>decay distribution · the full backlog by validity window</span>
          <span>
            {total} disputed
            {activeFilterCount > 0 && total !== rows.length && (
              <span style={{ color: "rgba(233,237,255,0.3)" }}> · {rows.length} shown</span>
            )}
          </span>
        </div>

        {/* the decay axis, aggregated: one segment per band, width ∝ backlog,
            each a filter. Scales to any N; never a smear. */}
        <div className="mt-4 flex h-9 overflow-hidden rounded-lg border border-white/10">
          {facets.bands.length === 0 && (
            <div className={`${FONT_MONO} grid w-full place-items-center text-[11px] text-[#e9edff]/35`}>
              nothing under dispute
            </div>
          )}
          {facets.bands.map((b) => {
            const on = filter.band === b.value;
            const tone = BAND_TONE[b.value as DecayBand];
            const label = DECAY_BANDS.find((d) => d.band === b.value)?.label ?? b.value;
            return (
              <button
                key={b.value}
                type="button"
                disabled={demo}
                onClick={() => toggle("band", b.value)}
                aria-pressed={on}
                title={demo ? `${label} · ${b.count}` : `${label} · ${b.count} — click to filter`}
                className={`${FONT_MONO} relative min-w-[2.5rem] border-r border-black/30 text-[10px] transition last:border-r-0 ${demo ? "cursor-default" : ""}`}
                style={{
                  flexGrow: b.count,
                  flexBasis: 0,
                  background: withAlpha(tone, on ? 0.5 : 0.18),
                  color: on ? "#0b0b12" : tone,
                  outline: on ? `1px solid ${tone}` : "none",
                }}
              >
                <span className="pointer-events-none px-1">
                  {b.count}
                  {(b.count / bandTotal > 0.12 || on) && (
                    <span className="ml-1 opacity-70">{label}</span>
                  )}
                </span>
              </button>
            );
          })}
        </div>

        {/* filter controls: kind + team facets, min-claims, min-age */}
        {!demo && (
        <div className="mt-4 space-y-2">
          {facets.kinds.length > 1 && (
            <FacetRow
              label="kind"
              options={facets.kinds}
              active={filter.kind}
              onToggle={(v) => toggle("kind", v)}
            />
          )}
          {facets.teams.length > 1 && (
            <FacetRow
              label="team"
              options={facets.teams}
              active={filter.teamId}
              onToggle={(v) => toggle("team", v)}
            />
          )}
          {/* Only once anything is project-stamped: one org-shared bucket has
              no distinction worth a filter row (PR2). */}
          {facets.projects.length > 1 && (
            <FacetRow
              label="project"
              options={facets.projects}
              active={filter.project}
              onToggle={(v) => toggle("project", v)}
            />
          )}
          <div className="flex flex-wrap items-center gap-1.5">
            <span className={`${LABEL} w-12 shrink-0`} style={{ color: "rgba(233,237,255,0.3)" }}>
              claims
            </span>
            {MIN_CLAIMS_CHOICES.map((n) => (
              <Chip
                key={n}
                on={filter.minClaims === n}
                onClick={() => toggle("minClaims", String(n))}
                label={`≥${n}`}
              />
            ))}
            <span className={`${LABEL} ml-3 w-8 shrink-0`} style={{ color: "rgba(233,237,255,0.3)" }}>
              age
            </span>
            {MIN_AGE_CHOICES.map((a) => (
              <Chip
                key={a.hours}
                on={filter.minAgeHours === a.hours}
                onClick={() => toggle("minAge", String(a.hours))}
                label={a.label}
              />
            ))}
            {activeFilterCount > 0 && (
              <button
                type="button"
                onClick={() =>
                  setParams((p) => {
                    ["kind", "team", "project", "band", "minClaims", "minAge"].forEach((k) => p.delete(k));
                  })
                }
                className={`${FONT_MONO} ml-auto rounded-full border border-white/15 px-2.5 py-0.5 text-[11px] text-[#e9edff]/50 transition hover:bg-white/5`}
              >
                clear {activeFilterCount} filter{activeFilterCount === 1 ? "" : "s"}
              </button>
            )}
          </div>
        </div>
        )}

        {/* the list — one ordered page, legible at its own limit */}
        <ul className="mt-5 space-y-1.5">
          {rows.map((m) => {
            const isSel = m.memory_id === selected;
            const p = pos(daysLeft(m));
            const mband = bandOf(m);
            const tone = m.claims.wrong > 0 ? MAGENTA : BAND_TONE[mband];
            return (
              <li key={m.memory_id}>
                <button
                  type="button"
                  onClick={() => setSelected(m.memory_id)}
                  aria-pressed={isSel}
                  className="w-full rounded-lg border px-3 py-2.5 text-left transition hover:bg-white/[0.03]"
                  style={{
                    borderColor: isSel ? withAlpha(tone, 0.5) : "rgba(255,255,255,0.07)",
                    background: isSel ? withAlpha(tone, 0.08) : "transparent",
                  }}
                >
                  <div className="flex items-center gap-3">
                    <span
                      className="h-2.5 w-2.5 shrink-0 rounded-full"
                      style={{ background: withAlpha(tone, 0.85), boxShadow: `0 0 6px ${withAlpha(tone, 0.5)}` }}
                    />
                    <span className="min-w-0 flex-1 truncate text-sm text-[#e9edff]/90">
                      {m.title ?? m.content}
                    </span>
                    <span className={`${FONT_MONO} shrink-0 text-[11px]`} style={{ color: MAGENTA }}>
                      {m.claims.wrong}✗ {m.claims.outdated}◷
                    </span>
                    <span
                      className={`${FONT_MONO} shrink-0 text-[11px]`}
                      style={{ color: m.reporters === 1 ? "rgba(233,237,255,0.4)" : GAMMA }}
                      title={`${m.reporters} distinct reporter${m.reporters === 1 ? "" : "s"}`}
                    >
                      {m.reporters}&#8226;rp
                    </span>
                    <span className={`${FONT_MONO} w-10 shrink-0 text-right text-[11px] text-[#e9edff]/35`}>
                      {ageLabel(m.oldest_claim_secs)}
                    </span>
                  </div>
                  {/* per-row decay track — the axis, one memory, no collisions */}
                  <div className="relative mt-1.5 ml-[1.375rem] h-1 rounded-full" style={{ background: "rgba(255,255,255,0.05)" }}>
                    {p === null ? (
                      <span
                        className={`${FONT_MONO} absolute right-0 -top-0.5 text-[9px]`}
                        style={{ color: "rgba(233,237,255,0.3)" }}
                      >
                        no expiry
                      </span>
                    ) : (
                      <span
                        className="absolute top-1/2 h-2 w-2 -translate-x-1/2 -translate-y-1/2 rounded-full"
                        style={{ left: `${p}%`, background: tone, boxShadow: `0 0 6px ${tone}` }}
                      />
                    )}
                  </div>
                </button>
              </li>
            );
          })}
          {rows.length === 0 && (
            <li className={`${FONT_MONO} grid place-items-center py-10 text-sm text-[#e9edff]/45`}>
              {activeFilterCount > 0 ? "no disputes match this filter" : "nothing decaying under dispute"}
            </li>
          )}
        </ul>

        {/* paging — the backlog past row 50 is reachable */}
        {pages > 1 && (
          <div className={`${FONT_MONO} mt-4 flex items-center justify-between text-[11px] text-[#e9edff]/45`}>
            <button
              type="button"
              disabled={page <= 0}
              onClick={() => gotoPage(page - 1)}
              className="rounded-full border border-white/15 px-3 py-1 transition disabled:opacity-30 hover:enabled:bg-white/5"
            >
              ← prev
            </button>
            <span>
              page {page + 1} of {pages} · {total} disputed
            </span>
            <button
              type="button"
              disabled={page >= pages - 1}
              onClick={() => gotoPage(page + 1)}
              className="rounded-full border border-white/15 px-3 py-1 transition disabled:opacity-30 hover:enabled:bg-white/5"
            >
              next →
            </button>
          </div>
        )}

        <div className={`${FONT_MONO} mt-5 flex flex-wrap items-center gap-4 text-[11px] text-[#e9edff]/35`}>
          <span>✗ wrong · ◷ outdated · rp = distinct reporters</span>
          <span className="ml-auto">↑↓/jk select · [ ] page</span>
        </div>
      </div>

      {/* the sample under the lens — only for an EXPLICITLY chosen, present row */}
      {active ? (
        <motion.div
          key={active.memory_id}
          initial={reduced ? false : { opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.25 }}
          className="mt-4 rounded-xl border p-5"
          style={{
            borderColor: active.claims.wrong > 0 ? withAlpha(MAGENTA, 0.27) : "rgba(233,237,255,0.12)",
            background: `linear-gradient(180deg, ${bandGlow("theta", 0.05)}, transparent)`,
          }}
        >
          <div className={`${LABEL} flex flex-wrap items-center gap-x-3`} style={{ color: "rgba(233,237,255,0.35)" }}>
            <span>{active.kind}</span>
            <span>{active.status}</span>
            <span>{active.team ? `team ${active.team}` : "org-wide"}</span>
            <span>disputed {ageLabel(active.oldest_claim_secs)} ago</span>
            <span style={{ color: (daysLeft(active) ?? 1) < 0 ? MAGENTA : GAMMA }}>
              {(() => {
                const l = daysLeft(active);
                if (l === null) return "no expiry set";
                return l < 0 ? `${Math.abs(Math.round(l))}d past its window` : `${Math.round(l)}d of half-life left`;
              })()}
            </span>
          </div>

          {active.title && (
            <h2 className="mt-2 text-lg font-medium text-[#e9edff]/95">{active.title}</h2>
          )}
          <p className="mt-2 text-base leading-relaxed text-[#e9edff]/90">{active.content}</p>

          <div
            className={`${FONT_MONO} mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px]`}
            style={{ color: "rgba(233,237,255,0.4)" }}
          >
            {active.provenance ? (
              <span>
                from {active.provenance.actor_kind} · {active.provenance.actor_id}
                {active.provenance.model_ref && ` · ${active.provenance.model_ref}`}
              </span>
            ) : (
              <span>provenance unrecorded</span>
            )}
            <span>
              {active.confidence === null
                ? "no confidence recorded"
                : `confidence ${active.confidence.toFixed(2)}`}
            </span>
          </div>

          <div className={`${FONT_MONO} mt-3 text-xs`} style={{ color: MAGENTA }}>
            {claimCount(active)} claim{claimCount(active) === 1 ? "" : "s"} ·{" "}
            {active.claims.wrong}× wrong · {active.claims.outdated}× outdated
            {" · "}
            <span style={{ color: active.reporters === 1 ? GAMMA : MAGENTA }}>
              {active.reporters} reporter{active.reporters === 1 ? "" : "s"}
            </span>
          </div>

          {active.reports.length > 0 ? (
            <ul className="mt-2 space-y-2 border-l-2 pl-3" style={{ borderColor: withAlpha(MAGENTA, 0.33) }}>
              {active.reports.map((r) => (
                <li key={`${r.reporter_id}-${r.age_secs}-${r.verdict}`}>
                  <div className={`${FONT_MONO} flex flex-wrap items-center gap-x-2 text-[11px]`}>
                    <span style={{ color: r.verdict === "wrong" ? MAGENTA : THETA }}>{r.verdict}</span>
                    <span style={{ color: "rgba(233,237,255,0.55)" }}>{reporterLabel(r)}</span>
                    {r.reporter_on_owning_team && (
                      <span
                        className="rounded-full px-1.5"
                        style={{ background: withAlpha(GAMMA, 0.14), color: GAMMA }}
                        title="the reporter sits on the team that owns this memory"
                      >
                        owning team
                      </span>
                    )}
                    <span style={{ color: "rgba(233,237,255,0.3)" }}>{ageLabel(r.age_secs)} ago</span>
                  </div>
                  {r.note && (
                    <p className={`${FONT_MONO} text-sm text-[#e9edff]/65`}>“{r.note}”</p>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className={`${FONT_MONO} mt-2 text-sm text-[#e9edff]/35`}>
              reported without a note — the verdict is the whole signal
            </p>
          )}

          <div className="mt-4">
            <DecisionBar memoryId={active.memory_id} live={data.live} onResult={setReceipt} />
          </div>
        </motion.div>
      ) : (
        rows.length > 0 && (
          <div
            className={`${FONT_MONO} mt-4 grid place-items-center rounded-xl border border-dashed border-white/10 py-8 text-sm text-[#e9edff]/35`}
          >
            {selected
              ? "that dispute left the queue — pick another to answer"
              : "select a dispute to see its reporters and answer it"}
          </div>
        )
      )}

      {receipt && (
        <div
          role="status"
          className={`${FONT_MONO} mt-3 flex items-start gap-3 rounded-lg border px-3.5 py-2.5 text-sm`}
          style={{
            borderColor: withAlpha(receipt.ok ? GAMMA : MAGENTA, 0.35),
            color: receipt.ok ? GAMMA : MAGENTA,
          }}
        >
          <span className="flex-1">{receipt.message}</span>
          <button
            type="button"
            onClick={() => setReceipt(null)}
            className="text-[#e9edff]/40 transition hover:text-[#e9edff]/80"
            aria-label="dismiss"
          >
            ×
          </button>
        </div>
      )}

      {!data.live && (
        <div className={`${LABEL} mt-3`} style={{ color: "rgba(233,237,255,0.3)" }}>
          demo data
        </div>
      )}
    </div>
  );
}

function Chip({ on, onClick, label }: { on: boolean; onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`${FONT_MONO} rounded-full border px-2 py-0.5 text-[11px] transition hover:bg-white/5`}
      style={{
        borderColor: on ? withAlpha(THETA, 0.55) : "rgba(233,237,255,0.12)",
        color: on ? THETA : "rgba(233,237,255,0.5)",
        background: on ? withAlpha(THETA, 0.1) : "transparent",
      }}
    >
      {label}
    </button>
  );
}

function FacetRow({
  label,
  options,
  active,
  onToggle,
}: {
  label: string;
  options: { value: string; label: string; count: number }[];
  active: string | undefined;
  onToggle: (v: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className={`${LABEL} w-12 shrink-0`} style={{ color: "rgba(233,237,255,0.3)" }}>
        {label}
      </span>
      {options.map((o) => (
        <Chip
          key={o.value || "orgwide"}
          on={active === o.value}
          onClick={() => onToggle(o.value)}
          label={`${o.label} ${o.count}`}
        />
      ))}
    </div>
  );
}
