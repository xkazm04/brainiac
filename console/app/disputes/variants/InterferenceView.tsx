"use client";

/*
 * Disputes — "Interference" view (physics lens).
 *
 * Mental model: every canonical memory is a signal the org is transmitting.
 * A reader's claim is a wave arriving out of phase with it. `wrong` inverts
 * the signal (it was never true); `outdated` attenuates it (it stopped being
 * true). The strip for each memory shows the standing wave with a notch per
 * claim — you can see, at a glance, which of the org's signals are being
 * cancelled and how hard. Answering restores, nulls, or cancels the noise.
 */

import { motion, useReducedMotion } from "framer-motion";

import { band, FONT_DISPLAY, FONT_MONO, LABEL, MAGENTA } from "@/design/theme";

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

/**
 * The memory's signal, with a destructive notch per claim. `wrong` claims
 * invert the wave (phase flip); `outdated` claims flatten it. Pure geometry —
 * no animation loop, path is drawn once on entry.
 */
function SignalStrip({ m }: { m: DisputedMemory }) {
  const W = 260;
  const H = 44;
  const mid = H / 2;
  const wrong = m.claims.wrong;
  const outdated = m.claims.outdated;
  const claims = wrong + outdated;
  // Amplitude falls as the memory is attenuated; inversion grows with `wrong`.
  const amp = Math.max(3, 15 - outdated * 3);
  const notchAt = Array.from({ length: claims }, (_, i) => ((i + 1) / (claims + 1)) * W);

  let d = `M 0 ${mid}`;
  for (let x = 0; x <= W; x += 4) {
    // Nearest notch pulls the wave toward (or through) the axis.
    const near = notchAt.reduce(
      (acc, nx) => Math.min(acc, Math.abs(x - nx)),
      Number.POSITIVE_INFINITY,
    );
    const dip = Math.exp(-(near * near) / 900); // gaussian well around the claim
    const inverted = wrong > 0 ? 1 - 2 * dip : 1 - dip;
    const y = mid - Math.sin((x / W) * Math.PI * 6) * amp * inverted;
    d += ` L ${x} ${y.toFixed(1)}`;
  }

  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} aria-hidden className="shrink-0">
      <line x1="0" y1={mid} x2={W} y2={mid} stroke="rgba(233,237,255,0.08)" strokeWidth="1" />
      <motion.path
        d={d}
        fill="none"
        stroke={wrong > 0 ? MAGENTA : THETA}
        strokeWidth="1.5"
        initial={{ pathLength: 0, opacity: 0 }}
        animate={{ pathLength: 1, opacity: 1 }}
        transition={{ duration: 0.6, ease: "easeOut" }}
      />
      {notchAt.map((x, i) => (
        <line
          key={i}
          x1={x}
          y1={mid - 18}
          x2={x}
          y2={mid + 18}
          stroke={i < wrong ? MAGENTA : THETA}
          strokeWidth="1"
          strokeDasharray="2 3"
          opacity={0.5}
        />
      ))}
    </svg>
  );
}

export default function InterferenceView({ data }: { data: DisputeData }) {
  const reduced = useReducedMotion();
  const rows = triageOrder(data.flagged);
  const totalClaims = rows.reduce((n, m) => n + claimCount(m), 0);
  const inverted = rows.filter((m) => m.claims.wrong > 0).length;

  return (
    <div className="mx-auto max-w-6xl px-6 py-8">
      <div className={LABEL} style={{ color: THETA }}>
        θ · disputes · interference
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        Readers are transmitting against us.
      </h1>
      <p className={`${FONT_MONO} mt-2 max-w-2xl text-sm leading-relaxed text-[#e9edff]/55`}>
        Every memory below is a signal the organization still broadcasts. The notches are
        agents and operators arriving out of phase — {inverted} inverted (
        <span style={{ color: MAGENTA }}>wrong</span>), the rest attenuated (
        <span style={{ color: THETA }}>outdated</span>). Until you answer, retrieval keeps
        serving them with a warning.
      </p>

      <div className={`${FONT_MONO} mt-5 flex items-center gap-4 text-xs text-[#e9edff]/40`}>
        <span>
          <span className="text-[#e9edff]/85">{rows.length}</span> memories disputed
        </span>
        <span>
          <span className="text-[#e9edff]/85">{totalClaims}</span> standing claims
        </span>
        {!data.live && <span className="ml-auto">demo data</span>}
      </div>

      <div className="mt-4 space-y-3">
        {rows.map((m, i) => {
          const left = daysLeft(m);
          const expired = left !== null && left < 0;
          return (
            <motion.article
              key={m.memory_id}
              initial={reduced ? false : { opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.3, delay: Math.min(0.3, i * 0.05) }}
              className="rounded-xl border border-white/10 bg-white/[0.02] p-4 transition hover:border-white/20"
              style={{
                borderColor: m.claims.wrong > 0 ? `${MAGENTA}44` : undefined,
              }}
            >
              <div className="flex flex-wrap items-start gap-4">
                <SignalStrip m={m} />
                <div className="min-w-0 flex-1">
                  <p className="text-sm leading-relaxed text-[#e9edff]/90">{m.content}</p>
                  <div className={`${LABEL} mt-2 flex flex-wrap items-center gap-x-3 gap-y-1`} style={{ color: "rgba(233,237,255,0.35)" }}>
                    <span>{m.kind}</span>
                    <span>{m.status}</span>
                    {m.team_id && <span>team {m.team_id}</span>}
                    <span>standing {ageLabel(m.oldest_claim_secs)}</span>
                    {left !== null && (
                      <span style={{ color: expired ? MAGENTA : undefined }}>
                        {expired ? "window closed" : `${Math.round(left)}d of validity left`}
                      </span>
                    )}
                    <span style={{ color: severity(m) >= 4 ? MAGENTA : THETA }}>
                      {m.claims.wrong}× wrong · {m.claims.outdated}× outdated
                    </span>
                  </div>
                </div>
              </div>

              {m.notes.length > 0 && (
                <ul className={`${FONT_MONO} mt-3 space-y-1.5 border-l-2 pl-3 text-xs text-[#e9edff]/60`} style={{ borderColor: `${MAGENTA}55` }}>
                  {m.notes.map((n, k) => (
                    <li key={k}>“{n}”</li>
                  ))}
                </ul>
              )}

              <div className="mt-3.5">
                <DecisionBar memoryId={m.memory_id} live={data.live} size="sm" />
              </div>
            </motion.article>
          );
        })}

        {rows.length === 0 && (
          <div className={`${FONT_MONO} rounded-xl border border-white/10 bg-white/[0.02] p-8 text-center text-sm text-[#e9edff]/50`}>
            No interference. Every signal the org broadcasts is unchallenged.
          </div>
        )}
      </div>
    </div>
  );
}
