"use client";

/*
 * The knowledge base — the wiki that cannot rot.
 *
 * Told in seven moves, each led by a DRAWING with text as commentary — the
 * page practices what it preaches: concepts are compiled into diagrams the way
 * the product compiles knowledge into pages, and prose only carries what a
 * drawing cannot.
 *
 *   1. the thesis      — the decay curve, drawn (and labelled a schematic)
 *   2. the asymmetry   — the one-way relationship, drawn
 *   3. the properties  — five claims, each with its own mechanism sketch
 *   4. the pipeline    — six stages on a rail, the breaker visibly off
 *   5. publishing      — one-way into the wiki you already read, drawn
 *   6. never           — the refusals
 *   7. the ladder      — the honest status of every phase
 *
 * Honesty rule enforced by kb-data.test.ts: every capability carries a status
 * stamp and nothing is overstated or understated. Audience rule enforced by the
 * same tests: no file paths, no internal table names — a visitor cannot open
 * the repo mid-sentence, so the page never asks them to.
 */

import Link from "next/link";
import { motion, useReducedMotion, useScroll, useSpring } from "framer-motion";

import { BG, FONT_DISPLAY, FONT_MONO, GOLD, GOLD_GLOW, LABEL, MAGENTA, band } from "../design/theme";
import ProjectionDiagram from "./ProjectionDiagram";
import RotCurve from "./RotCurve";
import {
  ArtifactSurvives,
  GateMeter,
  LifecycleSplit,
  OneWayPublish,
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
  hidden: { opacity: 0, y: 18 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.55, ease: [0.2, 0.7, 0.3, 1] as const } },
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
  eyebrow,
  tone = GOLD,
  children,
  id,
}: {
  eyebrow: string;
  tone?: string;
  children: React.ReactNode;
  id?: string;
}) {
  return (
    <motion.section
      id={id}
      initial="hidden"
      whileInView="visible"
      viewport={{ once: true, amount: 0.12 }}
      variants={{ visible: { transition: { staggerChildren: 0.08 } } }}
      className="relative mx-auto max-w-6xl px-6 py-24 md:py-28"
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
      className="mt-4 max-w-3xl text-4xl font-semibold leading-[1.1] tracking-tight text-white md:text-5xl"
    >
      {children}
    </motion.h2>
  );
}

function Lede({ children }: { children: React.ReactNode }) {
  return (
    <motion.p
      variants={rise}
      className={`${FONT_MONO} mt-5 max-w-2xl text-sm leading-relaxed`}
      style={{ color: dim(0.6) }}
    >
      {children}
    </motion.p>
  );
}

/* Panel: the shared quiet surface every drawing sits on. */
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

/* The property cards each lead with their mechanism drawing. */
const PROPERTY_ART: Record<string, React.ReactNode> = {
  projection: <PropagationSpark />,
  lifecycle: <LifecycleSplit />,
  structure: <ArtifactSurvives />,
  "health-gate": <GateMeter />,
  "round-trip": <RoundTripLoop />,
};

/* One stage on the compose rail. */
function StageNode({
  n,
  name,
  status,
  body,
  index,
  pulseCount,
}: {
  n: string;
  name: string;
  status: Status;
  body: string;
  index: number;
  pulseCount: number;
}) {
  const reduce = !!useReducedMotion();
  const tone = STATUS_TONE[status];
  const beat = 0.7;
  return (
    <div className="relative p-5" style={{ background: BG }}>
      <div className="flex items-center gap-3">
        <span className="relative flex h-9 w-9 shrink-0 items-center justify-center">
          <span
            className={`${FONT_MONO} flex h-9 w-9 items-center justify-center rounded-full border text-[10px]`}
            style={{ borderColor: tone, color: tone, borderStyle: status === "built_off" ? "double" : "solid" }}
          >
            {n}
          </span>
          {!reduce && (
            <motion.span
              className="absolute inset-0 rounded-full border"
              style={{ borderColor: tone }}
              initial={{ opacity: 0, scale: 1 }}
              animate={{ opacity: [0, 0.7, 0], scale: [1, 1.5, 1.8] }}
              transition={{
                duration: beat,
                delay: index * beat,
                repeat: Infinity,
                repeatDelay: pulseCount * beat - beat,
                ease: "easeOut",
              }}
            />
          )}
        </span>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold tracking-tight text-white">{name}</div>
          <div className={`${FONT_MONO} text-[9px] uppercase tracking-[0.14em]`} style={{ color: tone }}>
            {STATUS_GLYPH[status]} {STATUS_LABEL[status]}
          </div>
        </div>
      </div>
      <p className={`${FONT_MONO} mt-3 text-[11px] leading-relaxed`} style={{ color: dim(0.55) }}>
        {body}
      </p>
    </div>
  );
}

