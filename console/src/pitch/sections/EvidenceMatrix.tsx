"use client";

/*
 * EVIDENCE — "the results matrix".
 *
 * Won the section prototype round against a transcript variant.
 *
 * The whole trial in one grid: tasks down, arms across, outcome in the cell.
 * It is scannable in a single pass, and — the reason it won — the control we
 * LOST sits in the same glance as the two we won. The honesty claim is not made
 * in a sentence somewhere below; it is structural, and you cannot read the wins
 * without reading the loss.
 */

import { motion } from "framer-motion";

import { band, FONT_MONO, LABEL, MAGENTA } from "../../design/theme";
import { TRIAL } from "../pitch-data";

const GOLD = band("gamma");
const dim = (a: number) => `rgba(233,237,255,${a})`;

const rise = {
  hidden: { opacity: 0, y: 14 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.45 } },
};

const ARMS = [
  { key: "cold", label: "cold agent", sub: "no memory at all" },
  { key: "baseline", label: "native memory", sub: "the free baseline" },
  { key: "brainiac", label: "Brainiac", sub: "governed store" },
] as const;

function Outcome({ text, kind }: { text: string; kind: "win" | "fail" | "partial" | "none" }) {
  if (kind === "none") {
    return (
      <span className={FONT_MONO} style={{ color: dim(0.2) }}>
        —
      </span>
    );
  }
  const tone = kind === "win" ? GOLD : kind === "fail" ? MAGENTA : dim(0.5);
  const glyph = kind === "win" ? "✓" : kind === "fail" ? "✗" : "~";
  return (
    <div>
      <span className={`${FONT_MONO} text-base`} style={{ color: tone }}>
        {glyph}
      </span>
      <p className="mt-1.5 text-sm leading-relaxed" style={{ color: dim(0.62) }}>
        {text}
      </p>
    </div>
  );
}

export function EvidenceMatrix() {
  return (
    <motion.div
      variants={rise}
      initial="hidden"
      whileInView="visible"
      viewport={{ once: true, amount: 0.2 }}
      className="overflow-x-auto"
    >
      <table className="w-full min-w-[860px] border-collapse">
        <thead>
          <tr>
            <th className="w-[26%] p-0" />
            {ARMS.map((a) => {
              const us = a.key === "brainiac";
              return (
                <th key={a.key} className="px-4 pb-4 text-left align-bottom">
                  <div
                    className={`${FONT_MONO} text-sm`}
                    style={{ color: us ? GOLD : dim(0.7) }}
                  >
                    {a.label}
                  </div>
                  <div className={LABEL} style={{ color: dim(0.3) }}>
                    {a.sub}
                  </div>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {TRIAL.rows.map((r) => {
            const lost = r.verdict === "loss";
            return (
              <tr key={r.task}>
                <td
                  className="border-t py-6 pr-6 align-top"
                  style={{ borderColor: "rgba(233,237,255,0.08)" }}
                >
                  <div className={LABEL} style={{ color: lost ? dim(0.35) : GOLD }}>
                    {lost ? "the control" : r.gap}
                  </div>
                  <p className="mt-2 text-sm leading-relaxed text-white">{r.task}</p>
                </td>

                <td
                  className="border-t px-4 py-6 align-top"
                  style={{ borderColor: "rgba(233,237,255,0.08)" }}
                >
                  <Outcome
                    text={r.cold}
                    kind={r.cold === "—" ? "none" : lost ? "none" : "partial"}
                  />
                </td>

                <td
                  className="border-t px-4 py-6 align-top"
                  style={{ borderColor: "rgba(233,237,255,0.08)" }}
                >
                  <Outcome text={r.baseline} kind={lost ? "win" : "fail"} />
                </td>

                <td
                  className="border-t px-4 py-6 align-top"
                  style={{
                    borderColor: "rgba(233,237,255,0.08)",
                    background: lost ? "rgba(255,255,255,0.015)" : "hsla(46,90%,60%,0.04)",
                  }}
                >
                  <Outcome text={r.brainiac} kind={lost ? "partial" : "win"} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <div className={`${FONT_MONO} mt-6 flex flex-wrap gap-6 text-xs`} style={{ color: dim(0.45) }}>
        <span>
          <span style={{ color: GOLD }}>✓</span> answered correctly
        </span>
        <span>
          <span style={{ color: MAGENTA }}>✗</span> wrong, or could not answer
        </span>
        <span>
          <span style={{ color: dim(0.5) }}>~</span> right answer, no better than the baseline
        </span>
      </div>
    </motion.div>
  );
}
