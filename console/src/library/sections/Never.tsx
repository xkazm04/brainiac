"use client";

/* Move 6 — the refusals: four switch positions with no lever fitted. */

import { motion } from "framer-motion";

import { MAGENTA } from "../../design/theme";
import { NoLevers } from "../figures/never";
import { NEVER } from "../library-data";
import { H2, Lede, Section, dim, rise } from "../primitives";

export default function Never() {
  return (
    <Section id="never" eyebrow="what it will never do" tone={MAGENTA}>
      <H2>The refusals are the feature.</H2>
      <Lede>
        Each of these is a thing a competitor could ship next quarter and call an
        improvement. Each one would kill the trust the telemetry runs on — and a library
        nobody trusts is a folder of opinions again.
      </Lede>

      <motion.div variants={rise} className="mt-12 grid items-center gap-10 lg:grid-cols-[0.85fr_1.15fr]">
        <NoLevers />
        <div className="space-y-6">
          {NEVER.map((n) => (
            <div key={n.title}>
              <h3 className="text-base font-semibold tracking-tight" style={{ color: MAGENTA }}>
                {n.title}
              </h3>
              <p className="mt-1.5 max-w-xl text-sm leading-relaxed" style={{ color: dim(0.58) }}>
                {n.body}
              </p>
            </div>
          ))}
        </div>
      </motion.div>
    </Section>
  );
}