export default function KnowledgeBase() {
  const { scrollYProgress } = useScroll();
  const progress = useSpring(scrollYProgress, { stiffness: 90, damping: 30 });

  const railStages = COMPOSE_STAGES.slice(0, 4);
  const breakerStages = COMPOSE_STAGES.slice(4);

  return (
    <div className={`${FONT_DISPLAY} min-h-screen`} style={{ background: BG, color: INK }}>
      <motion.div
        aria-hidden
        className="fixed left-0 top-0 z-50 h-[2px] w-full origin-left"
        style={{ scaleX: progress, background: ALPHA, boxShadow: `0 0 12px ${ALPHA}` }}
      />

      {/* ─── header ──────────────────────────────────────────────────────── */}
      <header className="mx-auto flex max-w-6xl items-center justify-between px-6 py-6">
        <Link href="/" className="flex items-center gap-3">
          <span className="text-xl font-semibold tracking-tight text-white">Brainiac</span>
          <span className={LABEL} style={{ color: ALPHA }}>
            α · knowledge base
          </span>
        </Link>
        <nav
          className={`${FONT_MONO} hidden gap-6 text-xs uppercase tracking-widest md:flex`}
          style={{ color: dim(0.45) }}
        >
          <a href="#asymmetry" className="transition hover:text-[#f3c74f]">the asymmetry</a>
          <a href="#pipeline" className="transition hover:text-[#f3c74f]">how a page is built</a>
          <a href="#status" className="transition hover:text-[#f3c74f]">what is actually built</a>
          <Link href="/" className="transition hover:text-[#f3c74f]">the pitch →</Link>
        </nav>
      </header>

      {/* ─── 1. THE THESIS + THE DECAY CURVE ─────────────────────────────── */}
      <section className="mx-auto max-w-6xl px-6 pb-8 pt-10 md:pt-16">
        <motion.div initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.8 }}>
          <div className="flex flex-wrap items-center gap-3">
            <div className={LABEL} style={{ color: ALPHA }}>
              the document layer
            </div>
            <Stamp status="shipped" />
            <Stamp status="built_off" className="opacity-90" />
          </div>
          <h1 className="mt-6 max-w-4xl text-[2.6rem] font-semibold leading-[1.06] tracking-tight text-white lg:text-6xl">
            Your wiki is a{" "}
            <span style={{ color: MAGENTA }}>second source of truth</span>.
            <br />
            That is why it rots.
          </h1>
          <p className={`${FONT_MONO} mt-6 max-w-2xl text-sm leading-relaxed`} style={{ color: dim(0.65) }}>
            {THESIS_BODY}
          </p>

          <div
            className="mt-10 rounded-xl border p-5 md:p-8"
            style={{ borderColor: dim(0.1), background: "rgba(255,255,255,0.02)" }}
          >
            <RotCurve />
            <p className="mt-4 max-w-2xl text-base font-medium leading-snug tracking-tight text-white md:text-lg">
              {ROT_CAPTION}
            </p>
          </div>

          <p
            className="mt-10 max-w-3xl border-l-2 pl-6 text-xl font-medium leading-snug tracking-tight text-white md:text-2xl"
            style={{ borderColor: GOLD }}
          >
            {THESIS}
          </p>
          <p className={`${FONT_MONO} mt-8 max-w-2xl text-xs leading-relaxed`} style={{ color: dim(0.4) }}>
            {CHECK_US}
          </p>
        </motion.div>
      </section>

      {/* ─── 2. THE ASYMMETRY ────────────────────────────────────────────── */}
      <Section eyebrow="the asymmetry" id="asymmetry" tone={ALPHA}>
        <H2>Truth flows one way. There is no way back except through a human.</H2>
        <Lede>
          Memories compose into pages. Pages never write back — a human edit re-enters through
          extraction and faces the same review gate as any agent proposal.
        </Lede>

        <Panel className="mt-12">
          <ProjectionDiagram />
        </Panel>

        {/* the diagram's legend — one line per flow, not four paragraphs */}
        <motion.div variants={rise} className="mt-8 grid gap-x-10 gap-y-4 md:grid-cols-2">
          {ASYMMETRY.map((f) => (
            <div key={f.label} className="flex gap-3">
              <span
                className={`${FONT_MONO} mt-[1px] w-4 shrink-0 text-center text-sm`}
                style={{ color: f.allowed ? GOLD : MAGENTA }}
              >
                {f.allowed ? "→" : "⨯"}
              </span>
              <div>
                <span className="text-sm font-semibold tracking-tight" style={{ color: f.allowed ? "#fff" : MAGENTA }}>
                  {f.label}
                  {!f.allowed && " — never"}
                </span>
                {f.gate && (
                  <span className={`${FONT_MONO} ml-2 text-[10px] uppercase tracking-[0.14em]`} style={{ color: GOLD }}>
                    via {f.gate}
                  </span>
                )}
                <p className={`${FONT_MONO} mt-1 text-[11px] leading-relaxed`} style={{ color: dim(0.5) }}>
                  {f.note}
                </p>
              </div>
            </div>
          ))}
        </motion.div>
      </Section>

      {/* ─── 3. THE PROPERTIES ───────────────────────────────────────────── */}
      <Section eyebrow="five properties · five honest stamps" tone={MINT}>
        <H2>What makes a page that cannot rot.</H2>
        <Lede>
          Each mechanism, drawn. Four are shipped and measured; the breaker is built, tested, and
          deliberately switched off — the stamp says which is which.
        </Lede>

        <div className="mt-14 grid gap-6 md:grid-cols-2">
          {PROPERTIES.map((p) => {
            const wide = p.key === "round-trip";
            return (
              <motion.div
                key={p.key}
                variants={rise}
                className={`flex flex-col rounded-xl border p-6 ${wide ? "md:col-span-2 md:flex-row md:items-center md:gap-8" : ""}`}
                style={{
                  borderColor: p.status === "shipped" ? "hsla(158,90%,68%,0.25)" : "rgba(233,237,255,0.10)",
                  background: p.status === "shipped" ? "hsla(158,90%,60%,0.025)" : "rgba(255,255,255,0.02)",
                }}
              >
                <div className={wide ? "md:w-1/2" : ""}>{PROPERTY_ART[p.key]}</div>
                <div className={wide ? "mt-5 md:mt-0 md:w-1/2" : "mt-5"}>
                  <div className="flex items-start justify-between gap-4">
                    <h3 className="text-lg font-semibold tracking-tight text-white">{p.title}</h3>
                    <Stamp status={p.status} />
                  </div>
                  <div className={`${FONT_MONO} mt-2 text-xs`} style={{ color: STATUS_TONE[p.status] }}>
                    {p.claim}
                  </div>
                  <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                    {p.body}
                  </p>
                  <div className={`${FONT_MONO} mt-4 text-[10px] leading-relaxed`} style={{ color: dim(0.35) }}>
                    verified: {p.evidence}
                  </div>
                </div>
              </motion.div>
            );
          })}
        </div>
      </Section>

      {/* ─── 4. THE PIPELINE RAIL ────────────────────────────────────────── */}
      <Section eyebrow="how a page is built" id="pipeline">
        <H2>Nobody schedules a doc review. There is nothing to review.</H2>
        <Lede>{DIRTY_LOOP}</Lede>

        <motion.div variants={rise} className="mt-14 flex flex-col gap-6 lg:flex-row lg:items-stretch">
          {/* the four running stages */}
          <div
            className="grid flex-[2] gap-px overflow-hidden rounded-xl border sm:grid-cols-2 lg:grid-cols-4"
            style={{ borderColor: dim(0.1), background: dim(0.08) }}
          >
            {railStages.map((s, i) => (
              <StageNode key={s.n} {...s} index={i} pulseCount={COMPOSE_STAGES.length} />
            ))}
          </div>

          {/* the breaker — where the rail crosses to the outside world */}
          <div className="flex items-center gap-2 lg:flex-col lg:justify-center lg:px-1">
            <span className="h-px flex-1 border-t border-dashed lg:h-auto lg:w-px lg:flex-1 lg:border-l lg:border-t-0" style={{ borderColor: GOLD }} />
            <span className={`${FONT_MONO} whitespace-nowrap text-[9px] uppercase tracking-[0.2em] lg:[writing-mode:vertical-rl]`} style={{ color: GOLD }}>
              the breaker
            </span>
            <span className="h-px flex-1 border-t border-dashed lg:h-auto lg:w-px lg:flex-1 lg:border-l lg:border-t-0" style={{ borderColor: GOLD }} />
          </div>

          {/* the two external stages — built, and off */}
          <div
            className="grid flex-1 gap-px overflow-hidden rounded-xl border sm:grid-cols-2"
            style={{ borderColor: "hsla(46,90%,68%,0.3)", background: dim(0.08) }}
          >
            {breakerStages.map((s, i) => (
              <StageNode key={s.n} {...s} index={i + railStages.length} pulseCount={COMPOSE_STAGES.length} />
            ))}
          </div>
        </motion.div>

        <motion.p variants={rise} className={`${FONT_MONO} mt-5 text-[11px]`} style={{ color: dim(0.38) }}>
          stages 01–04 run today on every change · stages 05–06 exist, pass their tests, and stay
          dark until the org flips the switch
        </motion.p>
      </Section>

      {/* ─── 5. PUBLISHING ───────────────────────────────────────────────── */}
      <Section eyebrow="publishing · built, not enabled" tone={ALPHA}>
        <div className="mt-4 flex flex-wrap items-center gap-3">
          <Stamp status={CONFLUENCE.status} />
          <span className={`${FONT_MONO} text-xs`} style={{ color: dim(0.4) }}>
            merged and tested — switched on per organization, never by an upgrade
          </span>
        </div>
        <H2>{CONFLUENCE.headline}</H2>
        <Lede>{CONFLUENCE.body}</Lede>

        <Panel className="mt-12">
          <OneWayPublish />
        </Panel>

        <div className="mt-8 grid gap-6 md:grid-cols-3">
          {CONFLUENCE.invariants.map((inv) => (
            <motion.div
              key={inv.title}
              variants={rise}
              className="rounded-xl border p-5"
              style={{ borderColor: "hsla(190,90%,68%,0.22)", background: "rgba(255,255,255,0.015)" }}
            >
              <div className={LABEL} style={{ color: ALPHA }}>
                {inv.title}
              </div>
              <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.55) }}>
                {inv.body}
              </p>
            </motion.div>
          ))}
        </div>

        {/* scoping — two keys, one switch */}
        <motion.div
          variants={rise}
          className="mt-10 rounded-xl border p-7"
          style={{ borderColor: dim(0.12), background: "rgba(255,255,255,0.02)" }}
        >
          <div className="flex flex-wrap items-center gap-3">
            <div className={LABEL} style={{ color: GOLD }}>
              turning it on
            </div>
            <Stamp status={SCOPES.status} />
          </div>
          <p className={`${FONT_MONO} mt-4 max-w-3xl text-xs leading-relaxed`} style={{ color: dim(0.6) }}>
            {SCOPES.body}
          </p>
          <div className="mt-6 grid gap-4 md:grid-cols-2">
            {SCOPES.rows.map((r) => (
              <div key={r.scope} className="flex gap-4">
                <span
                  className={`${FONT_MONO} h-fit shrink-0 rounded-md border px-2.5 py-1.5 text-xs`}
                  style={{ borderColor: "hsla(46,90%,68%,0.35)", color: GOLD, background: "hsla(46,90%,60%,0.05)" }}
                >
                  {r.scope}
                </span>
                <p className={`${FONT_MONO} text-[11px] leading-relaxed`} style={{ color: dim(0.5) }}>
                  {r.body}
                </p>
              </div>
            ))}
          </div>
        </motion.div>
      </Section>

      {/* ─── 6. NEVER ────────────────────────────────────────────────────── */}
      <Section eyebrow="what it will never do" tone={MAGENTA}>
        <H2>The refusals are the feature.</H2>
        <Lede>
          Every one of these is a thing a competitor could ship next quarter and call an
          improvement. Each one would put the rot back.
        </Lede>

        <div className="mt-14 grid gap-6 md:grid-cols-2">
          {NEVER.map((n) => (
            <motion.div
              key={n.title}
              variants={rise}
              className="flex gap-5 rounded-xl border p-6"
              style={{ borderColor: "rgba(255,93,162,0.18)", background: "rgba(255,93,162,0.02)" }}
            >
              <span className="text-2xl leading-none" style={{ color: MAGENTA }} aria-hidden>
                ⨯
              </span>
              <div>
                <h3 className="text-base font-semibold tracking-tight" style={{ color: MAGENTA }}>
                  {n.title}
                </h3>
                <p className={`${FONT_MONO} mt-2 text-xs leading-relaxed`} style={{ color: dim(0.55) }}>
                  {n.body}
                </p>
              </div>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* ─── 7. THE LADDER ───────────────────────────────────────────────── */}
      <Section eyebrow="what is actually built" id="status" tone={dim(0.6)}>
        <H2>The status of every phase — including the one we built and switched off.</H2>
        <Lede>
          The build plan ships in the open with the product. Walk this ladder against it and catch
          us if a stamp is wrong. That is the point of printing it.
        </Lede>

        <div className="relative mt-14">
          {/* the rail */}
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
                      background: BG,
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
                    <p className={`${FONT_MONO} mt-2 max-w-3xl text-xs leading-relaxed`} style={{ color: dim(0.6) }}>
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
                      <p className={`${FONT_MONO} mt-3 max-w-3xl text-[11px] leading-relaxed`} style={{ color: dim(0.4) }}>
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
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
          className="relative overflow-hidden rounded-2xl border p-12 text-center md:p-20"
          style={{
            borderColor: "hsla(46,90%,68%,0.3)",
            background: "radial-gradient(ellipse at 50% 0%, hsla(46,90%,60%,0.10), transparent 70%)",
          }}
        >
          <h2 className="mx-auto max-w-3xl text-4xl font-semibold leading-tight tracking-tight text-white md:text-5xl">
            The page was stale the day after it was written.
            <br />
            <span style={{ color: GOLD, textShadow: `0 0 40px ${GOLD_GLOW}` }}>
              Stop writing pages.
            </span>
          </h2>
          <p className={`${FONT_MONO} mx-auto mt-6 max-w-xl text-xs leading-relaxed`} style={{ color: dim(0.45) }}>
            The memory layer underneath — capture, review gate, permission-aware retrieval,
            contradictions, Knowledge Health — is shipped and measured. The document layer on top
            is built and measured the same way. Publishing is the one switch still off, on purpose.
          </p>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
            <Link
              href="/"
              className={`${FONT_MONO} rounded-full px-7 py-3.5 text-sm font-semibold transition hover:scale-[1.03]`}
              style={{ background: GOLD, color: "#1a1405", boxShadow: `0 0 40px ${GOLD_GLOW}` }}
            >
              see the memory layer →
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
