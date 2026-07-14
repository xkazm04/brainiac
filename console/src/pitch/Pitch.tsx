"use client";

/*
 * The pitch — rebuilt for a reader with a brain rather than a chequebook.
 *
 * The argument, in order:
 *   1. the gate       — the hero: nothing becomes org truth until someone signs
 *   2. the problem    — three STRUCTURAL failures, told as scenes, not statistics
 *   3. the gap        — the market has bifurcated and nobody has joined the halves
 *   4. how it works   — the mechanisms. This is the persuasion. It is checkable.
 *   5. the retreat    — four vendors who looked at governance and walked away
 *   6. the matrix     — the one row that is empty for everyone but us
 *   7. what we tested — a small controlled experiment, reported with its n
 *   8. where it loses — including the case where we are simply the wrong tool
 *
 * WHAT WAS CUT, AND WHY (see pitch-data.ts):
 *   - "benchmark theater". We attacked the market for staging accuracy numbers
 *     and then staged our own. Both halves are gone.
 *   - the borrowed industry stat tiles, and our own NDCG/F1 tiles. A number
 *     measured on a fixture org we wrote ourselves is not evidence that the
 *     design is right; the mechanism is.
 *
 * TYPOGRAPHY: body copy is never below text-sm. The 10–11px sizes survive only
 * inside the LABEL token — uppercase, tracked, instrument microcopy — which is
 * what that size is legible for.
 */

import { useCallback, useRef, useState } from "react";
import Link from "next/link";
import { motion } from "framer-motion";
import {
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
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  GOLD_GLOW,
  LABEL,
  MAGENTA,
  band,
} from "../design/theme";
import Divider from "../components/Divider";
import { MECHANISM_FIGURES, PROBLEM_FIGURES } from "./diagrams";
import { EvidenceMatrix } from "./sections/EvidenceMatrix";
import { LimitsBalanceSheet } from "./sections/LimitsBalanceSheet";
import { RetreatAutopsy } from "./sections/RetreatAutopsy";
import LedgerField from "./LedgerField";
import type { HeroStats } from "./hero-types";
import {
  BIFURCATION_LINE,
  EMPTY_ROW,
  KB_TEASER,
  MATRIX,
  MATRIX_VENDORS,
  MECHANISMS,
  PIPELINE_STAGES,
  PLAYERS,
  POISON,
  PROBLEMS,
  RETREAT_LEDE,
  TRIAL,
  type Cell,
  type Mechanism,
  type Player,
} from "./pitch-data";

const MINT = band("beta");
const ALPHA = band("alpha");
const INK = "#e9edff";
const dim = (a: number) => `rgba(233,237,255,${a})`;

const rise = {
  hidden: { opacity: 0, y: 16 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.5, ease: [0.2, 0.7, 0.3, 1] as const } },
};

/* ── shared furniture ─────────────────────────────────────────────────────── */

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

/** Body copy. Never smaller than this. */
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


function Tip({ title, body }: { title: string; body: string }) {
  return (
    <div
      className="max-w-sm rounded-lg border px-4 py-3 text-sm leading-relaxed shadow-2xl backdrop-blur"
      style={{
        borderColor: "rgba(233,237,255,0.16)",
        background: "rgba(10,9,16,0.95)",
        color: dim(0.75),
      }}
    >
      <div className="mb-1.5 font-semibold text-white">{title}</div>
      {body}
    </div>
  );
}

/* ── the page ─────────────────────────────────────────────────────────────── */

