"use client";

/*
 * The knowledge base — the wiki that cannot rot.
 *
 * Composition rule, learned from the pitch: THE FIGURE IS THE SECTION. Every
 * move leads with a drawing that carries the idea; prose is a caption beside
 * it, never the argument. Body copy is never below text-sm and never mono —
 * the 10–11px mono sizes survive only inside LABEL microcopy, which is what
 * that size is legible for.
 *
 * The moves:
 *   1. rot        — the decay curve (labelled a schematic, because it is one)
 *   2. asymmetry  — the one-way relationship, drawn; a one-line legend
 *   3. anatomy    — five mechanisms, five drawings, alternating sides
 *   4. pipeline   — the whole rebuild as one rail, the breaker a literal gap
 *   5. publishing — one-way into the wiki you already read
 *   6. never      — four switch positions with no lever fitted
 *   7. status     — the honest ladder
 *
 * Chrome (nav rail, background, dividers) is shared with /pitch:
 * app/kb/layout.tsx mounts the SectionRail on the pitch-surface; Divider is the
 * shared brand hairline. Honesty + audience rules enforced by kb-data.test.ts.
 */

import Link from "next/link";
import { motion } from "framer-motion";

import { FONT_DISPLAY, FONT_MONO, GOLD, GOLD_GLOW, LABEL, MAGENTA, band } from "../design/theme";
import Divider from "../components/Divider";
import ProjectionDiagram from "./ProjectionDiagram";
import RotCurve from "./RotCurve";
import {
  ArtifactSurvives,
  GateMeter,
  LifecycleSplit,
  NoSwitches,
  OneWayPublish,
  PipelineFigure,
  PropagationSpark,
  RoundTripLoop,
} from "./illustrations";
import {
  ASYMMETRY,
  CHECK_US,
  COMPOSE_STAGES,
  CONFLUENCE,
  DIRTY_LOOP,
  LADDER,
  NEVER,
  PROPERTIES,
  ROT_CAPTION,
  SCOPES,
  STATUS_LABEL,
  THESIS,
  THESIS_BODY,
  type Status,
} from "./kb-data";

const MINT = band("beta");
const ALPHA = band("alpha");
const INK = "#e9edff";
const dim = (a: number) => `rgba(233,237,255,${a})`;

const rise = {
  hidden: { opacity: 0, y: 16 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.5, ease: [0.2, 0.7, 0.3, 1] as const } },
};

/* Status stamps. Shipped is mint, roadmap a dashed cyan outline. `built · not
   enabled` is gold and ringed rather than filled — the code IS there, nothing
   is running. Impossible to read as shipped, impossible to read as roadmap. */
const STATUS_TONE: Record<Status, string> = {
  shipped: MINT,
  built_off: GOLD,
  in_progress: GOLD,
  roadmap: ALPHA,
};

const STATUS_GLYPH: Record<Status, string> = {
  shipped: "●",
  built_off: "◍",
  in_progress: "◐",
  roadmap: "○",
};

function Stamp({ status, className = "" }: { status: Status; className?: string }) {
  const tone = STATUS_TONE[status];
  const roadmap = status === "roadmap";
  const off = status === "built_off";
  return (
    <span
      className={`${FONT_MONO} inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-1 text-[10px] uppercase tracking-[0.14em] ${className}`}
      style={{
        borderColor: tone,
        borderStyle: roadmap ? "dashed" : "solid",
        color: tone,
        background: roadmap || off ? "transparent" : `${tone.replace(", 1)", ", 0.08)")}`,
        opacity: roadmap ? 0.85 : 1,
      }}
    >
      {STATUS_GLYPH[status]} {STATUS_LABEL[status]}
    </span>
  );
}

