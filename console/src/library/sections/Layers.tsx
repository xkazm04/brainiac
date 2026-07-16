"use client";

/* Move 2 — the third layer: the drawing carries the flows; the legend names
   them one line each, forbidden ones in magenta. */

import { motion } from "framer-motion";

import { FONT_MONO, GOLD, MAGENTA } from "../../design/theme";
import { LayersFigure } from "../figures/layers";
import { INTAKE, LAYERS_INTRO } from "../library-data";
import { H2, Lede, Panel, Section, dim, rise } from "../primitives";

export default function Layers() {
  return (
    <Section id="layers" eyebrow="the third layer">
      <H2>Truth, then knowledge, then judgment — and one gate into each.</H2>
      <Lede>{LAYERS_INTRO}</Lede>

      <Panel className="mt-12">
        <LayersFigure />
      </Panel>

      {/* the intake legend: one line per flow — the drawing made the case */}
      <motion.div
        variants={rise}
        className={`${FONT_MONO} mt-6 grid gap-x-10 gap-y-2 text-xs sm:grid-cols-2`}
      >
        {INTAKE.map((f) => (
          <div key={f.label} className="flex items-baseline gap-2.5">
            <span className="w-3 text-center" style={{ color: f.allowed ? GOLD : MAGENTA }}>
              {f.allowed ? "→" : "⨯"}
            </span>
            <span style={{ color: f.allowed ? dim(0.75) : MAGENTA }}>{f.label}</span>
            <span style={{ color: dim(0.35) }}>
              {f.allowed ? (f.gate ? `via ${f.gate}` : "automatic") : "does not exist"}
            </span>
          </div>
        ))}
      </motion.div>
    </Section>
  );
}