export default function Pitch() {
  const approveRef = useRef<(() => void) | null>(null);
  const [stats, setStats] = useState<HeroStats>({ queued: 0, canonical: 0 });
  const throttle = useRef(0);

  const onStats = useCallback((s: HeroStats) => {
    const now = performance.now();
    if (now - throttle.current < 200) return;
    throttle.current = now;
    setStats((prev) =>
      prev.queued === s.queued && prev.canonical === s.canonical ? prev : s,
    );
  }, []);

  return (
    <div className={`${FONT_DISPLAY}`} style={{ color: INK }}>
      {/* ─── 1. THE GATE ─────────────────────────────────────────────────── */}
      <section className="mx-auto max-w-6xl px-6 pt-10">
        <div
          className="relative overflow-hidden rounded-2xl border"
          style={{ borderColor: "rgba(233,237,255,0.10)" }}
        >
          {/* The field owns the masked layer; the copy sits above it. */}
          <div
            className="absolute inset-y-0 right-0 w-full md:w-[56%]"
            style={{
              maskImage: "linear-gradient(to right, transparent 0%, black 16%)",
              WebkitMaskImage: "linear-gradient(to right, transparent 0%, black 16%)",
            }}
          >
            <LedgerField
              onStats={onStats}
              onApproveRef={(fn) => {
                approveRef.current = fn;
              }}
            />
          </div>

          <div className="pointer-events-none relative z-10 flex h-[76vh] min-h-[580px] flex-col justify-between p-8 md:p-12">
            <motion.div
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.8 }}
              className="pointer-events-auto max-w-xl"
            >
              <div className={LABEL} style={{ color: GOLD }}>
                org memory · governed
              </div>
              <h1 className="mt-5 text-[2.6rem] font-semibold leading-[1.05] tracking-tight text-white lg:text-[3.4rem]">
                Nothing becomes
                <br />
                what your company
                <br />
                <span style={{ color: GOLD, textShadow: `0 0 48px ${GOLD_GLOW}` }}>knows</span>{" "}
                <span style={{ color: dim(0.5) }}>until</span>
                <br />
                someone signs it.
              </h1>
              <p className="mt-6 max-w-lg text-base leading-relaxed" style={{ color: dim(0.62) }}>
                Every other memory product lets a language model write an unattributed
                belief straight into your organisation&rsquo;s record, then serves it back
                as truth. Brainiac makes an agent <em>propose</em> and a named human{" "}
                <em>promote</em>.
              </p>

              <div className="mt-8 flex flex-wrap items-center gap-4">
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
                  ◉ sign the next claim
                </button>
                <a
                  href="#problem"
                  onClick={(e) => {
                    e.preventDefault();
                    document.getElementById("problem")?.scrollIntoView({ behavior: "smooth" });
                  }}
                  className={`${FONT_MONO} text-sm underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
                  style={{ color: dim(0.5) }}
                >
                  → the problem this solves
                </a>
              </div>
            </motion.div>

            <div
              className={`${LABEL} mt-8 flex flex-wrap items-center justify-between gap-3`}
              style={{ color: dim(0.35) }}
            >
              <span>
                <span style={{ color: dim(0.6) }}>{stats.queued}</span> unsigned ·{" "}
                <span style={{ color: GOLD }}>{stats.canonical}</span> signed ·{" "}
                <span style={{ color: MAGENTA }}>contested claims cannot be signed</span>
              </span>
            </div>
          </div>
        </div>
      </section>

      <Divider />

      {/* ─── 2. THE PROBLEM ──────────────────────────────────────────────── */}
      <Section id="problem" eyebrow="the problem" tone={MAGENTA}>
        <H2>Three failures that no amount of discipline fixes.</H2>
        <Lede>
          Not because your team is careless — because each failure is structural, and
          structure can be drawn. Three drawings; one sentence each on why they cannot be
          fixed with effort.
        </Lede>

        {/* Figure-first: the drawing carries the scene (its one-line caption
            just names it), and the only prose is the structural insight — the
            sentence a drawing cannot make. */}
        <div className="mt-14 space-y-6">
          {PROBLEMS.map((p, i) => {
            const Fig = PROBLEM_FIGURES[p.key];
            return (
              <motion.article
                key={p.key}
                variants={rise}
                className="grid items-center gap-10 rounded-xl border p-8 lg:grid-cols-[1.15fr_1fr]"
                style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
              >
                <div className={i % 2 ? "lg:order-2" : ""}>
                  <Fig />
                  <p className={`${FONT_MONO} mt-3 text-xs leading-relaxed`} style={{ color: dim(0.42) }}>
                    {p.scene}
                  </p>
                </div>
                <div className={i % 2 ? "lg:order-1" : ""}>
                  <div className="flex items-baseline gap-3">
                    <span className={LABEL} style={{ color: dim(0.3) }}>
                      {String(i + 1).padStart(2, "0")}
                    </span>
                    <h3 className="text-xl font-semibold leading-snug tracking-tight text-white">
                      {p.title}
                    </h3>
                  </div>
                  <p
                    className="mt-5 border-l-2 pl-4 text-base leading-relaxed"
                    style={{ borderColor: MAGENTA, color: dim(0.75) }}
                  >
                    {p.structural}
                  </p>
                </div>
              </motion.article>
            );
          })}
        </div>
      </Section>

      <Divider tone={ALPHA} />

      {/* ─── 3. THE GAP ──────────────────────────────────────────────────── */}
      <Section id="quadrant" eyebrow="the gap" tone={ALPHA}>
        <H2>Two industries. Neither one is memory you can trust.</H2>
        <Lede>{BIFURCATION_LINE}</Lede>
        <Quadrant />
      </Section>

      <Divider />

      {/* ─── 4. THE MECHANISMS ───────────────────────────────────────────── */}
      <Section id="mechanisms" eyebrow="how it works">
        <H2>Six mechanisms. Each one is a thing you can go and read.</H2>
        <Lede>
          This is the whole argument. Not a score on a benchmark we chose. The design,
          stated plainly enough that you can disagree with it.
        </Lede>

        <div className="mt-14 space-y-6">
          {MECHANISMS.map((m, i) => (
            <MechanismCard key={m.key} m={m} n={i + 1} />
          ))}
        </div>
      </Section>

      <Divider tone={MAGENTA} />

      {/* ─── 5. THE RETREAT ──────────────────────────────────────────────── */}
      <Section id="retreat" eyebrow="the retreat" tone={band("theta")}>
        <H2>They looked straight at this problem and walked away.</H2>
        <Lede>{RETREAT_LEDE}</Lede>

        <div className="mt-12">
          <RetreatAutopsy />
        </div>
      </Section>

      <Divider tone={ALPHA} />

      {/* ─── 6. THE MATRIX ───────────────────────────────────────────────── */}
      <Section id="matrix" eyebrow="the matrix" tone={ALPHA}>
        <H2>One row is empty for everyone but us.</H2>
        <Lede>
          Permission enforcement is filling in fast and will be commodity within a year. We
          do not rest the argument there. The row nobody fills is the review gate, and
          it is empty for a structural reason: this category&rsquo;s entire design centre
          is <em>fully automatic, invisible, zero-config</em>. A human in the loop looks to
          them like a bug.
        </Lede>
        <Matrix />
      </Section>

      <Divider tone={MINT} />

      {/* ─── 7. WHAT WE TESTED ───────────────────────────────────────────── */}
      <Section id="trial" eyebrow="what we actually tested" tone={MINT}>
        <H2>A small experiment, reported with its n — including the one we lost.</H2>
        <Lede>{TRIAL.design}</Lede>

        <motion.div
          variants={rise}
          className="mt-6 max-w-2xl rounded-xl border px-6 py-5"
          style={{ borderColor: "rgba(233,237,255,0.14)", background: "rgba(255,255,255,0.02)" }}
        >
          <div className={LABEL} style={{ color: dim(0.4) }}>
            the caveat, stated first
          </div>
          <p className="mt-3 text-sm leading-relaxed" style={{ color: dim(0.72) }}>
            {TRIAL.caveat}
          </p>
        </motion.div>

        <div className="mt-12">
          <EvidenceMatrix />
        </div>
      </Section>

      <Divider tone={MAGENTA} />

      {/* ─── the adversarial probe ───────────────────────────────────────── */}
      <Section id="poison" eyebrow="we tried to poison it" tone={MAGENTA}>
        <H2>We planted a false memory. It beat us. Then we fixed it.</H2>
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
              <p className="mt-4 text-sm leading-relaxed" style={{ color: dim(0.68) }}>
                {r.behavior}
              </p>
              <div
                className="mt-5 text-base font-semibold tracking-tight"
                style={{ color: r.tone === "good" ? GOLD : MAGENTA }}
              >
                {r.tone === "good" ? "✓ " : "✗ "}
                {r.outcome}
              </div>
              {i < 2 && (
                <span
                  aria-hidden
                  className={`${FONT_MONO} absolute -right-3 top-1/2 hidden text-lg md:block`}
                  style={{ color: dim(0.25) }}
                >
                  →
                </span>
              )}
            </motion.div>
          ))}
        </div>

        <motion.p
          variants={rise}
          className="mt-12 max-w-3xl border-l-2 pl-6 text-xl font-medium leading-snug tracking-tight text-white md:text-2xl"
          style={{ borderColor: GOLD }}
        >
          {POISON.moral}
        </motion.p>
      </Section>

      <Divider />

      {/* ─── 8. WHERE IT LOSES ───────────────────────────────────────────── */}
      <Section id="limits" eyebrow="where it loses" tone={dim(0.6)}>
        <H2>Where this is the wrong tool.</H2>
        <Lede>
          A design whose author cannot tell you when not to use it has not been thought
          about hard enough. Here is ours.
        </Lede>

        <div className="mt-14">
          <LimitsBalanceSheet />
        </div>
      </Section>

      <Divider />

      {/* ─── the pipeline + the wiki ─────────────────────────────────────── */}
      <Section id="pipeline" eyebrow="the shape of it" tone={ALPHA}>
        <H2>Session in. Signed, canonical knowledge out.</H2>
        <div className="mt-12 grid gap-px overflow-hidden rounded-xl border sm:grid-cols-2 lg:grid-cols-3" style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(233,237,255,0.08)" }}>
          {PIPELINE_STAGES.map((s) => (
            <motion.div key={s.n} variants={rise} className="p-7" style={{ background: "#08070c" }}>
              <div className={LABEL} style={{ color: GOLD }}>
                {s.n} · {s.name}
              </div>
              <p className="mt-3 text-sm leading-relaxed" style={{ color: dim(0.6) }}>
                {s.body}
              </p>
            </motion.div>
          ))}
        </div>
        <motion.p variants={rise} className="mt-8 max-w-2xl text-sm leading-relaxed" style={{ color: dim(0.5) }}>
          One Rust binary. Postgres is the only mandatory dependency. Your model, your
          keys, your own box. Nothing you ingest is ever shipped to us. It runs on 1 vCPU.
        </motion.p>

        <motion.div
          variants={rise}
          className="mt-14 rounded-xl border p-8"
          style={{ borderColor: "hsla(46,90%,68%,0.22)", background: "hsla(46,90%,60%,0.03)" }}
        >
          <div className={LABEL} style={{ color: GOLD }}>
            {KB_TEASER.status}
          </div>
          <h3 className="mt-4 max-w-2xl text-2xl font-semibold leading-snug tracking-tight text-white">
            {KB_TEASER.headline}
          </h3>
          <p className="mt-4 max-w-3xl text-sm leading-relaxed" style={{ color: dim(0.65) }}>
            {KB_TEASER.body}
          </p>
          <Link
            href={KB_TEASER.href}
            className={`${FONT_MONO} mt-6 inline-block text-sm underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
            style={{ color: GOLD }}
          >
            → the wiki that cannot rot
          </Link>
        </motion.div>
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
            background:
              "radial-gradient(ellipse at 50% 0%, hsla(46,90%,60%,0.10), transparent 70%)",
          }}
        >
          <h2 className="mx-auto max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white md:text-[2.75rem]">
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
              walk the demo org →
            </Link>
            <Link
              href="/console"
              className={`${FONT_MONO} rounded-full border px-7 py-3.5 text-sm transition hover:text-[#f3c74f]`}
              style={{ borderColor: "rgba(233,237,255,0.2)", color: dim(0.7) }}
            >
              sign in to the console
            </Link>
          </div>
          <p className={`${FONT_MONO} mt-8 text-sm`} style={{ color: dim(0.4) }}>
            MIT licensed · self-hosted · bring your own model
          </p>
        </motion.div>
      </section>
    </div>
  );
}

