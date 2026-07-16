"use client";

/* Move 3 — the anatomy: five properties, five drawings, alternating sides.
   One is shipped (the detector); the stamps say so and the tests pin them. */

import { motion } from "framer-motion";

import { FONT_MONO, LABEL } from "../../design/theme";
import { DetectorBeat, ProvenanceChain, RuleAtom, SkillShelf, VitalsFigure } from "../figures/anatomy";
import { PROPERTIES } from "../library-data";
import { H2, Lede, MINT, Section, Stamp, dim, rise } from "../primitives";

const PROPERTY_ART: Record<string, React.ReactNode> = {
  detector: <DetectorBeat />,
  atom: <RuleAtom />,
  provenance: <ProvenanceChain />,
  skills: <SkillShelf />,
  vitals: <VitalsFigure />,
};

export default function Anatomy() {
  return (
    <Section id="anatomy" eyebrow="the anatomy" tone={MINT}>
      <H2>A library with a pulse, part by part.</H2>
      <Lede>
        Five mechanisms, drawn. Four run today — the detector, the rule atom, the
        attribution constraint, the skill shelf — and the pulse is readable in the console.
        The vital signs stay stamped roadmap for one honest reason: nothing goes red on its
        own yet; a human still has to look. The stamps are tested.
      </Lede>

      <div className="mt-14 space-y-6">
        {PROPERTIES.map((p, i) => (
          <motion.article
            key={p.key}
            variants={rise}
            className="grid items-center gap-8 rounded-xl border p-7 md:p-8 lg:grid-cols-[1.05fr_1fr]"
            style={{
              borderColor:
                p.status === "shipped" ? "hsla(158,90%,68%,0.18)" : "hsla(190,90%,68%,0.16)",
              background: "rgba(255,255,255,0.02)",
            }}
          >
            <div className={i % 2 ? "lg:order-2" : ""}>{PROPERTY_ART[p.key]}</div>
            <div className={i % 2 ? "lg:order-1" : ""}>
              <div className="flex items-baseline gap-3">
                <span className={LABEL} style={{ color: dim(0.3) }}>
                  {String(i + 1).padStart(2, "0")}
                </span>
                <h3 className="text-xl font-semibold leading-snug tracking-tight text-white">
                  {p.title}
                </h3>
              </div>
              <p className="mt-4 text-sm leading-relaxed" style={{ color: dim(0.65) }}>
                {p.body}
              </p>
              <div className="mt-5 flex flex-wrap items-center gap-3">
                <Stamp status={p.status} />
                <span className={`${FONT_MONO} text-xs`} style={{ color: dim(0.35) }}>
                  {p.evidence}
                </span>
              </div>
            </div>
          </motion.article>
        ))}
      </div>
    </Section>
  );
}
