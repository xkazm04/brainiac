"use client";

/*
 * The knowledge base — the wiki that cannot rot.
 *
 * Told in seven moves:
 *   1. the thesis      — a page is a projection, not a second source of truth
 *   2. the asymmetry   — the one-way relationship, drawn
 *   3. the properties  — four claims, each stamped shipped / in progress / roadmap
 *   4. the pipeline    — how a page is actually built
 *   5. Confluence      — the incumbent becomes a render target (roadmap)
 *   6. never           — what this layer will refuse to do
 *   7. the ladder      — the honest status of every phase, checkable in the repo
 *
 * Honesty rule inherited from pitch-data.ts:1-19 and enforced by kb-data.test.ts:
 * every capability carries an explicit status and nothing unbuilt is drawn as
 * shipped. A doc layer that lies on its own landing page has already lost the
 * argument it is making.
 */

import Link from "next/link";
import { motion, useScroll, useSpring } from "framer-motion";

import { BG, FONT_DISPLAY, FONT_MONO, GOLD, GOLD_GLOW, LABEL, MAGENTA, band } from "../design/theme";
import ProjectionDiagram from "./ProjectionDiagram";
import {
  ASYMMETRY,
  CHECK_US,
  COMPOSE_STAGES,
  CONFLUENCE,
  DIRTY_LOOP,
  LADDER,
  NEVER,
  PROPERTIES,
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

/* The one piece of visual vocabulary this page adds to the system: a status
   stamp. Shipped is mint (the console's "active recall" band), in-progress is
   gold, roadmap is a dashed cyan outline — dashed because it is not there. */
const STATUS_TONE: Record<Status, string> = {
  shipped: MINT,
  in_progress: GOLD,
  roadmap: ALPHA,
};

function Stamp({ status, className = "" }: { status: Status; className?: string }) {
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
      {status === "shipped" ? "●" : status === "in_progress" ? "◐" : "○"} {STATUS_LABEL[status]}
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

export default function KnowledgeBase() {
  const { scrollYProgress } = useScroll();
  const progress = useSpring(scrollYProgress, { stiffness: 90, damping: 30 });

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

      {/* ─── 1. THE THESIS ───────────────────────────────────────────────── */}
      <section className="mx-auto max-w-6xl px-6 pb-8 pt-10 md:pt-16">
        <motion.div initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.8 }}>
          <div className="flex flex-wrap items-center gap-3">
            <div className={LABEL} style={{ color: ALPHA }}>
              the document layer
            </div>
            <Stamp status="in_progress" />
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
          <p
            className="mt-8 max-w-3xl border-l-2 pl-6 text-xl font-medium leading-snug tracking-tight text-white md:text-2xl"
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
          The memory layer and the knowledge base are separate, and the relationship between
          them is deliberately lopsided. Memories compose into pages. Pages never write back
          into memories — a human edit re-enters through extraction and faces the same review
          gate as any agent proposal.
        </Lede>

        <motion.div
          variants={rise}
          className="mt-12 rounded-xl border p-5 md:p-8"
          style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
        >
          <ProjectionDiagram />
        </motion.div>

        <div className="mt-10 grid gap-px overflow-hidden rounded-xl border md:grid-cols-2" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {ASYMMETRY.map((f) => (
            <motion.div key={f.label} variants={rise} className="p-7" style={{ background: BG }}>
              <div className={`${FONT_MONO} flex items-center gap-2 text-xs`} style={{ color: dim(0.5) }}>
                <span style={{ color: f.allowed ? dim(0.75) : MAGENTA }}>{f.from}</span>
                <span style={{ color: f.allowed ? GOLD : MAGENTA }}>{f.allowed ? "→" : "⨯"}</span>
                <span style={{ color: f.allowed ? dim(0.75) : MAGENTA }}>{f.to}</span>
              </div>
              <h3
                className="mt-3 text-lg font-semibold tracking-tight"
                style={{ color: f.allowed ? "#fff" : MAGENTA }}
              >
                {f.allowed ? f.label : `${f.label} — never`}
              </h3>
              {f.gate && (
                <div className={`${LABEL} mt-2`} style={{ color: GOLD }}>
                  through: {f.gate}
                </div>
              )}
              <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.55) }}>
                {f.note}
              </p>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* ─── 3. THE PROPERTIES ───────────────────────────────────────────── */}
      <Section eyebrow="four properties · four honest statuses" tone={MINT}>
        <H2>What makes a page that cannot rot — and how much of it exists today.</H2>
        <Lede>
          Two of these are shipped and you can check them in the repo. One is being built as
          you read this. One is designed and unbuilt. Nothing on this page pretends otherwise.
        </Lede>

        <div className="mt-14 grid gap-6 md:grid-cols-2">
          {PROPERTIES.map((p) => {
            const tone = STATUS_TONE[p.status];
            return (
              <motion.div
                key={p.key}
                variants={rise}
                className="flex flex-col rounded-xl border p-7"
                style={{
                  borderColor: p.status === "shipped" ? "hsla(158,90%,68%,0.3)" : "rgba(233,237,255,0.10)",
                  borderStyle: p.status === "roadmap" ? "dashed" : "solid",
                  background: p.status === "shipped" ? "hsla(158,90%,60%,0.03)" : "rgba(255,255,255,0.02)",
                }}
              >
                <div className="flex items-start justify-between gap-4">
                  <h3 className="text-xl font-semibold tracking-tight text-white">{p.title}</h3>
                  <Stamp status={p.status} />
                </div>
                <div className={`${FONT_MONO} mt-3 text-xs`} style={{ color: tone }}>
                  {p.claim}
                </div>
                <p className={`${FONT_MONO} mt-4 flex-1 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                  {p.body}
                </p>
                <div className={`${FONT_MONO} mt-5 text-[10px]`} style={{ color: dim(0.32) }}>
                  check it: {p.evidence}
                </div>
              </motion.div>
            );
          })}
        </div>
      </Section>

      {/* ─── 4. THE PIPELINE ─────────────────────────────────────────────── */}
      <Section eyebrow="how a page is built" id="pipeline">
        <H2>Nobody schedules a doc review. There is nothing to review.</H2>
        <Lede>{DIRTY_LOOP}</Lede>

        <div
          className="mt-14 grid gap-px overflow-hidden rounded-xl border sm:grid-cols-2 lg:grid-cols-3"
          style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}
        >
          {COMPOSE_STAGES.map((s) => (
            <motion.div key={s.n} variants={rise} className="p-7" style={{ background: BG }}>
              <div className="flex items-center justify-between gap-3">
                <div className={LABEL} style={{ color: STATUS_TONE[s.status] }}>
                  {s.n} · {s.name}
                </div>
                <Stamp status={s.status} />
              </div>
              <p className={`${FONT_MONO} mt-4 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                {s.body}
              </p>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* ─── 5. CONFLUENCE ───────────────────────────────────────────────── */}
      <Section eyebrow="publishing · roadmap" tone={ALPHA}>
        <div className="mt-4 flex flex-wrap items-center gap-3">
          <Stamp status={CONFLUENCE.status} />
          <span className={`${FONT_MONO} text-xs`} style={{ color: dim(0.4) }}>
            designed, not built — docs/KB-PLAN.md, phase KB3
          </span>
        </div>
        <H2>{CONFLUENCE.headline}</H2>
        <Lede>{CONFLUENCE.body}</Lede>

        <div className="mt-12 grid gap-6 md:grid-cols-3">
          {CONFLUENCE.invariants.map((inv) => (
            <motion.div
              key={inv.title}
              variants={rise}
              className="rounded-xl border border-dashed p-6"
              style={{ borderColor: "hsla(190,90%,68%,0.28)", background: "rgba(255,255,255,0.015)" }}
            >
              <div className={LABEL} style={{ color: ALPHA }}>
                {inv.title}
              </div>
              <p className={`${FONT_MONO} mt-4 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                {inv.body}
              </p>
            </motion.div>
          ))}
        </div>

        {/* scoping */}
        <motion.div
          variants={rise}
          className="mt-14 rounded-xl border border-dashed p-8"
          style={{ borderColor: "rgba(233,237,255,0.14)", background: "rgba(255,255,255,0.02)" }}
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
          <div className="mt-6 grid gap-4 md:grid-cols-3">
            {SCOPES.rows.map((r) => (
              <div key={r.scope}>
                <code className={`${FONT_MONO} text-sm`} style={{ color: GOLD }}>
                  {r.scope}
                </code>
                <p className={`${FONT_MONO} mt-2 text-[11px] leading-relaxed`} style={{ color: dim(0.5) }}>
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

        <div
          className="mt-14 grid gap-px overflow-hidden rounded-xl border md:grid-cols-2"
          style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}
        >
          {NEVER.map((n) => (
            <motion.div key={n.title} variants={rise} className="p-7" style={{ background: BG }}>
              <h3 className="text-lg font-semibold tracking-tight" style={{ color: MAGENTA }}>
                {n.title}
              </h3>
              <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                {n.body}
              </p>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* ─── 7. THE LADDER ───────────────────────────────────────────────── */}
      <Section eyebrow="what is actually built" id="status" tone={dim(0.6)}>
        <H2>The status of every phase, including the ones we have not written yet.</H2>
        <Lede>
          This is the same ladder as <code>docs/KB-PLAN.md</code>. A reviewer can walk it against
          the status log in the repo and catch us if a stamp is wrong. That is the point of
          printing it.
        </Lede>

        <div className="mt-14 space-y-px overflow-hidden rounded-xl border" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {LADDER.map((p) => (
            <motion.div
              key={p.id}
              variants={rise}
              className="grid gap-5 p-7 md:grid-cols-[180px_1fr]"
              style={{ background: BG }}
            >
              <div>
                <div className="text-lg font-semibold tracking-tight" style={{ color: STATUS_TONE[p.status] }}>
                  {p.id}
                </div>
                <div className={`${FONT_MONO} mt-1 text-xs`} style={{ color: dim(0.6) }}>
                  {p.name}
                </div>
                <div className="mt-3">
                  <Stamp status={p.status} />
                </div>
              </div>
              <div>
                <p className={`${FONT_MONO} text-xs leading-relaxed`} style={{ color: dim(0.62) }}>
                  {p.body}
                </p>
                {p.gate && (
                  <p
                    className={`${FONT_MONO} mt-4 border-l pl-4 text-[11px] leading-relaxed`}
                    style={{ borderColor: MINT, color: dim(0.45) }}
                  >
                    {p.gate}
                  </p>
                )}
              </div>
            </motion.div>
          ))}
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
            The memory layer underneath this — capture, review gate, permission-aware retrieval,
            contradictions, Knowledge Health — is shipped and measured. The document layer on top
            of it is being built now, in the open.
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
              style={{ borderColor: "rgba(233,237,255,0.2)", color: dim(0.7) }}
            >
              the health score that gates publishing
            </Link>
          </div>
        </motion.div>
      </section>

      <footer
        className={`${LABEL} mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 border-t px-6 py-8`}
        style={{ borderColor: "rgba(233,237,255,0.10)", color: dim(0.32) }}
      >
        <span>brainiac · the wiki that cannot rot</span>
        <span>every capability on this page is stamped shipped, in progress, or roadmap</span>
      </footer>
    </div>
  );
}