/* ── mechanisms ───────────────────────────────────────────────────────────── */

/**
 * A mechanism reads FIGURE-FIRST.
 *
 * The old card led with three paragraphs and buried a code block beside them —
 * which asked the reader to reconstruct a spatial idea ("the scan never sees the
 * row") out of prose. Now the diagram states the mechanism and the text is the
 * caption underneath it. The SQL/schema is still available, but folded away: it
 * is proof for the reader who wants it, not the thing they must read first.
 */
function MechanismCard({ m, n }: { m: Mechanism; n: number }) {
  const tone = band(m.band);
  const Figure = MECHANISM_FIGURES[m.key];

  return (
    <motion.article
      variants={rise}
      className="overflow-hidden rounded-xl border"
      style={{ borderColor: "rgba(233,237,255,0.10)", background: "rgba(255,255,255,0.02)" }}
    >
      <div className="grid items-center gap-10 p-8 lg:grid-cols-[1fr_1.1fr]">
        <div>
          <div className="flex items-baseline gap-3">
            <span className={LABEL} style={{ color: tone }}>
              {String(n).padStart(2, "0")}
            </span>
            <h3 className="text-xl font-semibold leading-snug tracking-tight text-white">
              {m.title}
            </h3>
          </div>

          <p className="mt-4 text-base leading-relaxed" style={{ color: dim(0.82) }}>
            {m.claim}
          </p>

          <div className="mt-6 border-l-2 pl-4" style={{ borderColor: MAGENTA }}>
            <div className={LABEL} style={{ color: MAGENTA }}>
              what everyone else does
            </div>
            <p className="mt-2.5 text-sm leading-relaxed" style={{ color: dim(0.6) }}>
              {m.instead}
            </p>
          </div>

          <details className="group mt-6">
            <summary
              className={`${FONT_MONO} cursor-pointer list-none text-sm underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
              style={{ color: dim(0.45) }}
            >
              how it works, precisely {m.artifact ? "· and the schema" : ""} ↓
            </summary>
            <p className="mt-4 text-sm leading-relaxed" style={{ color: dim(0.6) }}>
              {m.how}
            </p>
            {m.artifact && (
              <pre
                className={`${FONT_MONO} mt-4 overflow-x-auto rounded-lg border p-4 text-xs leading-relaxed`}
                style={{
                  borderColor: "rgba(233,237,255,0.08)",
                  background: "rgba(0,0,0,0.35)",
                  color: dim(0.72),
                }}
              >
                {m.artifact}
              </pre>
            )}
          </details>
        </div>

        <div className="min-w-0">{Figure ? <Figure /> : null}</div>
      </div>
    </motion.article>
  );
}

/* ── the quadrant ─────────────────────────────────────────────────────────── */

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
        className="rounded-xl border p-4 md:p-6"
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
                  value: "permission enforced by the database  →",
                  position: "insideBottom",
                  offset: -22,
                  style: { fill: dim(0.45), fontSize: 12 },
                }}
              />
              <YAxis
                type="number"
                dataKey="y"
                domain={[0, 10]}
                tick={false}
                axisLine={{ stroke: "rgba(233,237,255,0.18)" }}
                label={{
                  value: "truth is governed  →",
                  angle: -90,
                  position: "insideLeft",
                  offset: 0,
                  style: { fill: dim(0.45), fontSize: 12 },
                }}
              />
              <ZAxis range={[80, 80]} />

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
                  fontSize: 11,
                  letterSpacing: "0.18em",
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
                  return <Tip title={p.full} body={p.why} />;
                }}
              />
              <Scatter
                data={PLAYERS}
                onMouseEnter={(p) => setActive((p as unknown as Player).name)}
                onMouseLeave={() => setActive(null)}
                shape={(props: unknown) => {
                  const { cx, cy, payload } = props as { cx: number; cy: number; payload: Player };
                  const p = payload;
                  const us = p.camp === "us";
                  const color = CAMP_COLOR[p.camp];
                  const hot = active === p.name;
                  const left = p.side === "left";
                  return (
                    <g style={{ cursor: "pointer" }}>
                      {us && (
                        <circle cx={cx} cy={cy} r={18} fill="hsla(46,90%,60%,0.16)">
                          <animate attributeName="r" values="13;26;13" dur="2.8s" repeatCount="indefinite" />
                          <animate attributeName="opacity" values="0.55;0.05;0.55" dur="2.8s" repeatCount="indefinite" />
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
                        fontSize={us ? 14 : 12}
                        fontWeight={us ? 600 : 400}
                        fill={us ? color : hot ? INK : dim(0.6)}
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

        <div className={`${FONT_MONO} mt-1 flex flex-wrap items-center gap-x-6 gap-y-2 text-xs`} style={{ color: dim(0.45) }}>
          <span className="flex items-center gap-2">
            <span className="inline-block h-2 w-2 rounded-full" style={{ background: ALPHA }} />
            respects permissions · believes nothing
          </span>
          <span className="flex items-center gap-2">
            <span className="inline-block h-2 w-2 rounded-full" style={{ background: MAGENTA }} />
            believes things · respects nothing
          </span>
          <span className="ml-auto hidden md:inline">hover any point for the reasoning</span>
        </div>
      </div>

      <p className="mt-4 max-w-3xl text-sm leading-relaxed" style={{ color: dim(0.42) }}>
        Positions are our reading of each vendor&rsquo;s public documentation. Hover any point
        and you get the doc language its position rests on. We would rather show
        the reasoning and be argued with than publish a number you cannot check.
      </p>
    </motion.div>
  );
}

/* ── the matrix ───────────────────────────────────────────────────────────── */

function Matrix() {
  const mark = (c: Cell) => (c === "yes" ? "●" : c === "partial" ? "◐" : "○");
  const color = (c: Cell, us: boolean) =>
    c === "yes" ? (us ? GOLD : MINT) : c === "partial" ? dim(0.45) : dim(0.18);

  return (
    <motion.div variants={rise} className="mt-12 overflow-x-auto">
      <table className="w-full min-w-[760px] border-collapse">
        <thead>
          <tr>
            <th className="w-[38%] p-0" />
            {MATRIX_VENDORS.map((v) => {
              const us = v === "Brainiac";
              return (
                <th
                  key={v}
                  className={`${FONT_MONO} px-2 pb-4 align-bottom text-center text-xs`}
                  style={{ color: us ? GOLD : dim(0.45) }}
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
              <tr key={row.capability} style={{ background: isEmptyRow ? "hsla(46,90%,60%,0.05)" : undefined }}>
                <td className="border-t py-4 pr-6 align-top" style={{ borderColor: "rgba(233,237,255,0.08)" }}>
                  <div className="text-sm font-medium" style={{ color: isEmptyRow ? GOLD : dim(0.9) }}>
                    {row.capability}
                  </div>
                  <div className="mt-1 text-sm leading-relaxed" style={{ color: dim(0.42) }}>
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
      <div className={`${FONT_MONO} mt-5 flex flex-wrap gap-6 text-xs`} style={{ color: dim(0.45) }}>
        <span>● shipped</span>
        <span>◐ partial, or by convention rather than enforcement</span>
        <span>○ absent</span>
      </div>
    </motion.div>
  );
}