function Section({
  id,
  eyebrow,
  tone = GOLD,
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

function H2({ children }: { children: React.ReactNode }) {
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
function Lede({ children }: { children: React.ReactNode }) {
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
function Panel({ children, className = "" }: { children: React.ReactNode; className?: string }) {
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

const PROPERTY_ART: Record<string, React.ReactNode> = {
  projection: <PropagationSpark />,
  lifecycle: <LifecycleSplit />,
  structure: <ArtifactSurvives />,
  "health-gate": <GateMeter />,
  "round-trip": <RoundTripLoop />,
};

export default function KnowledgeBase() {
  return (
    <div className={FONT_DISPLAY} style={{ color: INK }}>
      {/* ─── 1. THE ROT ──────────────────────────────────────────────────── */}
      <section id="rot" className="mx-auto max-w-6xl px-6 pb-6 pt-12 md:pt-16">
        <div className="grid items-center gap-10 lg:grid-cols-[1fr_1.15fr]">
          <motion.div initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.8 }}>
            <div className="flex flex-wrap items-center gap-3">
              <div className={LABEL} style={{ color: ALPHA }}>
                the document layer
              </div>
              <Stamp status="shipped" />
              <Stamp status="built_off" className="opacity-90" />
            </div>
            <h1 className="mt-6 text-[2.4rem] font-semibold leading-[1.06] tracking-tight text-white lg:text-[3.2rem]">
              Your wiki is a{" "}
              <span style={{ color: MAGENTA }}>second source of truth</span>.
              <br />
              That is why it rots.
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
            <RotCurve />
            <p className="mt-4 text-base font-medium leading-snug tracking-tight text-white">
              {ROT_CAPTION}
            </p>
          </motion.div>
        </div>

        <motion.p
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ duration: 0.7 }}
          className="mt-14 max-w-3xl border-l-2 pl-6 text-xl font-medium leading-snug tracking-tight text-white md:text-2xl"
          style={{ borderColor: GOLD }}
        >
          {THESIS}
        </motion.p>
      </section>

      <Divider />

      {/* ─── 2. THE ASYMMETRY ────────────────────────────────────────────── */}
      <Section id="asymmetry" eyebrow="the asymmetry" tone={ALPHA}>
        <H2>Truth flows one way. There is no way back except through a human.</H2>

        <Panel className="mt-12">
          <ProjectionDiagram />
        </Panel>

        {/* the legend: one line per flow — the drawing already made the case */}
        <motion.div
          variants={rise}
          className={`${FONT_MONO} mt-6 grid gap-x-10 gap-y-2 text-xs sm:grid-cols-2`}
        >
          {ASYMMETRY.map((f) => (
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

      <Divider tone={MINT} />

      {/* ─── 3. THE ANATOMY ──────────────────────────────────────────────── */}
      <Section id="anatomy" eyebrow="the anatomy" tone={MINT}>
        <H2>A page that cannot rot, part by part.</H2>
        <Lede>
          Five mechanisms, drawn. Four run today; the breaker is built, tested, and switched
          off — and its stamp says so.
        </Lede>

        <div className="mt-14 space-y-6">
          {PROPERTIES.map((p, i) => (
            <motion.article
              key={p.key}
              variants={rise}
              className="grid items-center gap-8 rounded-xl border p-7 md:p-8 lg:grid-cols-[1.05fr_1fr]"
              style={{
                borderColor: p.status === "shipped" ? "hsla(158,90%,68%,0.18)" : "hsla(46,90%,68%,0.25)",
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

      <Divider />

      {/* ─── 4. THE REBUILD ──────────────────────────────────────────────── */}
      <Section id="pipeline" eyebrow="how it rebuilds">
        <H2>Nobody schedules a doc review. There is nothing to review.</H2>
        <Lede>{DIRTY_LOOP}</Lede>

        <Panel className="mt-12">
          <PipelineFigure stages={COMPOSE_STAGES} />
        </Panel>

        <motion.p variants={rise} className={`${FONT_MONO} mt-4 text-xs`} style={{ color: dim(0.38) }}>
          hover a station for what it does · 01–04 run on every change · 05–06 pass their tests
          and stay dark until the org flips the switch
        </motion.p>
      </Section>

      <Divider tone={ALPHA} />

      {/* ─── 5. PUBLISHING ───────────────────────────────────────────────── */}
      <Section id="publishing" eyebrow="publishing · built, not enabled" tone={ALPHA}>
        <H2>{CONFLUENCE.headline}</H2>
        <Lede>{CONFLUENCE.body}</Lede>

        <Panel className="mt-12">
          <OneWayPublish />
        </Panel>

        {/* the three invariants — the drawing shows them; these name them */}
        <motion.div variants={rise} className="mt-6 grid gap-x-10 gap-y-3 md:grid-cols-3">
          {CONFLUENCE.invariants.map((inv) => (
            <div key={inv.title}>
              <div className={LABEL} style={{ color: ALPHA }}>
                {inv.title}
              </div>
              <p className="mt-2 text-sm leading-relaxed" style={{ color: dim(0.55) }}>
                {inv.body}
              </p>
            </div>
          ))}
        </motion.div>

        {/* turning it on: one switch, two keys */}
        <motion.div
          variants={rise}
          className="mt-12 rounded-xl border p-7"
          style={{ borderColor: "hsla(46,90%,68%,0.2)", background: "hsla(46,90%,60%,0.02)" }}
        >
          <div className="flex flex-wrap items-center gap-3">
            <div className={LABEL} style={{ color: GOLD }}>
              turning it on
            </div>
            <Stamp status={SCOPES.status} />
          </div>
          <p className="mt-3 max-w-3xl text-sm leading-relaxed" style={{ color: dim(0.6) }}>
            {SCOPES.body}
          </p>
          <div className="mt-5 grid gap-5 md:grid-cols-2">
            {SCOPES.rows.map((r) => (
              <div key={r.scope} className="flex items-start gap-4">
                <span
                  className={`${FONT_MONO} shrink-0 rounded-md border px-2.5 py-1.5 text-xs`}
                  style={{ borderColor: "hsla(46,90%,68%,0.35)", color: GOLD, background: "hsla(46,90%,60%,0.05)" }}
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

      <Divider tone={MAGENTA} />

      {/* ─── 6. NEVER ────────────────────────────────────────────────────── */}
      <Section id="never" eyebrow="what it will never do" tone={MAGENTA}>
        <H2>The refusals are the feature.</H2>
        <Lede>
          Every one of these is a thing a competitor could ship next quarter and call an
          improvement. Each one would put the rot back.
        </Lede>

        <motion.div variants={rise} className="mt-12 grid items-center gap-10 lg:grid-cols-[0.85fr_1.15fr]">
          <NoSwitches />
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

      <Divider />

      {/* ─── 7. THE LADDER ───────────────────────────────────────────────── */}
      <Section id="status" eyebrow="what is actually built" tone={dim(0.6)}>
        <H2>The status of every phase — including the one we built and switched off.</H2>
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
                      borderStyle: p.status === "built_off" ? "double" : "solid",
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
                    {p.stats && (
                      <div className="mt-3 flex flex-wrap gap-2">
                        {p.stats.map((s) => (
                          <span
                            key={s}
                            className={`${FONT_MONO} rounded-full border px-2.5 py-1 text-[10px]`}
                            style={{ borderColor: "hsla(158,90%,68%,0.3)", color: dim(0.65) }}
                          >
                            {s}
                          </span>
                        ))}
                      </div>
                    )}
                    {p.gate && (
                      <p className="mt-3 max-w-3xl text-sm leading-relaxed" style={{ color: dim(0.42) }}>
                        {p.gate}
                      </p>
                    )}
                  </div>
                </motion.div>
              );
            })}
          </div>
        </div>
      </Section>

      {/* ─── CTA ─────────────────────────────────────────────────────────── */}
      <section className="mx-auto max-w-6xl px-6 pb-32">
        <motion.div
          initial={{ opacity: 0, y: 18 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
          className="overflow-hidden rounded-2xl border p-12 text-center md:p-20"
          style={{
            borderColor: "hsla(46,90%,68%,0.3)",
            background: "radial-gradient(ellipse at 50% 0%, hsla(46,90%,60%,0.10), transparent 70%)",
          }}
        >
          <h2 className="mx-auto max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white md:text-[2.75rem]">
            The page was stale the day after it was written.
            <br />
            <span style={{ color: GOLD, textShadow: `0 0 40px ${GOLD_GLOW}` }}>
              Stop writing pages.
            </span>
          </h2>
          <p className="mx-auto mt-6 max-w-xl text-sm leading-relaxed" style={{ color: dim(0.5) }}>
            The memory layer underneath is shipped and measured. The document layer on top is
            built and measured the same way. Publishing is the one switch still off, on purpose.
          </p>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
            <Link
              href="/pitch"
              className={`${FONT_MONO} rounded-full px-7 py-3.5 text-sm font-semibold transition hover:scale-[1.03]`}
              style={{ background: GOLD, color: "#1a1405", boxShadow: `0 0 40px ${GOLD_GLOW}` }}
            >
              the full case →
            </Link>
            <Link
              href="/health"
              className={`${FONT_MONO} rounded-full border px-7 py-3.5 text-sm transition hover:text-[#f3c74f]`}
              style={{ borderColor: dim(0.2), color: dim(0.7) }}
            >
              the health score that gates publishing
            </Link>
          </div>
        </motion.div>
      </section>

      <footer
        className={`${LABEL} mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 border-t px-6 py-8`}
        style={{ borderColor: dim(0.1), color: dim(0.32) }}
      >
        <span>brainiac · the wiki that cannot rot</span>
        <span>every capability stamped: shipped · built, not enabled · roadmap</span>
      </footer>
    </div>
  );
}
