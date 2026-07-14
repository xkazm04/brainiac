"use client";

/*
 * LIMITS — "the balance sheet".
 *
 * Won the section prototype round against a decision-boundary variant.
 *
 * Credits and debits, side by side, in the ledger's own idiom (which is also the
 * hero's metaphor). Putting what it costs directly beside what it buys is the
 * format a skeptical reader is already running in their head, and it stops an
 * honest admission from looking like boilerplate in a grey box.
 */

import { motion } from "framer-motion";

import { band, FONT_MONO, LABEL, MAGENTA } from "../../design/theme";
import { WEAKNESSES } from "../pitch-data";

const GOLD = band("gamma");
const dim = (a: number) => `rgba(233,237,255,${a})`;

const rise = {
  hidden: { opacity: 0, y: 14 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.45 } },
};

/** The other column. Deliberately short: the debits are the point of this page. */
const CREDITS = [
  {
    title: "Knowledge crosses team boundaries.",
    body: "The case a per-repo file structurally cannot serve, and the one our trial actually won.",
  },
  {
    title: "A reversal stops being served.",
    body: "Supersession is a column, so the stale claim dies the moment the new one lands.",
  },
  {
    title: "Every claim has an author.",
    body: "Who asserted it, from which session, with which model. You can ask, and get an answer.",
  },
  {
    title: "The database enforces who may read.",
    body: "Not a filter your application remembers to pass.",
  },
];

export function LimitsBalanceSheet() {
  return (
    <motion.div
      variants={rise}
      initial="hidden"
      whileInView="visible"
      viewport={{ once: true, amount: 0.15 }}
      className="overflow-hidden rounded-xl border"
      style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
    >
      <div className="grid md:grid-cols-2">
        {/* credits */}
        <div className="border-b p-8 md:border-b-0 md:border-r" style={{ borderColor: "rgba(233,237,255,0.08)" }}>
          <div className={LABEL} style={{ color: GOLD }}>
            what it buys you
          </div>
          <div className="mt-6 space-y-5">
            {CREDITS.map((c) => (
              <div key={c.title} className="flex gap-3">
                <span className={`${FONT_MONO} shrink-0 text-sm`} style={{ color: GOLD }}>
                  +
                </span>
                <div>
                  <div className="text-sm font-medium text-white">{c.title}</div>
                  <p className="mt-1 text-sm leading-relaxed" style={{ color: dim(0.55) }}>
                    {c.body}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* debits — the reason this section exists */}
        <div className="p-8" style={{ background: "rgba(255,93,162,0.03)" }}>
          <div className={LABEL} style={{ color: MAGENTA }}>
            what it costs you
          </div>
          <div className="mt-6 space-y-5">
            {WEAKNESSES.map((w) => (
              <div key={w.title} className="flex gap-3">
                <span className={`${FONT_MONO} shrink-0 text-sm`} style={{ color: MAGENTA }}>
                  −
                </span>
                <div>
                  <div className="text-sm font-medium text-white">{w.title}</div>
                  <p className="mt-1 text-sm leading-relaxed" style={{ color: dim(0.55) }}>
                    {w.body}
                  </p>
                  <div className={`${LABEL} mt-1.5`} style={{ color: dim(0.32) }}>
                    {w.metric}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* the net line */}
      <div
        className="flex flex-wrap items-center justify-between gap-3 border-t px-8 py-5"
        style={{ borderColor: "rgba(233,237,255,0.08)", background: "rgba(0,0,0,0.25)" }}
      >
        <span className={LABEL} style={{ color: dim(0.35) }}>
          net
        </span>
        <span className="text-sm" style={{ color: dim(0.8) }}>
          Positive exactly where knowledge crosses a boundary. Negative where a text file
          already wins.
        </span>
      </div>
    </motion.div>
  );
}
