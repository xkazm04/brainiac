"use client";

/* Move 1 — the drift: the hero. Two practices leave the written standard and
   nothing measures the gap; the thesis line closes the move. */

import { motion } from "framer-motion";

import { LABEL, MAGENTA } from "../../design/theme";
import { DriftBeat } from "../figures/hero";
import { DRIFT_CAPTION, THESIS, THESIS_BODY } from "../library-data";
import { Stamp, THETA, dim } from "../primitives";

export default function Drift() {
  return (
    <section id="drift" className="mx-auto max-w-6xl px-6 pb-6 pt-12 md:pt-16">
      <div className="grid items-center gap-10 lg:grid-cols-[1fr_1.15fr]">
        <motion.div initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.8 }}>
          <div className="flex flex-wrap items-center gap-3">
            <div className={LABEL} style={{ color: THETA }}>
              the library · standards + skills
            </div>
            <Stamp status="shipped" />
            <Stamp status="roadmap" className="opacity-90" />
          </div>
          <h1 className="mt-6 text-[2.4rem] font-semibold leading-[1.06] tracking-tight text-white lg:text-[3.2rem]">
            Your standards are <span style={{ color: MAGENTA }}>wishes</span>.
            <br />
            Practice is the truth.
          </h1>
          <p className="mt-6 max-w-lg text-base leading-relaxed" style={{ color: dim(0.62) }}>
            {THESIS_BODY}
          </p>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 14 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.8, delay: 0.15 }}
          className="rounded-xl border p-5 md:p-7"
          style={{ borderColor: dim(0.1), background: "rgba(255,255,255,0.02)" }}
        >
          <DriftBeat />
          <p className="mt-4 text-base font-medium leading-snug tracking-tight text-white">
            {DRIFT_CAPTION}
          </p>
        </motion.div>
      </div>

      <motion.p
        initial={{ opacity: 0 }}
        whileInView={{ opacity: 1 }}
        viewport={{ once: true }}
        transition={{ duration: 0.7 }}
        className="mt-14 max-w-3xl border-l-2 pl-6 text-xl font-medium leading-snug tracking-tight text-white md:text-2xl"
        style={{ borderColor: THETA }}
      >
        {THESIS}
      </motion.p>
    </section>
  );
}
