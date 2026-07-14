"use client";

/*
 * The pitch. One argument, told in eight moves:
 *
 *   1. the gate            — the hero: nothing becomes truth without a signature
 *   2. the capture gap     — why indexing what people wrote can never be enough
 *   3. the bifurcation     — the empty quadrant, and who sits where
 *   4. benchmark theater   — why every number in this market is worthless
 *   5. the retreat         — four vendors who looked at governance and walked away
 *   6. the matrix          — the row that is empty for everyone but us
 *   7. our evidence        — including the trial we allowed ourselves to lose
 *   8. where we lose       — the section no competitor has
 *
 * Every competitor claim carries a primary-source link (see pitch-data.ts).
 * Every number of ours is reproducible from results/ and uat/ in this repo.
 */

import { useCallback, useRef, useState } from "react";
import Link from "next/link";
import { motion, useReducedMotion, useScroll, useSpring } from "framer-motion";
import {
  Bar,
  BarChart,
  Cell as RCell,
  ReferenceArea,
  ReferenceLine,
  ResponsiveContainer,
  Scatter,
  ScatterChart,
  Tooltip,
  XAxis,
  YAxis,
  ZAxis,
} from "recharts";

import {
  BG,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  GOLD_GLOW,
  LABEL,
  MAGENTA,
  band,
} from "../design/theme";
import GateField from "./GateField";
import {
  BIFURCATION_LINE,
  CAPTURE_GAP,
  EMPTY_ROW,
  KB_TEASER,
  LOCOMO_CEILING,
  LOCOMO_FACTS,
  LOCOMO_WAR,
  MATRIX,
  MATRIX_VENDORS,
  PIPELINE,
  PIPELINE_STAGES,
  PLAYERS,
  POISON,
  RETREAT,
  RETRIEVAL,
  UAT,
  WEAKNESSES,
  type Cell,
  type Player,
} from "./pitch-data";

const MINT = band("beta");
const ALPHA = band("alpha");
const INK = "#e9edff";
const dim = (a: number) => `rgba(233,237,255,${a})`;

const rise = {
  hidden: { opacity: 0, y: 18 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.55, ease: [0.2, 0.7, 0.3, 1] as const } },
};

/** Section wrapper: consistent rhythm, scroll-reveal, an eyebrow and a spine. */
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
      viewport={{ once: true, amount: 0.15 }}
      variants={{ visible: { transition: { staggerChildren: 0.08 } } }}
      className="relative mx-auto max-w-6xl px-6 py-24 md:py-32"
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

