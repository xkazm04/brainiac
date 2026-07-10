"use client";

/*
 * Variant B — "Cortex". Editorial-scientific: ivory paper, ink, one cobalt
 * accent and a coral counterpoint. Fraunces serif display, hairline rules,
 * numbered sections — the org's knowledge as a peer-reviewed lab notebook.
 * Motion is restrained: figures draw themselves, nothing loops forever.
 */

import { motion, useReducedMotion } from "framer-motion";
import { ArrowUpRight } from "lucide-react";
import {
  Bar,
  BarChart,
  Cell,
  LabelList,
  Line,
  LineChart,
  ResponsiveContainer,
  XAxis,
  YAxis,
} from "recharts";

import {
  CANONICAL_DEMO,
  CONTRADICTION,
  INGESTION_WEEKS,
  KPIS,
  QUEUE,
  STRATA,
} from "../demo-data";

const DISPLAY = "font-[family-name:var(--font-cortex-display)]";
const TEXT = "font-[family-name:var(--font-cortex-text)]";

const PAPER = "#faf7f0";
const INK = "#1c1b17";
const COBALT = "#1f3fbf";
const CORAL = "#d65a4a";
const RULE = "border-[#1c1b17]/15";

const rise = {
  hidden: { opacity: 0, y: 14 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.6, ease: [0.25, 0.5, 0.3, 1] as const } },
};
const stagger = { visible: { transition: { staggerChildren: 0.08 } } };

const CAPTION = `${TEXT} text-[11px] uppercase tracking-[0.18em] text-[#1c1b17]/50`;

/** Line-drawn hemisphere that sketches itself in, gyri first. */
function CortexFigure({ animate }: { animate: boolean }) {
  const strokes = [
    // outer hemisphere
    "M60 190 C 40 120, 95 48, 180 44 C 268 40, 330 96, 332 162 C 334 226, 288 284, 205 292 C 130 299, 78 252, 60 190 Z",
    // gyri folds
    "M92 150 C 120 122, 152 128, 168 150 C 184 172, 214 168, 228 144",
    "M100 208 C 132 186, 158 196, 176 218 C 194 240, 228 234, 244 208",
    "M148 84 C 172 100, 198 96, 216 76",
    "M238 100 C 262 118, 288 116, 302 96",
    "M256 176 C 280 160, 300 164, 312 184",
    // brain stem
    "M188 292 C 190 312, 200 326, 218 334",
  ];
  return (
    <svg viewBox="0 0 380 360" className="h-full w-full" role="img" aria-label="Figure 1 — the organizational cortex">
      {strokes.map((d, i) => (
        <motion.path
          key={i}
          d={d}
          fill="none"
          stroke={INK}
          strokeWidth={i === 0 ? 2 : 1.4}
          strokeLinecap="round"
          initial={animate ? { pathLength: 0 } : false}
          animate={{ pathLength: 1 }}
          transition={{ duration: 1.1, delay: 0.15 + i * 0.18, ease: "easeInOut" }}
        />
      ))}
      {/* annotation: the canonical node, marked like a specimen */}
      <motion.g
        initial={animate ? { opacity: 0 } : false}
        animate={{ opacity: 1 }}
        transition={{ delay: 1.9, duration: 0.5 }}
      >
        <circle cx="228" cy="144" r="5" fill={COBALT} />
        <line x1="233" y1="139" x2="292" y2="84" stroke={COBALT} strokeWidth="1" />
        <text x="296" y="80" fontSize="11" fill={COBALT} fontStyle="italic">
          fig. 1a — canonical
        </text>
      </motion.g>
      <motion.g
        initial={animate ? { opacity: 0 } : false}
        animate={{ opacity: 1 }}
        transition={{ delay: 2.2, duration: 0.5 }}
      >
        <circle cx="176" cy="218" r="5" fill="none" stroke={CORAL} strokeWidth="1.5" />
        <line x1="171" y1="224" x2="112" y2="282" stroke={CORAL} strokeWidth="1" />
        <text x="30" y="296" fontSize="11" fill={CORAL} fontStyle="italic">
          fig. 1b — under review
        </text>
      </motion.g>
    </svg>
  );
}

function SectionHead({ n, title }: { n: string; title: string }) {
  return (
    <div className={`flex items-baseline gap-4 border-t ${RULE} pt-4`}>
      <span className={`${DISPLAY} text-sm italic text-[#1c1b17]/40`}>{n}</span>
      <h2 className={`${DISPLAY} text-2xl text-[#1c1b17]`}>{title}</h2>
    </div>
  );
}

