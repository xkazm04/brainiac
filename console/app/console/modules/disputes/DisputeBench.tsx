"use client";

/*
 * Disputes — the decay bench (winner of the 2026-07-13 prototype round;
 * Interference and Testimony were cut).
 *
 * Mental model: the corpus is radioactive. Every memory already has a
 * half-life (the validity window its kind was given); a reader's claim is
 * evidence it is decaying faster than the clock says. The bench plots each
 * disputed memory against the decay axis — already dark on the left, still
 * hot on the right — so the question stops being "what got flagged" and
 * becomes "what is dying before anyone re-verified it". Answering pushes a
 * memory right (re-verified), collapses it now (deprecated), or leaves it
 * decaying on schedule (dismissed).
 *
 * This is the lens where the freshness lifecycle (TTL) and the feedback loop
 * are visibly the same mechanism.
 */

import { useState } from "react";
import { motion, useReducedMotion } from "framer-motion";

import {
  band,
  bandGlow,
  FONT_MONO,
  LABEL,
  MAGENTA,
  withAlpha,
} from "@/design/theme";

import type { DecisionResult } from "./actions";
import DecisionBar from "./DecisionBar";
import {
  ageLabel,
  claimCount,
  daysLeft,
  reporterLabel,
  severity,
  triageOrder,
  type DisputeData,
  type DisputedMemory,
} from "./disputes-data";

const THETA = band("theta");
const GAMMA = band("gamma");

/** Decay axis: -30d … +180d, clamped, log-ish so the near term breathes. */
const AXIS_MIN = -30;
const AXIS_MAX = 180;
const pos = (days: number | null) => {
  if (days === null) return 100; // no TTL = never decays on the clock
  const clamped = Math.min(AXIS_MAX, Math.max(AXIS_MIN, days));
  return ((clamped - AXIS_MIN) / (AXIS_MAX - AXIS_MIN)) * 100;
};

const TICKS = [
  { at: 0, label: "now" },
  { at: 30, label: "30d" },
  { at: 90, label: "90d" },
  { at: 180, label: "180d+" },
];

export default function DisputeBench({ data }: { data: DisputeData }) {
  const reduced = useReducedMotion();
  const rows = triageOrder(data.flagged);
  const [selected, setSelected] = useState<string | null>(rows[0]?.memory_id ?? null);
  // The receipt lives HERE, not in the DecisionBar, because the DecisionBar
  // does not survive its own success: answering revalidates, the answered row
  // leaves `rows`, and the bar unmounts — taking `claims_closed`, the only
  // proof the write landed, with it before anyone read it.
  const [receipt, setReceipt] = useState<DecisionResult | null>(null);
  const active: DisputedMemory | undefined =
    rows.find((m) => m.memory_id === selected) ?? rows[0];

  const dark = rows.filter((m) => (daysLeft(m) ?? 1) < 0).length;

  return (
    <div className="mx-auto max-w-6xl px-6 py-8">
      {/* No headline and no standfirst. An operator opens this module already
          knowing what it is; the prose that used to sit here was explaining the
          product, which is the tour's job, not the console's — it lives in the
          demo's own intro element (app/demo/ModuleIntro.tsx) and nowhere else.
          The one thing that paragraph carried that was NOT pitch — how many
          memories are already past their window — was a real number, so it moved
          down onto the bench rather than out. */}
      <div className={LABEL} style={{ color: THETA }}>
        θ · disputes · half-life
      </div>

      {/* the bench */}
      <div className="mt-4 rounded-xl border border-white/10 bg-white/[0.02] p-5">
        <div className={`${LABEL} flex items-center justify-between`} style={{ color: "rgba(233,237,255,0.35)" }}>
          <span>decay axis · time until the validity window closes</span>
          <span>
            {rows.length} disputed
            {dark > 0 && (
              <>
                {" · "}
                <span style={{ color: MAGENTA }}>{dark} past window</span>
              </>
            )}
          </span>
        </div>

        <div className="relative mt-6 h-[240px]">
          {/* now-line and ticks */}
          {TICKS.map((t) => (
            <div
              key={t.label}
              className="absolute inset-y-0"
              style={{ left: `${pos(t.at)}%` }}
            >
              <div
                className="h-full border-l"
                style={{
                  borderColor: t.at === 0 ? withAlpha(MAGENTA, 0.4) : "rgba(233,237,255,0.07)",
                }}
              />
              <span
                className={`${FONT_MONO} absolute -bottom-5 -translate-x-1/2 text-[10px]`}
                style={{ color: t.at === 0 ? MAGENTA : "rgba(233,237,255,0.3)" }}
              >
                {t.label}
              </span>
            </div>
          ))}

          {/* memories as decaying nuclei */}
          {rows.map((m, i) => {
            const left = daysLeft(m);
            const expired = left !== null && left < 0;
            const isSel = m.memory_id === active?.memory_id;
            const y = rows.length === 1 ? 50 : (i / (rows.length - 1)) * 78 + 8;
            const size = 12 + Math.min(16, severity(m) * 4);
            const tone = expired || m.claims.wrong > 0 ? MAGENTA : THETA;
            return (
              <motion.button
                key={m.memory_id}
                type="button"
                onClick={() => setSelected(m.memory_id)}
                aria-pressed={isSel}
                title={m.content}
                className="absolute -translate-x-1/2 -translate-y-1/2 rounded-full transition"
                style={{
                  left: `${pos(left)}%`,
                  top: `${y}%`,
                  width: size,
                  height: size,
                  // The last of the hex-suffix bugs, and the loudest: `tone` is
                  // MAGENTA (hex) for a dying memory but THETA (hsla) for a
                  // living one, so every non-expired nucleus on this bench was
                  // rendering with no fill at all — only its 1px border.
                  background: withAlpha(tone, isSel ? 0.8 : 0.33),
                  border: `1px solid ${tone}`,
                  boxShadow: isSel ? `0 0 18px ${tone}` : `0 0 8px ${withAlpha(tone, 0.27)}`,
                }}
                initial={reduced ? false : { opacity: 0, scale: 0.4 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ duration: 0.35, delay: Math.min(0.4, i * 0.06) }}
              />
            );
          })}

          {rows.length === 0 && (
            <div className={`${FONT_MONO} grid h-full place-items-center text-sm text-[#e9edff]/45`}>
              nothing decaying under dispute
            </div>
          )}
        </div>

        <div className={`${FONT_MONO} mt-7 flex items-center gap-4 text-[11px] text-[#e9edff]/35`}>
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-2 w-2 rounded-full" style={{ background: MAGENTA }} />
            wrong / already dark
          </span>
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-2 w-2 rounded-full" style={{ background: THETA }} />
            outdated, still hot
          </span>
          <span className="ml-auto">bigger = more claims against it</span>
        </div>
      </div>

      {/* the sample under the lens */}
      {active && (
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
            {/* A NAME. `team_id` is a uuid, and rendering it read as
                "team 3f2a9c1e-…" to everyone whose data was real. */}
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

          {/* Where it came from and how sure the corpus was. A `wrong` claim
              against a 0.44-confidence LLM extraction and one against a fact a
              human wrote are not the same claim; the bench used to show
              neither, so they looked identical. */}
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
            {/* The decisive number. "5 claims" from one agent firing five times
                is not five people, and it was undecidable from this screen. */}
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
                    {/* Undated quotes were the trap: a note from yesterday and
                        one from two years ago looked identical. */}
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
