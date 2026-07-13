"use client";

/*
 * Disputes — "Testimony" view (annotated-recording lens).
 *
 * Mental model: an EEG technician reviewing a recording someone else marked
 * up. The docket on the left is the take list; the panel on the right is the
 * marked passage — the memory as recorded, with each reader's note pinned to
 * it as an annotation. The reviewer moves j/k through the docket and answers
 * r/d/x without leaving the keyboard: this is the lens for a maintainer who
 * has forty of these and wants them gone.
 */

import { useCallback, useEffect, useState } from "react";
import { motion, useReducedMotion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import { useDecision } from "../DecisionBar";
import {
  ageLabel,
  claimCount,
  daysLeft,
  DECISIONS,
  severity,
  triageOrder,
  type DisputeData,
  type Resolution,
} from "../disputes-data";

const THETA = band("theta");
const GAMMA = band("gamma");

const TONE: Record<Resolution, string> = {
  reverified: GAMMA,
  deprecated: MAGENTA,
  dismissed: "rgba(233,237,255,0.6)",
};

export default function TestimonyView({ data }: { data: DisputeData }) {
  const reduced = useReducedMotion();
  const rows = triageOrder(data.flagged);
  const [cursor, setCursor] = useState(0);
  const { pending, result, decide } = useDecision();
  const active = rows[Math.min(cursor, rows.length - 1)];

  const onKey = useCallback(
    (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "j" || e.key === "ArrowDown") {
        setCursor((c) => Math.min(rows.length - 1, c + 1));
      } else if (e.key === "k" || e.key === "ArrowUp") {
        setCursor((c) => Math.max(0, c - 1));
      } else if (active && data.live && !pending) {
        const hit = DECISIONS.find((d) => d.key === e.key.toLowerCase());
        if (hit) decide(active.memory_id, hit.id);
      }
    },
    [rows.length, active, data.live, pending, decide],
  );

  useEffect(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onKey]);

  return (
    <div className="mx-auto max-w-7xl px-6 py-8">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className={LABEL} style={{ color: THETA }}>
            θ · disputes · testimony
          </div>
          <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
            The recording, and what the listeners wrote on it.
          </h1>
        </div>
        <div className={`${FONT_MONO} text-xs text-[#e9edff]/40`}>
          <kbd className="rounded border border-white/15 px-1.5 py-0.5">j</kbd>{" "}
          <kbd className="rounded border border-white/15 px-1.5 py-0.5">k</kbd> move ·{" "}
          {DECISIONS.map((d) => (
            <span key={d.id}>
              <kbd
                className="rounded border px-1.5 py-0.5"
                style={{ borderColor: `${TONE[d.id]}66`, color: TONE[d.id] }}
              >
                {d.key}
              </kbd>{" "}
              {d.verb}{" "}
            </span>
          ))}
        </div>
      </div>

      <div className="mt-5 grid gap-4 lg:grid-cols-[minmax(0,320px)_1fr]">
        {/* docket */}
        <div className="overflow-hidden rounded-xl border border-white/10">
          <div
            className={`${LABEL} border-b border-white/10 bg-white/[0.02] px-4 py-2.5`}
            style={{ color: "rgba(233,237,255,0.4)" }}
          >
            docket · {rows.length} takes
          </div>
          <ul role="listbox" aria-label="Disputed memories" className="max-h-[520px] overflow-y-auto">
            {rows.map((m, i) => {
              const sel = i === Math.min(cursor, rows.length - 1);
              const tone = m.claims.wrong > 0 ? MAGENTA : THETA;
              return (
                <li key={m.memory_id}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={sel}
                    onClick={() => setCursor(i)}
                    className="w-full border-b border-white/[0.05] px-4 py-3 text-left transition hover:bg-white/[0.03]"
                    style={{
                      background: sel ? "rgba(255,255,255,0.05)" : undefined,
                      borderLeft: `2px solid ${sel ? tone : "transparent"}`,
                    }}
                  >
                    <div className="truncate text-sm text-[#e9edff]/85">{m.content}</div>
                    <div
                      className={`${FONT_MONO} mt-1 flex items-center gap-2 text-[10px] uppercase tracking-widest`}
                      style={{ color: "rgba(233,237,255,0.3)" }}
                    >
                      <span>{m.kind}</span>
                      <span style={{ color: tone }}>
                        {claimCount(m)} claim{claimCount(m) === 1 ? "" : "s"}
                      </span>
                      <span className="ml-auto">{ageLabel(m.oldest_claim_secs)}</span>
                    </div>
                  </button>
                </li>
              );
            })}
            {rows.length === 0 && (
              <li className={`${FONT_MONO} px-4 py-8 text-center text-sm text-[#e9edff]/45`}>
                docket clear
              </li>
            )}
          </ul>
        </div>

        {/* the marked passage */}
        {active ? (
          <motion.div
            key={active.memory_id}
            initial={reduced ? false : { opacity: 0, x: 8 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.22 }}
            className="rounded-xl border border-white/10 bg-white/[0.02] p-6"
          >
            <div
              className={`${LABEL} flex flex-wrap items-center gap-x-3 gap-y-1`}
              style={{ color: "rgba(233,237,255,0.35)" }}
            >
              <span>{active.kind}</span>
              <span>{active.status}</span>
              {active.team_id && <span>team {active.team_id}</span>}
              {(() => {
                const l = daysLeft(active);
                if (l === null) return null;
                return (
                  <span style={{ color: l < 0 ? MAGENTA : undefined }}>
                    {l < 0 ? "validity window closed" : `${Math.round(l)}d validity left`}
                  </span>
                );
              })()}
              <span style={{ color: severity(active) >= 4 ? MAGENTA : THETA }}>
                {active.claims.wrong}× wrong · {active.claims.outdated}× outdated
              </span>
            </div>

            {/* the take */}
            <p className="mt-3 border-l-2 pl-4 text-lg leading-relaxed text-white" style={{ borderColor: THETA }}>
              {active.content}
            </p>

            {/* annotations */}
            <div className={`${LABEL} mt-6`} style={{ color: "rgba(233,237,255,0.35)" }}>
              annotations · what readers wrote on this take
            </div>
            {active.notes.length > 0 ? (
              <ul className="mt-2 space-y-2">
                {active.notes.map((n, k) => (
                  <li
                    key={k}
                    className={`${FONT_MONO} rounded-lg border px-3.5 py-2.5 text-sm text-[#e9edff]/75`}
                    style={{ borderColor: `${MAGENTA}44`, background: "rgba(255,93,162,0.04)" }}
                  >
                    <span className="mr-2 text-[10px] uppercase tracking-widest" style={{ color: MAGENTA }}>
                      reader
                    </span>
                    {n}
                  </li>
                ))}
              </ul>
            ) : (
              <p className={`${FONT_MONO} mt-2 text-sm text-[#e9edff]/35`}>
                marked without a note — {claimCount(active)} reader
                {claimCount(active) === 1 ? "" : "s"} flagged it and said nothing more.
              </p>
            )}

            {/* decision */}
            <div className="mt-6 border-t border-white/10 pt-4">
              <div className={`${FONT_MONO} flex flex-wrap items-center gap-2`}>
                {DECISIONS.map((d) => (
                  <button
                    key={d.id}
                    type="button"
                    disabled={pending || !data.live}
                    onClick={() => decide(active.memory_id, d.id)}
                    title={data.live ? d.gloss : "demo data — connect the API to answer claims"}
                    className="rounded-full border px-3.5 py-1.5 text-sm transition hover:bg-white/5 disabled:cursor-not-allowed disabled:opacity-40"
                    style={{ borderColor: `${TONE[d.id]}66`, color: TONE[d.id] }}
                  >
                    {d.verb}
                    <span className="ml-1.5 text-[10px] text-white/30">{d.key.toUpperCase()}</span>
                  </button>
                ))}
                {result && (
                  <span
                    role="status"
                    className="text-xs"
                    style={{ color: result.ok ? GAMMA : MAGENTA }}
                  >
                    {result.message}
                  </span>
                )}
              </div>
              <p className={`${FONT_MONO} mt-2 text-xs text-[#e9edff]/35`}>
                {DECISIONS.map((d) => `${d.key} — ${d.gloss}`).join("  ·  ")}
              </p>
            </div>
          </motion.div>
        ) : (
          <div className={`${FONT_MONO} grid place-items-center rounded-xl border border-white/10 bg-white/[0.02] p-12 text-sm text-[#e9edff]/45`}>
            Nothing on the docket — no reader disputes an active memory.
          </div>
        )}
      </div>

      {!data.live && (
        <div className={`${LABEL} mt-3`} style={{ color: "rgba(233,237,255,0.3)" }}>
          demo data
        </div>
      )}
    </div>
  );
}