export default function CortexVariant() {
  const reduce = useReducedMotion();
  const animate = !reduce;
  return (
    <div className={`${TEXT} min-h-screen`} style={{ background: PAPER, color: INK }}>
      <div className="mx-auto max-w-6xl px-6">
        {/* masthead */}
        <nav className={`flex items-center justify-between border-b ${RULE} py-5`} aria-label="Cortex">
          <div className="flex items-baseline gap-3">
            <span className={`${DISPLAY} text-2xl font-medium tracking-tight`}>Brainiac</span>
            <span className={CAPTION}>the organizational cortex</span>
          </div>
          <div className="flex items-center gap-7 text-sm">
            {["Reviews", "Graph", "Analytics"].map((l) => (
              <span key={l} className="group relative cursor-pointer">
                {l}
                <span className="absolute -bottom-0.5 left-0 h-px w-0 bg-[#1f3fbf] transition-all duration-300 group-hover:w-full" />
              </span>
            ))}
            <span className="cursor-pointer border border-[#1c1b17] px-4 py-1.5 text-sm transition hover:bg-[#1c1b17] hover:text-[#faf7f0]">
              Open console
            </span>
          </div>
        </nav>

        {/* hero — a journal spread */}
        <motion.section
          initial="hidden"
          animate="visible"
          variants={stagger}
          className="grid items-center gap-12 py-16 lg:grid-cols-[1.1fr_0.9fr]"
        >
          <div>
            <motion.div variants={rise} className={CAPTION}>
              vol. 1 · meridian fintech · week 28
            </motion.div>
            <motion.h1 variants={rise} className={`${DISPLAY} mt-5 text-6xl leading-[1.02] tracking-tight lg:text-7xl`}>
              What the org
              <br />
              <em className="text-[#1f3fbf]">knows,</em> governed
              <br />
              like code.
            </motion.h1>
            <motion.p variants={rise} className="mt-6 max-w-md text-lg leading-relaxed text-[#1c1b17]/65">
              Every session distilled into reviewed, versioned, permission-aware
              memory — with provenance for every claim, like citations in a journal.
            </motion.p>
            <motion.div variants={rise} className="mt-9 flex items-center gap-6">
              <button className="group inline-flex items-center gap-2 bg-[#1c1b17] px-6 py-3 font-medium text-[#faf7f0] transition hover:bg-[#1f3fbf]">
                Review today&apos;s findings
                <ArrowUpRight size={16} className="transition group-hover:translate-x-0.5 group-hover:-translate-y-0.5" />
              </button>
              <span className="cursor-pointer text-sm underline decoration-[#1c1b17]/30 underline-offset-4 transition hover:decoration-[#1f3fbf] hover:text-[#1f3fbf]">
                Read the method
              </span>
            </motion.div>
            {/* footnote KPIs */}
            <motion.div variants={rise} className={`mt-12 grid grid-cols-2 gap-x-8 gap-y-4 border-t ${RULE} pt-5 sm:grid-cols-4`}>
              {KPIS.map((k, i) => (
                <div key={k.label}>
                  <div className={`${DISPLAY} text-3xl`}>{k.value}</div>
                  <div className={CAPTION}>
                    <sup>{i + 1}</sup> {k.label}
                  </div>
                </div>
              ))}
            </motion.div>
          </div>
          <motion.div variants={rise} className="relative mx-auto w-full max-w-[380px]">
            <CortexFigure animate={animate} />
            <div className={`${CAPTION} mt-2 text-center`}>figure 1 — the organizational cortex</div>
          </motion.div>
        </motion.section>

        {/* section 2: retrieval quality — the honest chart */}
        <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="pb-14">
          <motion.div variants={rise}>
            <SectionHead n="§2" title="Retrieval, measured" />
          </motion.div>
          <div className="mt-6 grid gap-10 lg:grid-cols-2">
            <motion.div variants={rise}>
              <div className={CAPTION}>NDCG@10 by stratum — qwen text-embedding-v4 (cobalt) vs deterministic baseline (dotted)</div>
              <div className="mt-4 h-64">
                <ResponsiveContainer>
                  <BarChart data={[...STRATA]} layout="vertical" margin={{ top: 0, right: 44, left: 8, bottom: 0 }}>
                    <XAxis type="number" domain={[0, 1]} hide />
                    <YAxis type="category" dataKey="name" width={82} tickLine={false} axisLine={false} fontSize={12} stroke={INK} />
                    <Bar dataKey="qwen" barSize={12} radius={0}>
                      <LabelList
                        dataKey="qwen"
                        position="right"
                        fontSize={11}
                        fill={INK}
                        formatter={(v) => (typeof v === "number" ? v.toFixed(2) : String(v ?? ""))}
                      />
                      {STRATA.map((s) => (
                        <Cell key={s.name} fill={s.qwen > 0.9 ? INK : COBALT} />
                      ))}
                    </Bar>
                    <Bar dataKey="baseline" barSize={3} fill={INK} opacity={0.25} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </motion.div>
            <motion.div variants={rise}>
              <div className={CAPTION}>memories captured (ink) · promoted to canonical (cobalt) / week</div>
              <div className="mt-4 h-64">
                <ResponsiveContainer>
                  <LineChart data={[...INGESTION_WEEKS]} margin={{ top: 8, right: 8, left: -24, bottom: 0 }}>
                    <XAxis dataKey="week" tickLine={false} axisLine={{ stroke: INK, strokeWidth: 1 }} fontSize={12} stroke={INK} />
                    <YAxis tickLine={false} axisLine={false} fontSize={12} stroke={INK} opacity={0.5} />
                    <Line type="linear" dataKey="captured" stroke={INK} strokeWidth={1.5} dot={{ r: 3, fill: PAPER, strokeWidth: 1.5 }} />
                    <Line type="linear" dataKey="promoted" stroke={COBALT} strokeWidth={1.5} dot={{ r: 3, fill: COBALT }} />
                  </LineChart>
                </ResponsiveContainer>
              </div>
              <p className="mt-3 max-w-md text-sm leading-relaxed text-[#1c1b17]/60">
                Promotion is deliberate: a third of captured knowledge earns canonical
                status, each promotion signed by a maintainer. The rest waits, cited but
                unratified.
              </p>
            </motion.div>
          </div>
        </motion.section>

        {/* section 3: peer review */}
        <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="pb-14">
          <motion.div variants={rise}>
            <SectionHead n="§3" title="Awaiting peer review" />
          </motion.div>
          <div className="mt-6 grid gap-px overflow-hidden border border-[#1c1b17]/15 bg-[#1c1b17]/15 lg:grid-cols-3">
            {QUEUE.map((q, i) => (
              <motion.article key={q.id} variants={rise} className="group bg-[#faf7f0] p-6 transition hover:bg-white">
                <div className="flex items-baseline justify-between">
                  <span className={`${DISPLAY} italic text-[#1c1b17]/40`}>{String(i + 1).padStart(2, "0")}</span>
                  <span className={CAPTION}>
                    {q.kind} · {q.team} · {q.age}
                  </span>
                </div>
                <p className={`${DISPLAY} mt-4 text-lg leading-snug`}>{q.content}</p>
                <div className="mt-5 flex items-center gap-4 text-sm">
                  <button className="border-b border-[#1f3fbf] pb-0.5 font-medium text-[#1f3fbf] transition hover:border-b-2">
                    Ratify
                  </button>
                  <button className="border-b border-transparent pb-0.5 text-[#1c1b17]/50 transition hover:border-[#d65a4a] hover:text-[#d65a4a]">
                    Decline
                  </button>
                  <span className={`${CAPTION} ml-auto`}>{q.rule}</span>
                </div>
              </motion.article>
            ))}
          </div>
        </motion.section>

        {/* section 4: errata + lexicon, two columns */}
        <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="grid gap-10 pb-16 lg:grid-cols-2">
          <motion.div variants={rise}>
            <SectionHead n="§4" title="Errata" />
            <div className="mt-6 space-y-3">
              <p className="text-[#1c1b17]/45 line-through decoration-[#d65a4a]/60">{CONTRADICTION.a}</p>
              <p className="text-lg">
                {CONTRADICTION.b}
                <sup className="ml-1 text-[#1f3fbf]">[supersedes]</sup>
              </p>
              <p className={`${CAPTION} pt-1`}>{CONTRADICTION.suggestion}</p>
            </div>
          </motion.div>
          <motion.div variants={rise}>
            <SectionHead n="§5" title="Lexicon" />
            <div className="mt-6">
              <div className="flex flex-wrap items-baseline gap-x-3 gap-y-2">
                <span className={`${DISPLAY} text-3xl italic text-[#1f3fbf]`}>{CANONICAL_DEMO.name}</span>
                <span className="text-[#1c1b17]/40">n., canonical —</span>
              </div>
              <dl className="mt-3 space-y-1.5">
                {CANONICAL_DEMO.aliases.map((a) => (
                  <div key={a.team} className="flex gap-3 text-sm">
                    <dt className={`${CAPTION} w-20 shrink-0 pt-0.5`}>{a.team}</dt>
                    <dd>&ldquo;{a.name}&rdquo;</dd>
                  </div>
                ))}
              </dl>
              <p className="mt-4 max-w-sm text-sm leading-relaxed text-[#1c1b17]/60">
                Three teams, three dialects, one referent. The lexicon is maintained by
                soft merges — reversible with a single deletion, never by rewriting sources.
              </p>
            </div>
          </motion.div>
        </motion.section>

        <footer className={`flex items-baseline justify-between border-t ${RULE} py-6`}>
          <span className={CAPTION}>brainiac · issue №28 · zero leaks recorded</span>
          <span className={`${DISPLAY} italic text-[#1c1b17]/50`}>every claim cites its source.</span>
        </footer>
      </div>
    </div>
  );
}
