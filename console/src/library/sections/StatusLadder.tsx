"use client";

/* Move 7 — the honest ladder: every phase stamped, the stamps pinned to the
   build plan's status log by tests. */

import { motion } from "framer-motion";

import { FONT_MONO } from "../../design/theme";
import { CHECK_US, LADDER } from "../library-data";
import { H2, Lede, STATUS_GLYPH, STATUS_TONE, Section, Stamp, dim, rise } from "../primitives";

export default function StatusLadder() {
  return (
    <Section id="status" eyebrow="what is actually built" tone={dim(0.6)}>
      <H2>The status of every phase — almost all of it roadmap, on purpose.</H2>
      <Lede>{CHECK_US}</Lede>

      <div className="relative mt-14">
        <span aria-hidden className="absolute bottom-3 left-[13px] top-3 w-px" style={{ background: dim(0.12) }} />
        <div className="space-y-10">
          {LADDER.map((p) => {
            const tone = STATUS_TONE[p.status];
            return (
              <motion.div key={p.id} variants={rise} className="relative flex gap-6">
                <span
                  className={`${FONT_MONO} z-10 flex h-7 w-7 shrink-0 items-center justify-center rounded-full border text-[10px]`}
                  style={{
                    borderColor: tone,
                    color: tone,
                    background: "#08070c",
                    borderStyle: p.status === "roadmap" ? "dashed" : "solid",
                  }}
                >
                  {STATUS_GLYPH[p.status]}
                </span>
                <div className="min-w-0 flex-1 pb-2">
                  <div className="flex flex-wrap items-center gap-3">
                    <span className="text-lg font-semibold tracking-tight" style={{ color: tone }}>
                      {p.id}
                    </span>
                    <span className="text-lg font-medium tracking-tight text-white">{p.name}</span>
                    <Stamp status={p.status} />
                  </div>
                  <p className="mt-2 max-w-3xl text-sm leading-relaxed" style={{ color: dim(0.6) }}>
                    {p.body}
                  </p>
                </div>
              </motion.div>
            );
          })}
        </div>
      </div>
    </Section>
  );
}
