"use client";

/*
 * Shared building blocks of the /library page: the band tones, the motion
 * variant, the status stamp, and the section scaffolding (Section, H2, Lede,
 * Panel). Every section component composes these; none redefines them.
 */

import { motion } from "framer-motion";

import { FONT_MONO, GOLD, LABEL, band, bandGlow } from "../design/theme";
import { STATUS_LABEL, type Status } from "./library-data";

/** The Library's signature tone: theta, the divergence band. */
export const THETA = band("theta", 72);
export const THETA_GLOW = bandGlow("theta");
export const MINT = band("beta");
export const ALPHA = band("alpha");
export const INK = "#e9edff";
export const dim = (a: number) => `rgba(233,237,255,${a})`;

export const rise = {
  hidden: { opacity: 0, y: 16 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.5, ease: [0.2, 0.7, 0.3, 1] as const } },
};

/* Status stamps. Shipped is mint; roadmap a dashed cyan outline — this page is
   mostly roadmap and must LOOK it, or the one shipped stamp means nothing. */
export const STATUS_TONE: Record<Status, string> = {
  shipped: MINT,
  in_progress: GOLD,
  roadmap: ALPHA,
};

export const STATUS_GLYPH: Record<Status, string> = {
  shipped: "●",
  in_progress: "◐",
  roadmap: "○",
};

export function Stamp({ status, className = "" }: { status: Status; className?: string }) {
  const tone = STATUS_TONE[status];
  const roadmap = status === "roadmap";
  return (
    <span
      className={`${FONT_MONO} inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-1 text-[10px] uppercase tracking-[0.14em] ${className}`}
      style={{
        borderColor: tone,
        borderStyle: roadmap ? "dashed" : "solid",
        color: tone,
        background: roadmap ? "transparent" : `${tone.replace(", 1)", ", 0.08)")}`,
        opacity: roadmap ? 0.85 : 1,
      }}
    >
      {STATUS_GLYPH[status]} {STATUS_LABEL[status]}
    </span>
  );
}

export function Section({
  id,
  eyebrow,
  tone = THETA,
  children,
}: {
  id: string;
  eyebrow: string;
  tone?: string;
  children: React.ReactNode;
}) {
  return (
    <motion.section
      id={id}
      initial="hidden"
      whileInView="visible"
      viewport={{ once: true, amount: 0.1 }}
      variants={{ visible: { transition: { staggerChildren: 0.07 } } }}
      className="mx-auto max-w-6xl px-6 py-24 md:py-28"
    >
      <motion.div variants={rise} className={LABEL} style={{ color: tone }}>
        {eyebrow}
      </motion.div>
      {children}
    </motion.section>
  );
}

export function H2({ children }: { children: React.ReactNode }) {
  return (
    <motion.h2
      variants={rise}
      className="mt-4 max-w-3xl text-3xl font-semibold leading-[1.12] tracking-tight text-white md:text-[2.75rem]"
    >
      {children}
    </motion.h2>
  );
}

/** Body copy. Never smaller than this, never mono. */
export function Lede({ children }: { children: React.ReactNode }) {
  return (
    <motion.p
      variants={rise}
      className="mt-5 max-w-2xl text-base leading-relaxed"
      style={{ color: dim(0.62) }}
    >
      {children}
    </motion.p>
  );
}

/** The quiet panel every drawing sits on. */
export function Panel({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <motion.div
      variants={rise}
      className={`rounded-xl border p-5 md:p-8 ${className}`}
      style={{ borderColor: dim(0.1), background: "rgba(255,255,255,0.02)" }}
    >
      {children}
    </motion.div>
  );
}
