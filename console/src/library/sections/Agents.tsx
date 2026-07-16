"use client";

/* Move 5 — the programmatic surface: three keys, three jobs. */

import { motion } from "framer-motion";

import { FONT_MONO, LABEL } from "../../design/theme";
import { AGENTS } from "../library-data";
import { ALPHA, H2, Lede, Section, Stamp, dim, rise } from "../primitives";

export default function Agents() {
  return (
    <Section id="agents" eyebrow="for coding agents" tone={ALPHA}>
      <H2>{AGENTS.headline}</H2>
      <Lede>{AGENTS.body}</Lede>

      <motion.div
        variants={rise}
        className="mt-12 rounded-xl border p-7"
        style={{ borderColor: "hsla(190,90%,68%,0.2)", background: "hsla(190,90%,60%,0.02)" }}
      >
        <div className="flex flex-wrap items-center gap-3">
          <div className={LABEL} style={{ color: ALPHA }}>
            three keys, three jobs
          </div>
          <Stamp status={AGENTS.status} />
        </div>
        <div className="mt-6 grid gap-6 md:grid-cols-3">
          {AGENTS.rows.map((r) => (
            <div key={r.scope} className="flex flex-col items-start gap-3">
              <span
                className={`${FONT_MONO} rounded-md border px-2.5 py-1.5 text-xs`}
                style={{
                  borderColor: "hsla(190,90%,68%,0.35)",
                  color: ALPHA,
                  background: "hsla(190,90%,60%,0.05)",
                }}
              >
                {r.scope}
              </span>
              <p className="text-sm leading-relaxed" style={{ color: dim(0.55) }}>
                {r.body}
              </p>
            </div>
          ))}
        </div>
      </motion.div>
    </Section>
  );
}
