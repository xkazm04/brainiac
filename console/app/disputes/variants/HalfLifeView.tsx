"use client";

/*
 * Disputes — "Half-life" view (decay bench).
 *
 * Mental model: the corpus is radioactive. Every memory already has a
 * half-life (its TTL window); a reader's claim is evidence it is decaying
 * faster than the clock says. The bench plots each disputed memory against
 * the decay axis — already dark on the left, still hot on the right — so the
 * question stops being "what got flagged" and becomes "what is dying before
 * anyone re-verified it". Answering pushes a memory right (re-verified),
 * collapses it now (deprecated), or leaves it decaying on schedule.
 *
 * This is the lens where the freshness lifecycle and the feedback loop are
 * visibly the same mechanism.
 */

import { useState } from "react";
import { motion, useReducedMotion } from "framer-motion";

import { band, bandGlow, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

import DecisionBar from "../DecisionBar";
import {
  ageLabel,
  claimCount,
  daysLeft,
  severity,
  triageOrder,
  type DisputeData,
  type DisputedMemory,
} from "../disputes-data";

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

export default function HalfLifeView({ data }: { data: DisputeData }) {
  const reduced = useReducedMotion();
  const rows = triageOrder(data.flagged);
  const [selected, setSelected] = useState<string | null>(rows[0]?.memory_id ?? null);
  const active: DisputedMemory | undefined =
    rows.find((m) => m.memory_id === selected) ?? rows[0];

  const dark = rows.filter((m) => (daysLeft(m) ?? 1) < 0).length;

  return (
    <div className="mx-auto max-w-6xl px-6 py-8">
      <div className={LABEL} style={{ color: THETA }}>
        θ · disputes · half-life
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        What is decaying faster than the clock says.
      </h1>
      <p className={`${FONT_MONO} mt-2 max-w-2xl text-sm leading-relaxed text-[#e9edff]/55`}>
        Each memory carries a half-life — the validity window its kind was given. A reader&apos;s
        claim is evidence it is already dead. {dark > 0 ? `${dark} sit past their window` : "None are past their window yet"};
        the rest are still being served while disputed.
      </p>

      {/* the bench */}
      <div className="mt-6 rounded-xl border border-white/10 bg-white/[0.02] p-5">
        <div className={`${LABEL} flex items-center justify-between`} style={{ color: "rgba(233,237,255,0.35)" }}>
          <span>decay axis · time until the validity window closes</span>
          <span>{rows.length} disputed</span>
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
                  borderColor: t.at === 0 ? `${MAGENTA}66` : "rgba(233,237,255,0.07)",
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
                  background: `${tone}${isSel ? "cc" : "55"}`,
                  border: `1px solid ${tone}`,
                  boxShadow: isSel ? `0 0 18px ${tone}` : `0 0 8px ${tone}44`,
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
            borderColor: active.claims.wrong > 0 ? `${MAGENTA}44` : "rgba(233,237,255,0.12)",
            background: `linear-gradient(180deg, ${bandGlow("theta", 0.05)}, transparent)`,
          }}
        >
          <div className={`${LABEL} flex flex-wrap items-center gap-x-3`} style={{ color: "rgba(233,237,255,0.35)" }}>
            <span>{active.kind}</span>
            <span>{active.status}</span>
            {active.team_id && <span>team {active.team_id}</span>}
            <span>disputed {ageLabel(active.oldest_claim_secs)} ago</span>
            <span style={{ color: (daysLeft(active) ?? 1) < 0 ? MAGENTA : GAMMA }}>
              {(() => {
                const l = daysLeft(active);
                if (l === null) return "no expiry set";
                return l < 0 ? `${Math.abs(Math.round(l))}d past its window` : `${Math.round(l)}d of half-life left`;
              })()}
            </span>
          </div>

          <p className="mt-2.5 text-base leading-relaxed text-[#e9edff]/90">{active.content}</p>

          <div className={`${FONT_MONO} mt-3 text-xs`} style={{ color: MAGENTA }}>
            {claimCount(active)} claim{claimCount(active) === 1 ? "" : "s"} ·{" "}
            {active.claims.wrong}× wrong · {active.claims.outdated}× outdated
          </div>
          {active.notes.length > 0 ? (
            <ul className={`${FONT_MONO} mt-2 space-y-1.5 border-l-2 pl-3 text-sm text-[#e9edff]/65`} style={{ borderColor: `${MAGENTA}55` }}>
              {active.notes.map((n, k) => (
                <li key={k}>“{n}”</li>
              ))}
            </ul>
          ) : (
            <p className={`${FONT_MONO} mt-2 text-sm text-[#e9edff]/35`}>
              reported without a note — the verdict is the whole signal
            </p>
          )}

          <div className="mt-4">
            <DecisionBar memoryId={active.memory_id} live={data.live} />
          </div>
        </motion.div>
      )}

      {!data.live && (
        <div className={`${LABEL} mt-3`} style={{ color: "rgba(233,237,255,0.3)" }}>
          demo data
        </div>
      )}
    </div>
  );
}
