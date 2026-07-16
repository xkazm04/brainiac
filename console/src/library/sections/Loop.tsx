"use client";

/* Move 4 — the life of a rule: the whole loop as one rail, stations lit only
   where they actually run. */

import { motion } from "framer-motion";

import { FONT_MONO, GOLD } from "../../design/theme";
import { LifeRail } from "../figures/rail";
import { LOOP_LEDE, RULE_STAGES } from "../library-data";
import { H2, Lede, MINT, Panel, Section, Stamp, dim, rise } from "../primitives";

export default function Loop() {
  return (
    <Section id="loop" eyebrow="the life of a rule" tone={GOLD}>
      <H2>Nobody schedules a standards review. The loop runs.</H2>
      <Lede>{LOOP_LEDE}</Lede>

      <Panel className="mt-12">
        <LifeRail statuses={RULE_STAGES.map((s) => s.status)} />
        <div className="mt-8 grid gap-x-8 gap-y-6 sm:grid-cols-2 lg:grid-cols-3">
          {RULE_STAGES.map((s) => (
            <div key={s.n}>
              <div className="flex flex-wrap items-center gap-2.5">
                <span className={`${FONT_MONO} text-xs`} style={{ color: dim(0.3) }}>
                  {s.n}
                </span>
                <span
                  className="text-base font-semibold tracking-tight"
                  style={{ color: s.status === "shipped" ? MINT : "#fff" }}
                >
                  {s.name}
                </span>
                <Stamp status={s.status} />
              </div>
              <p className="mt-2 text-sm leading-relaxed" style={{ color: dim(0.55) }}>
                {s.body}
              </p>
            </div>
          ))}
        </div>
      </Panel>

      <motion.p variants={rise} className={`${FONT_MONO} mt-4 text-xs`} style={{ color: dim(0.38) }}>
        station 01 is the drift detector shipped with the standards board · 06 loops back into
        01 — a retired rule is a practice worth listening for again
      </motion.p>
    </Section>
  );
}
