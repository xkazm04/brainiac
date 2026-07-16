"use client";

/* The close: the pulse line, the two doors out, and the footer. */

import Link from "next/link";
import { motion } from "framer-motion";

import { FONT_MONO, LABEL } from "../../design/theme";
import { THETA, THETA_GLOW, dim } from "../primitives";

export default function Finale() {
  return (
    <>
      <section className="mx-auto max-w-6xl px-6 pb-32">
        <motion.div
          initial={{ opacity: 0, y: 18 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
          className="overflow-hidden rounded-2xl border p-12 text-center md:p-20"
          style={{
            borderColor: "hsla(224,90%,68%,0.3)",
            background: "radial-gradient(ellipse at 50% 0%, hsla(224,90%,60%,0.10), transparent 70%)",
          }}
        >
          <h2 className="mx-auto max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white md:text-[2.75rem]">
            A standard nobody measures is a wish.
            <br />
            <span style={{ color: THETA, textShadow: `0 0 40px ${THETA_GLOW}` }}>
              Give yours a pulse.
            </span>
          </h2>
          <p className="mx-auto mt-6 max-w-xl text-sm leading-relaxed" style={{ color: dim(0.5) }}>
            The memory layer underneath is shipped and measured. The knowledge base on top of it
            is shipped and measured. The Library&apos;s substrate and its distribution surface now
            run the same way — agents already fetch the org&apos;s rules and skills. What remains
            is the console, the mining, and the agents&apos; own proposals.
          </p>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
            <Link
              href="/demo?m=divergence"
              className={`${FONT_MONO} rounded-full px-7 py-3.5 text-sm font-semibold transition hover:scale-[1.03]`}
              style={{ background: THETA, color: "#060a1a", boxShadow: `0 0 40px ${THETA_GLOW}` }}
            >
              see the drift detector live →
            </Link>
            <Link
              href="/kb"
              className={`${FONT_MONO} rounded-full border px-7 py-3.5 text-sm transition hover:text-[#f3c74f]`}
              style={{ borderColor: dim(0.2), color: dim(0.7) }}
            >
              the layer below: the knowledge base
            </Link>
          </div>
        </motion.div>
      </section>

      <footer
        className={`${LABEL} mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 border-t px-6 py-8`}
        style={{ borderColor: dim(0.1), color: dim(0.32) }}
      >
        <span>brainiac · the library with a pulse</span>
        <span>every capability stamped: shipped · in progress · roadmap</span>
      </footer>
    </>
  );
}