function Source({ label, href }: { label: string; href: string }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className={`${FONT_MONO} text-[10px] uppercase tracking-[0.14em] underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
      style={{ color: dim(0.32) }}
    >
      {label} ↗
    </a>
  );
}

/** Chart tooltip in the console's instrument idiom. */
function Tip({ title, rows }: { title: string; rows: [string, string][] }) {
  return (
    <div
      className={`${FONT_MONO} max-w-xs rounded-lg border px-3 py-2 text-[11px] leading-relaxed shadow-2xl backdrop-blur`}
      style={{ borderColor: "rgba(233,237,255,0.14)", background: "rgba(10,9,16,0.94)", color: INK }}
    >
      <div className="font-semibold text-white">{title}</div>
      {rows.map(([k, v]) => (
        <div key={k} className="mt-1" style={{ color: dim(0.6) }}>
          {k ? <span style={{ color: dim(0.4) }}>{k} </span> : null}
          {v}
        </div>
      ))}
    </div>
  );
}

export default function Pitch() {
  const reduce = !!useReducedMotion();
  const approveRef = useRef<(() => void) | null>(null);
  const [queue, setQueue] = useState({ queued: 0, canonical: 0 });
  const throttle = useRef(0);

  const onQueueChange = useCallback((queued: number, canonical: number) => {
    // The canvas ticks at 60fps; React does not need to.
    const now = performance.now();
    if (now - throttle.current < 180) return;
    throttle.current = now;
    setQueue((prev) =>
      prev.queued === queued && prev.canonical === canonical ? prev : { queued, canonical },
    );
  }, []);

  const { scrollYProgress } = useScroll();
  const progress = useSpring(scrollYProgress, { stiffness: 90, damping: 30 });

  return (
    <div className={`${FONT_DISPLAY} min-h-screen`} style={{ background: BG, color: INK }}>
      {/* scroll progress — a single gold hairline */}
      <motion.div
        aria-hidden
        className="fixed left-0 top-0 z-50 h-[2px] w-full origin-left"
        style={{ scaleX: progress, background: GOLD, boxShadow: `0 0 12px ${GOLD_GLOW}` }}
      />

      {/* ─── 1. THE GATE ─────────────────────────────────────────────────── */}
      <header className="mx-auto flex max-w-6xl items-center justify-between px-6 py-6">
        <Link href="/" className="flex items-center gap-3">
          <span className="text-xl font-semibold tracking-tight text-white">Brainiac</span>
          <span className={LABEL} style={{ color: GOLD }}>
            γ · binding console
          </span>
        </Link>
        <nav className={`${FONT_MONO} hidden gap-6 text-xs uppercase tracking-widest md:flex`} style={{ color: dim(0.45) }}>
          <a href="#quadrant" className="transition hover:text-[#f3c74f]">the gap</a>
          <a href="#evidence" className="transition hover:text-[#f3c74f]">evidence</a>
          <a href="#weakness" className="transition hover:text-[#f3c74f]">where we lose</a>
          <Link href="/kb" className="transition hover:text-[#f3c74f]">knowledge base</Link>
          <Link href="/demo" className="transition hover:text-[#f3c74f]">demo</Link>
          {/* Gated: an anonymous visitor lands on the passcode screen, which is
              the honest thing to show them rather than a 401. */}
          <Link href="/console" className="transition hover:text-[#f3c74f]">console →</Link>
        </nav>
      </header>

      <section className="mx-auto max-w-6xl px-6">
        <div
          className="relative overflow-hidden rounded-2xl border"
          style={{ borderColor: "rgba(233,237,255,0.10)" }}
        >
          {/* The art owns the right half; the copy owns the left. Letting the
              field run full-bleed drifts queued particles under the headline,
              where they read as dirt on the screen rather than as a backlog.
              The left edge is masked so particles emerge rather than pop in. */}
          <div
            className="absolute inset-y-0 right-0 w-full md:w-[58%]"
            style={{
              maskImage: "linear-gradient(to right, transparent 0%, black 14%)",
              WebkitMaskImage: "linear-gradient(to right, transparent 0%, black 14%)",
            }}
          >
            <GateField
              onQueueChange={onQueueChange}
              onApproveRef={(fn) => {
                approveRef.current = fn;
              }}
            />
          </div>

          {/* Column layout, not absolute positioning: the copy and the telemetry
              strip must never overlap, whatever the viewport height. */}
          <div className="relative flex h-[78vh] min-h-[600px] flex-col justify-between p-8 md:p-12">
            <motion.div
              initial={{ opacity: 0, y: 16 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.9 }}
              className="max-w-xl"
            >
              <div className={LABEL} style={{ color: GOLD }}>
                org memory · governed
              </div>
              <h1 className="mt-5 text-[2.75rem] font-semibold leading-[1.04] tracking-tight text-white lg:text-6xl">
                Nothing becomes
                <br />
                what your company
                <br />
                <span style={{ color: GOLD, textShadow: `0 0 48px ${GOLD_GLOW}` }}>
                  knows
                </span>{" "}
                <span style={{ color: dim(0.5) }}>until</span>
                <br />
                someone signs it.
              </h1>
              <p
                className={`${FONT_MONO} mt-5 max-w-lg text-sm leading-relaxed`}
                style={{ color: dim(0.6) }}
              >
                Every other memory product lets a language model write an unattributed
                belief straight into your org&rsquo;s record and serve it back as truth.
                Brainiac makes an agent <em>propose</em>, and a named human{" "}
                <em>promote</em>.
              </p>

              <div className="mt-7 flex flex-wrap items-center gap-4">
                <button
                  onClick={() => approveRef.current?.()}
                  className={`${FONT_MONO} rounded-full border px-6 py-3 text-sm font-medium transition hover:scale-[1.02]`}
                  style={{
                    borderColor: GOLD,
                    color: GOLD,
                    background: "hsla(46,90%,60%,0.10)",
                    boxShadow: `0 0 32px ${GOLD_GLOW}`,
                  }}
                >
                  ◉ approve the queue
                </button>
                <Link
                  href="#quadrant"
                  className={`${FONT_MONO} text-xs uppercase tracking-[0.18em] underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
                  style={{ color: dim(0.45) }}
                >
                  → why nobody else does this
                </Link>
              </div>
            </motion.div>

            {/* live gate telemetry — reads off the running simulation */}
            <div
              className={`${LABEL} mt-8 flex flex-wrap items-center justify-between gap-3`}
              style={{ color: dim(0.35) }}
            >
              <span>
                <span style={{ color: dim(0.55) }}>{queue.queued}</span> awaiting review ·{" "}
                <span style={{ color: GOLD }}>{queue.canonical}</span> canonical ·{" "}
                <span style={{ color: MAGENTA }}>poison refused at the gate</span>
              </span>
              <span className="hidden md:inline">
                raw extractions cannot pass unsigned
              </span>
            </div>
          </div>
        </div>
      </section>

      {/* ─── 2. THE CAPTURE GAP ──────────────────────────────────────────── */}
      <Section eyebrow="the capture gap" tone={ALPHA}>
        <H2>
          Enterprise search only indexes what somebody already bothered to write
          down.
        </H2>
        <Lede>
          Glean is worth $7.2 billion and does one thing beautifully: it enforces who is
          allowed to read which document. Its MCP verbs are <code>search</code> and{" "}
          <code>read_document</code>. There is no capture verb anywhere in that market —
          so the ceiling of every index is the corpus, and the corpus is missing the part
          that matters.
        </Lede>

        <div className="mt-14 grid gap-px overflow-hidden rounded-xl border sm:grid-cols-2 lg:grid-cols-4" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {CAPTURE_GAP.map((s, i) => (
            <motion.div
              key={s.value}
              variants={rise}
              className="flex flex-col justify-between p-6"
              style={{ background: BG }}
            >
              <div>
                <div
                  className="text-4xl font-semibold tracking-tight"
                  style={{ color: i === 3 ? MAGENTA : GOLD }}
                >
                  {s.value}
                </div>
                <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.72) }}>
                  {s.claim}
                </p>
                <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.42) }}>
                  {s.note}
                </p>
              </div>
              <div className="mt-5">
                <Source label={s.cite.label} href={s.cite.href} />
              </div>
            </motion.div>
          ))}
        </div>

        <motion.blockquote
          variants={rise}
          className="mt-14 border-l-2 pl-6 text-2xl font-medium leading-snug tracking-tight text-white md:text-3xl"
          style={{ borderColor: GOLD }}
        >
          Search is a recall technology, sold to an organisation that has a capture
          problem.
        </motion.blockquote>
      </Section>

      {/* ─── 3. THE BIFURCATION ──────────────────────────────────────────── */}
      <Section eyebrow="the empty quadrant" id="quadrant">
        <H2>Two industries. Neither one is memory your company can trust.</H2>
        <Lede>{BIFURCATION_LINE}</Lede>
        <Quadrant />
      </Section>

      {/* ─── 4. BENCHMARK THEATER ────────────────────────────────────────── */}
      <Section eyebrow="benchmark theater" tone={MAGENTA}>
        <H2>Every accuracy number in this market is theater. Here is the receipt.</H2>
        <Lede>
          LOCOMO is the benchmark the memory category markets itself on. One system
          (Zep), one benchmark, fourteen months — and a thirty-six point spread, every
          number of it produced by a vendor with a commercial interest in the answer.
        </Lede>
        <LocomoWar />

        <div className="mt-16 grid gap-6 md:grid-cols-2">
          {LOCOMO_FACTS.map((f, i) => (
            <motion.div
              key={f.stat}
              variants={rise}
              className="rounded-xl border p-6"
              style={{
                borderColor: i === 3 ? "hsla(158,90%,68%,0.35)" : "rgba(233,237,255,0.10)",
                background: i === 3 ? "hsla(158,90%,60%,0.04)" : "rgba(255,255,255,0.02)",
              }}
            >
              <div
                className="text-3xl font-semibold tracking-tight"
                style={{ color: i === 3 ? MINT : MAGENTA }}
              >
                {f.stat}
              </div>
              <div className={`${FONT_MONO} mt-2 text-sm font-medium`} style={{ color: dim(0.85) }}>
                {f.claim}
              </div>
              <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.5) }}>
                {f.detail}
              </p>
              <div className="mt-4">
                <Source label={f.cite.label} href={f.cite.href} />
              </div>
            </motion.div>
          ))}
        </div>

        <motion.div
          variants={rise}
          className="mt-14 rounded-xl border p-8"
          style={{ borderColor: "rgba(233,237,255,0.12)", background: "rgba(255,255,255,0.02)" }}
        >
          <div className={LABEL} style={{ color: GOLD }}>
            what we do instead
          </div>
          <p className="mt-4 max-w-3xl text-xl font-medium leading-snug tracking-tight text-white md:text-2xl">
            We refuse to publish a LOCOMO score. We built a synthetic org, planted
            contradictions and poison in it, and ran a controlled trial against the free
            baseline — one we allowed ourselves to lose.
          </p>
          <p className={`${FONT_MONO} mt-4 max-w-2xl text-xs leading-relaxed`} style={{ color: dim(0.5) }}>
            The prescription any honest evaluator would give you: build a full-context
            baseline and a filesystem-plus-grep baseline on a sample of your own data, and
            only accept a memory system if it beats both by a wide margin. Hold us to it
            too.
          </p>
        </motion.div>
      </Section>

      {/* ─── 5. THE RETREAT ──────────────────────────────────────────────── */}
      <Section eyebrow="the retreat" tone={band("theta")}>
        <H2>
          They didn&rsquo;t fail to notice this problem. They looked straight at it and
          walked away.
        </H2>
        <Lede>
          In the last six months, four of the most sophisticated players in this market
          each retreated from exactly the thing we build. That is not a market that
          hasn&rsquo;t gotten to governance yet. It is a market that tried ungoverned
          memory and backed out.
        </Lede>

        <div className="mt-14 space-y-px overflow-hidden rounded-xl border" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {RETREAT.map((r) => (
            <motion.div
              key={r.who}
              variants={rise}
              className="grid gap-4 p-7 md:grid-cols-[200px_1fr]"
              style={{ background: BG }}
            >
              <div>
                <div className="text-lg font-semibold tracking-tight text-white">{r.who}</div>
                <div className={`${FONT_MONO} mt-1 text-xs`} style={{ color: MAGENTA }}>
                  {r.what}
                </div>
                <div className={`${LABEL} mt-2`} style={{ color: dim(0.3) }}>
                  {r.when}
                </div>
              </div>
              <div>
                <p className={`${FONT_MONO} text-xs leading-relaxed`} style={{ color: dim(0.6) }}>
                  {r.detail}
                </p>
                <div className="mt-3">
                  <Source label={r.cite.label} href={r.cite.href} />
                </div>
              </div>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* ─── 6. THE MATRIX ───────────────────────────────────────────────── */}
      <Section eyebrow="the capability matrix" tone={ALPHA}>
        <H2>One row is empty for everyone but us. It is the row that is the product.</H2>
        <Lede>
          Permission-aware retrieval is filling in fast and will be commodity within a
          year — we do not rest the argument there. The row nobody fills is the review
          gate, and it is empty for a structural reason: this category&rsquo;s entire
          design center is <em>fully automatic, invisible, zero-config</em>. Human-in-the-loop
          looks to them like a bug.
        </Lede>
        <Matrix />
      </Section>

      {/* ─── 7. OUR EVIDENCE ─────────────────────────────────────────────── */}
      <Section eyebrow="the evidence" id="evidence" tone={MINT}>
        <H2>Our numbers, and the trial we let ourselves lose.</H2>
        <Lede>
          Every figure below is reproducible from this repository — <code>results/</code>{" "}
          for the eval harness, <code>uat/</code> for the controlled trial. Real Qwen
          embeddings, real Claude Code agents, real MCP server.
        </Lede>

        <div className="mt-12 grid gap-6 lg:grid-cols-[1.15fr_1fr]">
          <RetrievalChart />
          <div className="grid gap-6">
            <StatTile
              label="permission leaks"
              value="0"
              tone={MINT}
              body="Across the full leak-probe suite. Row-level security runs inside the pgvector scan — an agent cannot retrieve what its operator cannot read, because the database refuses, not because the code remembered to filter."
            />
            <StatTile
              label="contradiction detection"
              value={`${PIPELINE.contradiction.precision.toFixed(2)} / ${PIPELINE.contradiction.recall.toFixed(2)}`}
              tone={GOLD}
              body={`Precision ${PIPELINE.contradiction.precision.toFixed(2)}, recall ${PIPELINE.contradiction.recall.toFixed(2)}, direction accuracy ${PIPELINE.contradiction.direction.toFixed(2)}, false-positive rate ${PIPELINE.contradiction.falsePositive.toFixed(2)}. When it flags a conflict, it is right, and it knows which claim supersedes which.`}
            />
            <StatTile
              label="stale memories served"
              value="0"
              tone={MINT}
              body={`Superseded memories in the top 3, across ${RETRIEVAL.queries} queries. Temporal rank-1 accuracy ${RETRIEVAL.temporalRank1.toFixed(2)} — ask what we believed in March and you get March's answer, not today's.`}
            />
          </div>
        </div>

        {/* the controlled trial */}
        <motion.div variants={rise} className="mt-24">
          <div className={LABEL} style={{ color: GOLD }}>
            the controlled trial
          </div>
          <h3 className="mt-4 max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white">
            Three arms. A cold agent, Claude&rsquo;s native memory built generously, and
            Brainiac on top of it. The verdict is C&nbsp;&minus;&nbsp;B — and it was
            allowed to come out negative.
          </h3>
          <p className={`${FONT_MONO} mt-4 max-w-2xl text-sm leading-relaxed`} style={{ color: dim(0.55) }}>
            The baseline was written first, and written generously, before a single
            journey ran — so it could not be tuned to lose. Two of the journeys are
            deliberate controls where we predicted Brainiac would add nothing.
          </p>
        </motion.div>

        <div className="mt-12 space-y-6">
          {UAT.map((j) => (
            <UatCard key={j.key} journey={j} />
          ))}
        </div>
      </Section>

      {/* ─── the poison ──────────────────────────────────────────────────── */}
      <Section eyebrow="we tried to poison it" tone={MAGENTA}>
        <H2>We planted a poison memory. It beat us. Then we fixed it.</H2>
        <Lede>{POISON.premise}</Lede>

        <div className="mt-14 grid gap-6 md:grid-cols-3">
          {POISON.rounds.map((r, i) => (
            <motion.div
              key={r.round}
              variants={rise}
              className="relative rounded-xl border p-6"
              style={{
                borderColor: r.tone === "good" ? "hsla(46,90%,68%,0.4)" : "rgba(255,93,162,0.28)",
                background: r.tone === "good" ? "hsla(46,90%,60%,0.05)" : "rgba(255,93,162,0.04)",
              }}
            >
              <div className={LABEL} style={{ color: r.tone === "good" ? GOLD : MAGENTA }}>
                {r.round}
              </div>
              <p className={`${FONT_MONO} mt-4 text-xs leading-relaxed`} style={{ color: dim(0.65) }}>
                {r.behavior}
              </p>
              <div
                className="mt-5 text-lg font-semibold tracking-tight"
                style={{ color: r.tone === "good" ? GOLD : MAGENTA }}
              >
                {r.tone === "good" ? "✓ " : "✗ "}
                {r.outcome}
              </div>
              {i < 2 && (
                <div
                  aria-hidden
                  className={`${FONT_MONO} absolute -right-3 top-1/2 hidden text-lg md:block`}
                  style={{ color: dim(0.25) }}
                >
                  →
                </div>
              )}
            </motion.div>
          ))}
        </div>

        <motion.blockquote
          variants={rise}
          className="mt-14 border-l-2 pl-6 text-2xl font-medium leading-snug tracking-tight text-white md:text-3xl"
          style={{ borderColor: GOLD }}
        >
          {POISON.quote}
        </motion.blockquote>
        <motion.p variants={rise} className={`${FONT_MONO} mt-5 max-w-2xl text-sm leading-relaxed`} style={{ color: dim(0.5) }}>
          {POISON.moral}
        </motion.p>
      </Section>

      {/* ─── 8. WHERE WE LOSE ────────────────────────────────────────────── */}
      <Section eyebrow="where we lose" id="weakness" tone={dim(0.6)}>
        <H2>The section no competitor on this page has.</H2>
        <Lede>
          A vendor who cannot tell you when their product is the wrong choice has not
          measured it. Here is ours, from our own evaluation, unprompted.
        </Lede>

        <div className="mt-14 grid gap-px overflow-hidden rounded-xl border md:grid-cols-2" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {WEAKNESSES.map((w) => (
            <motion.div key={w.title} variants={rise} className="p-7" style={{ background: BG }}>
              <div className={LABEL} style={{ color: MAGENTA }}>
                {w.metric}
              </div>
              <h3 className="mt-3 text-lg font-semibold tracking-tight text-white">{w.title}</h3>
              <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.55) }}>
                {w.body}
              </p>
            </motion.div>
          ))}
        </div>

        <motion.blockquote
          variants={rise}
          className="mt-14 border-l-2 pl-6 text-2xl font-medium leading-snug tracking-tight text-white md:text-3xl"
          style={{ borderColor: GOLD }}
        >
          Turn it on where the knowledge crosses a boundary. Leave it off where a text
          file already wins. We are the only memory layer that will tell you that.
        </motion.blockquote>
      </Section>

      {/* ─── how it works ────────────────────────────────────────────────── */}
      <Section eyebrow="how it works" tone={ALPHA}>
        <H2>Session in. Signed canonical knowledge out.</H2>
        <div className="mt-14 grid gap-px overflow-hidden rounded-xl border sm:grid-cols-2 lg:grid-cols-3" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {PIPELINE_STAGES.map((s) => (
            <motion.div key={s.n} variants={rise} className="p-7" style={{ background: BG }}>
              <div className={LABEL} style={{ color: GOLD }}>
                {s.n} · {s.name}
              </div>
              <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                {s.body}
              </p>
            </motion.div>
          ))}
        </div>
        <motion.p variants={rise} className={`${FONT_MONO} mt-8 max-w-2xl text-xs leading-relaxed`} style={{ color: dim(0.42) }}>
          One Rust binary. Postgres is the only mandatory dependency. Your model, your
          keys, your infrastructure — the transcripts never leave. Runs on a 1&nbsp;vCPU
          box.
        </motion.p>
      </Section>

      {/* ─── the knowledge base ──────────────────────────────────────────── */}
      <Section eyebrow={`the knowledge base · ${KB_TEASER.status}`} tone={ALPHA}>
        <H2>{KB_TEASER.headline}</H2>
        <Lede>{KB_TEASER.body}</Lede>

        <div className="mt-14 grid gap-6 md:grid-cols-3">
          {KB_TEASER.points.map((p) => {
            const built = p.label === "shipped";
            const soon = p.label === "in progress";
            const tone = built ? MINT : soon ? GOLD : ALPHA;
            return (
              <motion.div
                key={p.label}
                variants={rise}
                className="rounded-xl border p-6"
                style={{
                  borderColor: built ? "hsla(158,90%,68%,0.3)" : "rgba(233,237,255,0.12)",
                  // Dashed = not there yet. The page says which is which, and so does its border.
                  borderStyle: built || soon ? "solid" : "dashed",
                  background: built ? "hsla(158,90%,60%,0.03)" : "rgba(255,255,255,0.02)",
                }}
              >
                <div className={LABEL} style={{ color: tone }}>
                  {built ? "● " : soon ? "◐ " : "○ "}
                  {p.label}
                </div>
                <p className={`${FONT_MONO} mt-4 text-xs leading-relaxed`} style={{ color: dim(0.58) }}>
                  {p.body}
                </p>
              </motion.div>
            );
          })}
        </div>

        <motion.div variants={rise} className="mt-10">
          <Link
            href={KB_TEASER.href}
            className={`${FONT_MONO} text-xs uppercase tracking-[0.18em] underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
            style={{ color: dim(0.5) }}
          >
            → the wiki that cannot rot, and exactly how much of it exists today
          </Link>
        </motion.div>
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
            background:
              "radial-gradient(ellipse at 50% 0%, hsla(46,90%,60%,0.10), transparent 70%)",
          }}
        >
          <h2 className="mx-auto max-w-3xl text-4xl font-semibold leading-tight tracking-tight text-white md:text-5xl">
            Your agents are already learning things about your company.
            <br />
            <span style={{ color: GOLD, textShadow: `0 0 40px ${GOLD_GLOW}` }}>
              Nobody signed for any of it.
            </span>
          </h2>
          <div className="mt-10 flex flex-wrap items-center justify-center gap-4">
            <Link
              href="/demo"
              className={`${FONT_MONO} rounded-full px-7 py-3.5 text-sm font-semibold transition hover:scale-[1.03]`}
              style={{ background: GOLD, color: "#1a1405", boxShadow: `0 0 40px ${GOLD_GLOW}` }}
            >
              see the live fixture org →
            </Link>
            <Link
              href="/console"
              className={`${FONT_MONO} rounded-full border px-7 py-3.5 text-sm transition hover:text-[#f3c74f]`}
              style={{ borderColor: "rgba(233,237,255,0.2)", color: dim(0.7) }}
            >
              sign in to the console
            </Link>
          </div>
          <p className={`${FONT_MONO} mt-8 text-xs`} style={{ color: dim(0.35) }}>
            MIT licensed · self-hosted · bring your own model
          </p>
        </motion.div>
      </section>

      <footer
        className={`${LABEL} mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 border-t px-6 py-8`}
        style={{ borderColor: "rgba(233,237,255,0.10)", color: dim(0.32) }}
      >
        <span>brainiac · constructive by design</span>
        <span>
          every competitor claim on this page links to its primary source · every number
          of ours is in the repo
        </span>
      </footer>

      {reduce ? null : null}
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────────────────── */
/* Charts                                                                      */
/* ─────────────────────────────────────────────────────────────────────────── */

const CAMP_COLOR: Record<Player["camp"], string> = {
  search: ALPHA,
  memory: MAGENTA,
  neither: "rgba(233,237,255,0.35)",
  us: GOLD,
};

function Quadrant() {
  const [active, setActive] = useState<string | null>(null);

  return (
    <motion.div variants={rise} className="mt-12">
      <div
        className="relative rounded-xl border p-4 md:p-6"
        style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
      >
        <div className="h-[540px] w-full">
          <ResponsiveContainer>
            <ScatterChart margin={{ top: 24, right: 40, bottom: 44, left: 24 }}>
              <XAxis
                type="number"
                dataKey="x"
                domain={[0, 10]}
                tick={false}
                axisLine={{ stroke: "rgba(233,237,255,0.18)" }}
                label={{
                  value: "enforced authorization  →",
                  position: "insideBottom",
                  offset: -22,
                  style: {
                    fill: "rgba(233,237,255,0.4)",
                    fontSize: 11,
                    letterSpacing: "0.2em",
                    textTransform: "uppercase",
                  },
                }}
              />
              <YAxis
                type="number"
                dataKey="y"
                domain={[0, 10]}
                tick={false}
                axisLine={{ stroke: "rgba(233,237,255,0.18)" }}
                label={{
                  value: "governed truth  →",
                  angle: -90,
                  position: "insideLeft",
                  offset: 0,
                  style: {
                    fill: "rgba(233,237,255,0.4)",
                    fontSize: 11,
                    letterSpacing: "0.2em",
                    textTransform: "uppercase",
                  },
                }}
              />
              <ZAxis range={[80, 80]} />

              {/* The unclaimed square, anchored to the DATA rather than to the
                  container — a CSS box only lines up with the points by luck. */}
              <ReferenceArea
                x1={6.4}
                x2={10}
                y1={6.4}
                y2={10}
                fill="hsla(46,90%,60%,0.07)"
                stroke="hsla(46,90%,68%,0.32)"
                strokeDasharray="4 4"
                label={{
                  value: "THE UNCLAIMED SQUARE",
                  position: "insideTopLeft",
                  fill: "hsla(46,90%,68%,0.8)",
                  fontSize: 10,
                  letterSpacing: "0.2em",
                  fontFamily: "var(--font-mono)",
                }}
              />
              <ReferenceLine x={5} stroke="rgba(233,237,255,0.07)" />
              <ReferenceLine y={5} stroke="rgba(233,237,255,0.07)" />

              <Tooltip
                cursor={false}
                content={({ active: on, payload }) => {
                  if (!on || !payload?.length) return null;
                  const p = payload[0].payload as Player;
                  return <Tip title={p.full} rows={[["", p.why]]} />;
                }}
              />
              <Scatter
                data={PLAYERS}
                onMouseEnter={(p) => setActive((p as unknown as Player).name)}
                onMouseLeave={() => setActive(null)}
                shape={(props: unknown) => {
                  const { cx, cy, payload } = props as {
                    cx: number;
                    cy: number;
                    payload: Player;
                  };
                  const p = payload;
                  const us = p.camp === "us";
                  const color = CAMP_COLOR[p.camp];
                  const hot = active === p.name;
                  const left = p.side === "left";
                  return (
                    <g style={{ cursor: "pointer" }}>
                      {us && (
                        <circle cx={cx} cy={cy} r={18} fill="hsla(46,90%,60%,0.16)">
                          <animate
                            attributeName="r"
                            values="13;26;13"
                            dur="2.8s"
                            repeatCount="indefinite"
                          />
                          <animate
                            attributeName="opacity"
                            values="0.55;0.05;0.55"
                            dur="2.8s"
                            repeatCount="indefinite"
                          />
                        </circle>
                      )}
                      <circle
                        cx={cx}
                        cy={cy}
                        r={us ? 6.5 : hot ? 6 : 4.5}
                        fill={us ? color : "transparent"}
                        stroke={color}
                        strokeWidth={1.6}
                        style={us ? { filter: `drop-shadow(0 0 10px ${GOLD_GLOW})` } : undefined}
                      />
                      <text
                        x={cx + (left ? -11 : 11)}
                        y={cy + 4}
                        textAnchor={left ? "end" : "start"}
                        fontSize={us ? 13 : 11}
                        fontWeight={us ? 600 : 400}
                        fill={us ? color : hot ? INK : "rgba(233,237,255,0.55)"}
                        style={{ pointerEvents: "none", fontFamily: "var(--font-mono)" }}
                      >
                        {p.name}
                      </text>
                    </g>
                  );
                }}
              />
            </ScatterChart>
          </ResponsiveContainer>
        </div>

        <div className={`${FONT_MONO} mt-1 flex flex-wrap items-center justify-between gap-4 text-[11px]`} style={{ color: dim(0.4) }}>
          <span className="flex flex-wrap items-center gap-x-5 gap-y-2">
            <span className="flex items-center gap-2">
              <span className="inline-block h-2 w-2 rounded-full" style={{ background: ALPHA }} />
              respects permissions, believes nothing
            </span>
            <span className="flex items-center gap-2">
              <span className="inline-block h-2 w-2 rounded-full" style={{ background: MAGENTA }} />
              believes things, respects nothing
            </span>
            <span className="flex items-center gap-2">
              <span className="inline-block h-2 w-2 rounded-full" style={{ background: "rgba(233,237,255,0.35)" }} />
              neither
            </span>
          </span>
          <span className="hidden md:inline">hover any point for the reasoning</span>
        </div>
      </div>

      <p className={`${FONT_MONO} mt-4 max-w-3xl text-[11px] leading-relaxed`} style={{ color: dim(0.32) }}>
        Positions are our reading of each vendor&rsquo;s public documentation, not a
        benchmark — hover any point to see the specific doc language its score rests on.
        We would rather show you the reasoning and be argued with than publish a number
        you can&rsquo;t check.
      </p>
    </motion.div>
  );
}

function LocomoWar() {
  const data = LOCOMO_WAR.map((d, i) => ({ ...d, idx: i, label: `${d.when}` }));

  return (
    <motion.div
      variants={rise}
      className="mt-12 rounded-xl border p-6 md:p-8"
      style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
    >
      <div className={LABEL} style={{ color: dim(0.45) }}>
        one system · one benchmark · fourteen months
      </div>
      <div className="mt-6 h-[340px] w-full">
        <ResponsiveContainer>
          <BarChart data={data} margin={{ top: 30, right: 20, left: -20, bottom: 60 }}>
            <XAxis
              dataKey="label"
              tick={{ fill: "rgba(233,237,255,0.4)", fontSize: 10, fontFamily: "var(--font-mono)" }}
              axisLine={{ stroke: "rgba(233,237,255,0.15)" }}
              tickLine={false}
            />
            <YAxis
              domain={[0, 100]}
              tick={{ fill: "rgba(233,237,255,0.35)", fontSize: 10, fontFamily: "var(--font-mono)" }}
              axisLine={false}
              tickLine={false}
            />
            {/* The one honest line on the chart — gold, the canonical colour. */}
            <ReferenceLine
              y={LOCOMO_CEILING}
              stroke={GOLD}
              strokeDasharray="5 4"
              label={{
                value: `ceiling ${LOCOMO_CEILING}% — the answer key is 6.4% wrong`,
                position: "insideTopLeft",
                fill: GOLD,
                fontSize: 10,
                fontFamily: "var(--font-mono)",
              }}
            />
            <Tooltip
              cursor={{ fill: "rgba(255,255,255,0.03)" }}
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const d = payload[0].payload as (typeof data)[number];
                return (
                  <Tip
                    title={`${d.score}%`}
                    rows={[
                      ["", d.who],
                      ["when", d.when],
                    ]}
                  />
                );
              }}
            />
            {/* Nobody on this chart is a neutral party. Self-reported scores are
                the loudest colour; rival-measured ones are merely conflicted the
                other way. Gold — our canonical colour — is reserved for the
                ceiling, the only line here that isn't somebody's marketing. */}
            <Bar dataKey="score" radius={[3, 3, 0, 0]} maxBarSize={70}>
              {data.map((d) => (
                <RCell
                  key={d.who}
                  fill={d.tone === "self" ? "rgba(255,93,162,0.7)" : "hsla(190,90%,68%,0.55)"}
                />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>

      <div className="grid gap-3 md:grid-cols-5">
        {data.map((d) => (
          <div key={d.who} className={`${FONT_MONO} text-[10px] leading-relaxed`} style={{ color: dim(0.45) }}>
            <span style={{ color: d.tone === "self" ? MAGENTA : ALPHA }}>{d.score}%</span> — {d.who}
          </div>
        ))}
      </div>

      <div className={`${FONT_MONO} mt-5 flex flex-wrap gap-x-6 gap-y-2 text-[10px]`} style={{ color: dim(0.4) }}>
        <span className="flex items-center gap-2">
          <span className="inline-block h-2 w-2 rounded-full" style={{ background: MAGENTA }} />
          scored by the vendor itself
        </span>
        <span className="flex items-center gap-2">
          <span className="inline-block h-2 w-2 rounded-full" style={{ background: ALPHA }} />
          scored by its commercial rival
        </span>
        <span className="flex items-center gap-2">
          <span className="inline-block h-2 w-[10px]" style={{ borderTop: `2px dashed ${GOLD}` }} />
          what the benchmark can actually support
        </span>
      </div>

      <p className={`${FONT_MONO} mt-6 text-xs leading-relaxed`} style={{ color: dim(0.6) }}>
        The last two bars sit <em>at or above the mathematical ceiling</em> of a benchmark
        whose answer key is 6.4% wrong. A score above 93.6 on LOCOMO is not excellence. It
        is evidence of overfitting to a broken key.
      </p>
    </motion.div>
  );
}

function Matrix() {
  const mark = (c: Cell) =>
    c === "yes" ? "●" : c === "partial" ? "◐" : "○";
  const color = (c: Cell, us: boolean) =>
    c === "yes" ? (us ? GOLD : MINT) : c === "partial" ? dim(0.45) : dim(0.18);

  return (
    <motion.div variants={rise} className="mt-12 overflow-x-auto">
      <table className="w-full min-w-[720px] border-collapse">
        <thead>
          <tr>
            <th className="w-[40%] p-0" />
            {MATRIX_VENDORS.map((v) => {
              const us = v === "Brainiac";
              return (
                <th
                  key={v}
                  className={`${FONT_MONO} px-2 pb-4 text-center text-[10px] uppercase tracking-[0.14em] align-bottom`}
                  style={{ color: us ? GOLD : dim(0.42) }}
                >
                  {us ? <span className="font-semibold">{v}</span> : v}
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {MATRIX.map((row) => {
            const isEmptyRow = row.capability === EMPTY_ROW;
            return (
              <tr
                key={row.capability}
                style={{
                  background: isEmptyRow ? "hsla(46,90%,60%,0.05)" : undefined,
                }}
              >
                <td
                  className="border-t py-4 pr-6 align-top"
                  style={{ borderColor: "rgba(233,237,255,0.08)" }}
                >
                  <div
                    className="text-sm font-medium"
                    style={{ color: isEmptyRow ? GOLD : dim(0.9) }}
                  >
                    {row.capability}
                  </div>
                  <div className={`${FONT_MONO} mt-1 text-[11px] leading-relaxed`} style={{ color: dim(0.4) }}>
                    {row.detail}
                  </div>
                </td>
                {MATRIX_VENDORS.map((v) => {
                  const c = row.cells[v];
                  const us = v === "Brainiac";
                  return (
                    <td
                      key={v}
                      className="border-t px-2 py-4 text-center align-middle text-lg"
                      style={{
                        borderColor: "rgba(233,237,255,0.08)",
                        color: color(c, us),
                        background: us ? "hsla(46,90%,60%,0.04)" : undefined,
                      }}
                      title={c}
                    >
                      {mark(c)}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
      <div className={`${FONT_MONO} mt-5 flex flex-wrap gap-5 text-[11px]`} style={{ color: dim(0.4) }}>
        <span>● shipped</span>
        <span>◐ partial, or by convention rather than enforcement</span>
        <span>○ absent</span>
      </div>
    </motion.div>
  );
}

function RetrievalChart() {
  return (
    <motion.div
      variants={rise}
      className="rounded-xl border p-6"
      style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <div className={LABEL} style={{ color: MINT }}>
          retrieval quality · ndcg@10
        </div>
        <div className={`${FONT_MONO} text-[10px]`} style={{ color: dim(0.35) }}>
          {RETRIEVAL.model} · {RETRIEVAL.queries} queries
        </div>
      </div>

      <div className="mt-4 flex items-baseline gap-3">
        <span className="text-5xl font-semibold tracking-tight" style={{ color: MINT }}>
          {RETRIEVAL.ndcg.toFixed(3)}
        </span>
        <span className={`${FONT_MONO} text-xs`} style={{ color: dim(0.45) }}>
          overall · mrr {RETRIEVAL.mrr.toFixed(2)} · recall@5 {RETRIEVAL.recallAt5.toFixed(2)}
        </span>
      </div>

      <div className="mt-6 h-[240px] w-full">
        <ResponsiveContainer>
          <BarChart
            data={RETRIEVAL.strata}
            layout="vertical"
            margin={{ top: 0, right: 40, left: 90, bottom: 0 }}
          >
            <XAxis type="number" domain={[0, 1]} hide />
            <YAxis
              type="category"
              dataKey="name"
              tick={{ fill: "rgba(233,237,255,0.5)", fontSize: 11, fontFamily: "var(--font-mono)" }}
              axisLine={false}
              tickLine={false}
              width={90}
            />
            <Tooltip
              cursor={{ fill: "rgba(255,255,255,0.03)" }}
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const d = payload[0].payload as (typeof RETRIEVAL.strata)[number];
                return (
                  <Tip
                    title={d.name}
                    rows={[
                      ["ndcg@10", d.ndcg.toFixed(3)],
                      ["queries", String(d.n)],
                    ]}
                  />
                );
              }}
            />
            <Bar dataKey="ndcg" radius={[0, 3, 3, 0]} maxBarSize={22}>
              {RETRIEVAL.strata.map((s) => (
                <RCell key={s.name} fill={s.ndcg > 0.9 ? MINT : "hsla(158,80%,60%,0.5)"} />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>

      <p className={`${FONT_MONO} mt-3 text-[11px] leading-relaxed`} style={{ color: dim(0.42) }}>
        Strongest exactly where the thesis needs it: <span style={{ color: MINT }}>cross-team
        graph 0.93</span> — the case a per-repo file structurally cannot serve.
      </p>
    </motion.div>
  );
}

function StatTile({
  label,
  value,
  body,
  tone,
}: {
  label: string;
  value: string;
  body: string;
  tone: string;
}) {
  return (
    <motion.div
      variants={rise}
      className="rounded-xl border p-6"
      style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
    >
      <div className={LABEL} style={{ color: tone }}>
        {label}
      </div>
      <div className="mt-2 text-4xl font-semibold tracking-tight" style={{ color: tone }}>
        {value}
      </div>
      <p className={`${FONT_MONO} mt-3 text-[11px] leading-relaxed`} style={{ color: dim(0.5) }}>
        {body}
      </p>
    </motion.div>
  );
}

function UatCard({ journey }: { journey: (typeof UAT)[number] }) {
  const isControl = journey.gap === "none";
  const max = Math.max(...journey.arms.map((a) => a.tokens));

  return (
    <motion.div
      variants={rise}
      className="grid gap-8 rounded-xl border p-7 lg:grid-cols-[1fr_1.1fr]"
      style={{
        borderColor: isControl ? "rgba(233,237,255,0.10)" : "hsla(46,90%,68%,0.22)",
        background: isControl ? "rgba(255,255,255,0.015)" : "hsla(46,90%,60%,0.03)",
      }}
    >
      <div>
        <div className={LABEL} style={{ color: isControl ? dim(0.4) : GOLD }}>
          gap · {journey.gap}
          {isControl ? " · the control" : ""}
        </div>
        <h3 className="mt-3 text-2xl font-semibold leading-tight tracking-tight text-white">
          {journey.title}
        </h3>
        <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.55) }}>
          {journey.question}
        </p>
        <p
          className={`${FONT_MONO} mt-5 border-l pl-4 text-xs leading-relaxed`}
          style={{ borderColor: isControl ? dim(0.15) : GOLD, color: dim(0.72) }}
        >
          {journey.reading}
        </p>
      </div>

      <div className="space-y-3">
        {journey.arms.map((a) => {
          const isC = a.arm === "C";
          const ok = a.correct === true;
          const partial = a.correct === "partial";
          const tone = ok ? (isC ? GOLD : MINT) : partial ? dim(0.5) : MAGENTA;
          return (
            <div key={a.arm}>
              <div className="flex items-baseline justify-between gap-3">
                <span className={`${FONT_MONO} text-xs`} style={{ color: dim(0.75) }}>
                  <span style={{ color: tone }}>
                    {ok ? "✓" : partial ? "~" : "✗"}
                  </span>{" "}
                  <span style={{ color: isC ? GOLD : dim(0.75) }}>
                    {a.arm} · {a.label}
                  </span>
                </span>
                <span className={`${FONT_MONO} text-[10px]`} style={{ color: dim(0.4) }}>
                  {a.tokens.toLocaleString()} out-tok
                </span>
              </div>
              <div
                className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full"
                style={{ background: "rgba(233,237,255,0.06)" }}
              >
                <motion.div
                  className="h-full rounded-full"
                  style={{ background: tone, opacity: isC ? 1 : 0.55 }}
                  initial={{ width: 0 }}
                  whileInView={{ width: `${(a.tokens / max) * 100}%` }}
                  viewport={{ once: true }}
                  transition={{ duration: 0.8, ease: "easeOut" }}
                />
              </div>
              <div className={`${FONT_MONO} mt-1.5 text-[11px] leading-relaxed`} style={{ color: dim(0.5) }}>
                {a.verdict}
              </div>
            </div>
          );
        })}
      </div>
    </motion.div>
  );
}
