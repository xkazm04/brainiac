"use client";

/*
 * Variant A — "Synapse". Dark neural-electric: deep ink-navy space, cyan →
 * violet aurora, glassmorphism panels, a living synapse network whose
 * signals fire along dendrites. The org's knowledge as a nervous system.
 * Fixed art direction → literal hexes on purpose (kp Spark convention).
 */

import { motion, useReducedMotion } from "framer-motion";
import { ArrowRight, GitBranch, ShieldCheck, Sparkles, Zap } from "lucide-react";
import {
  Area,
  AreaChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import {
  CANONICAL_DEMO,
  CONTRADICTION,
  INGESTION_WEEKS,
  KPIS,
  PIPELINE_STAGES,
  QUEUE,
} from "../demo-data";

const DISPLAY = "font-[family-name:var(--font-synapse-display)]";
const MONO = "font-[family-name:var(--font-synapse-mono)]";

const INK = "#05070f";
const CYAN = "#22d3ee";
const VIOLET = "#a78bfa";
const GLASS = "rounded-2xl border border-white/10 bg-white/[0.04] backdrop-blur-md";

const fadeUp = {
  hidden: { opacity: 0, y: 22 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.55, ease: [0.21, 0.6, 0.35, 1] as const } },
};
const stagger = { visible: { transition: { staggerChildren: 0.09 } } };

/** The brand mark: a hexagonal synapse node with orbiting dendrites. */
function SynapseMark({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden>
      <defs>
        <linearGradient id="syn-mark" x1="0" y1="0" x2="32" y2="32">
          <stop stopColor={CYAN} />
          <stop offset="1" stopColor={VIOLET} />
        </linearGradient>
      </defs>
      <path
        d="M16 3 27 9.5v13L16 29 5 22.5v-13L16 3Z"
        stroke="url(#syn-mark)"
        strokeWidth="1.6"
      />
      <circle cx="16" cy="16" r="4" fill="url(#syn-mark)">
        <animate attributeName="opacity" values="1;0.5;1" dur="2.4s" repeatCount="indefinite" />
      </circle>
      <circle cx="16" cy="7" r="1.6" fill={CYAN} />
      <circle cx="24" cy="21" r="1.6" fill={VIOLET} />
      <circle cx="8" cy="21" r="1.6" fill={CYAN} />
    </svg>
  );
}

/** Hero illustration: a synapse network with signals firing along paths. */
function SynapseNetwork({ animate }: { animate: boolean }) {
  const nodes = [
    { x: 210, y: 60, r: 7, c: CYAN },
    { x: 80, y: 130, r: 5, c: VIOLET },
    { x: 320, y: 120, r: 6, c: VIOLET },
    { x: 150, y: 230, r: 6, c: CYAN },
    { x: 300, y: 250, r: 8, c: CYAN },
    { x: 60, y: 300, r: 4, c: VIOLET },
    { x: 220, y: 330, r: 5, c: VIOLET },
  ];
  const links = [
    "M210 60 C 160 90, 120 100, 80 130",
    "M210 60 C 260 85, 290 95, 320 120",
    "M80 130 C 100 170, 120 200, 150 230",
    "M320 120 C 315 170, 310 210, 300 250",
    "M150 230 C 200 240, 250 245, 300 250",
    "M80 130 C 70 190, 62 250, 60 300",
    "M300 250 C 275 280, 250 310, 220 330",
    "M150 230 C 170 265, 195 300, 220 330",
  ];
  return (
    <svg viewBox="0 0 380 380" className="h-full w-full" role="img" aria-label="Synapse network of organizational knowledge">
      <defs>
        <linearGradient id="syn-link" x1="0" y1="0" x2="1" y2="1">
          <stop stopColor={CYAN} stopOpacity="0.55" />
          <stop offset="1" stopColor={VIOLET} stopOpacity="0.55" />
        </linearGradient>
        <filter id="syn-glow" x="-80%" y="-80%" width="260%" height="260%">
          <feGaussianBlur stdDeviation="4" result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>
      {links.map((d, i) => (
        <path key={`l${i}`} d={d} stroke="url(#syn-link)" strokeWidth="1" fill="none" opacity="0.5" />
      ))}
      {animate &&
        links.map((d, i) => (
          <path
            key={`s${i}`}
            d={d}
            stroke={i % 2 ? VIOLET : CYAN}
            strokeWidth="2"
            strokeLinecap="round"
            fill="none"
            strokeDasharray="14 206"
            style={{
              animation: `signal 3.4s linear infinite`,
              animationDelay: `${i * 0.45}s`,
            }}
            filter="url(#syn-glow)"
          />
        ))}
      {nodes.map((n, i) => (
        <g key={`n${i}`} filter="url(#syn-glow)">
          <circle cx={n.x} cy={n.y} r={n.r} fill={n.c}>
            {animate && (
              <animate
                attributeName="opacity"
                values="1;0.45;1"
                dur={`${2 + (i % 3)}s`}
                repeatCount="indefinite"
              />
            )}
          </circle>
        </g>
      ))}
      {/* the canonical hub */}
      <g filter="url(#syn-glow)">
        <circle cx="300" cy="250" r="16" fill="none" stroke={CYAN} strokeWidth="1" opacity="0.6">
          {animate && (
            <animate attributeName="r" values="14;20;14" dur="3s" repeatCount="indefinite" />
          )}
        </circle>
      </g>
    </svg>
  );
}

function ChartTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: { name: string; value: number }[];
  label?: string;
}) {
  if (!active || !payload?.length) return null;
  return (
    <div className={`${GLASS} ${MONO} px-3 py-2 text-xs text-white/90`}>
      <div className="text-white/50">{label}</div>
      {payload.map((p) => (
        <div key={p.name}>
          {p.name}: <span className="text-cyan-300">{p.value}</span>
        </div>
      ))}
    </div>
  );
}

export default function SynapseVariant() {
  const reduce = useReducedMotion();
  const animate = !reduce;
  return (
    <div className={`${DISPLAY} min-h-screen text-white`} style={{ background: INK }}>
      {/* aurora background */}
      <div className="pointer-events-none fixed inset-0 overflow-hidden" aria-hidden>
        <div
          className="absolute -top-40 left-1/4 h-[480px] w-[620px] rounded-full opacity-25 blur-3xl"
          style={{ background: `radial-gradient(closest-side, ${CYAN}, transparent)` , animation: animate ? "drift 14s ease-in-out infinite" : undefined }}
        />
        <div
          className="absolute top-1/3 -right-32 h-[420px] w-[520px] rounded-full opacity-20 blur-3xl"
          style={{ background: `radial-gradient(closest-side, ${VIOLET}, transparent)`, animation: animate ? "drift 18s ease-in-out infinite reverse" : undefined }}
        />
      </div>

      <div className="relative mx-auto max-w-6xl px-6">
        {/* nav */}
        <nav className="flex items-center justify-between py-6" aria-label="Synapse">
          <div className="flex items-center gap-2.5">
            <SynapseMark />
            <span className="text-lg font-semibold tracking-tight">Brainiac</span>
            <span className={`${MONO} ml-2 rounded-full border border-cyan-400/30 bg-cyan-400/10 px-2 py-0.5 text-[10px] uppercase tracking-widest text-cyan-300`}>
              meridian
            </span>
          </div>
          <div className={`${MONO} flex items-center gap-6 text-sm text-white/60`}>
            <span className="cursor-pointer transition hover:text-cyan-300">Reviews</span>
            <span className="cursor-pointer transition hover:text-cyan-300">Graph</span>
            <span className="cursor-pointer transition hover:text-cyan-300">Analytics</span>
            <span className="cursor-pointer rounded-lg border border-white/15 bg-white/5 px-3 py-1.5 text-white transition hover:border-cyan-400/50 hover:text-cyan-300">
              ⌘K
            </span>
          </div>
        </nav>

        {/* hero */}
        <motion.section
          initial="hidden"
          animate="visible"
          variants={stagger}
          className="grid items-center gap-10 py-14 lg:grid-cols-[1.15fr_0.85fr]"
        >
          <div>
            <motion.div variants={fadeUp}>
              <span className={`${MONO} inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/5 px-3 py-1 text-xs text-white/70`}>
                <Zap size={12} className="text-cyan-300" />
                6 signals fired in the last hour
              </span>
            </motion.div>
            <motion.h1 variants={fadeUp} className="mt-6 text-5xl font-semibold leading-[1.04] tracking-tight lg:text-6xl">
              Your org has a<br />
              <span className="bg-gradient-to-r from-cyan-300 via-sky-300 to-violet-300 bg-clip-text text-transparent">
                nervous system
              </span>{" "}
              now.
            </motion.h1>
            <motion.p variants={fadeUp} className="mt-5 max-w-md text-lg leading-relaxed text-white/55">
              Brainiac captures what your teams learn in every AI session, resolves it
              into one governed graph, and fires it back into the next session.
            </motion.p>
            <motion.div variants={fadeUp} className="mt-8 flex flex-wrap items-center gap-4">
              <button className="group inline-flex items-center gap-2 rounded-xl bg-gradient-to-r from-cyan-400 to-violet-400 px-6 py-3 font-semibold text-slate-950 shadow-[0_0_32px_rgba(34,211,238,0.35)] transition hover:shadow-[0_0_48px_rgba(34,211,238,0.5)]">
                Open review queue
                <ArrowRight size={16} className="transition group-hover:translate-x-0.5" />
              </button>
              <button className={`${MONO} rounded-xl border border-white/15 px-5 py-3 text-sm text-white/80 transition hover:border-cyan-400/40 hover:text-cyan-300`}>
                memory_context()
              </button>
            </motion.div>
            {/* pipeline strip */}
            <motion.div variants={fadeUp} className={`${MONO} mt-10 flex flex-wrap items-center gap-1 text-[11px] uppercase tracking-wider text-white/40`}>
              {PIPELINE_STAGES.map((s, i) => (
                <span key={s} className="flex items-center gap-1">
                  <span className={i === 4 ? "text-cyan-300" : undefined}>{s}</span>
                  {i < PIPELINE_STAGES.length - 1 && <span className="text-white/20">→</span>}
                </span>
              ))}
            </motion.div>
          </div>
          <motion.div variants={fadeUp} className="relative mx-auto aspect-square w-full max-w-[380px]">
            <SynapseNetwork animate={animate} />
          </motion.div>
        </motion.section>

        {/* KPIs */}
        <motion.section
          initial="hidden"
          whileInView="visible"
          viewport={{ once: true, margin: "-60px" }}
          variants={stagger}
          className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4"
        >
          {KPIS.map((k) => (
            <motion.div key={k.label} variants={fadeUp} className={`${GLASS} group relative overflow-hidden p-5 transition hover:border-cyan-400/30`}>
              <div className="pointer-events-none absolute inset-0 -translate-x-full bg-gradient-to-r from-transparent via-white/[0.06] to-transparent transition group-hover:animate-[shimmer_1.2s_ease]" />
              <div className={`${MONO} text-[11px] uppercase tracking-widest text-white/45`}>{k.label}</div>
              <div className="mt-2 text-3xl font-semibold tracking-tight">{k.value}</div>
              <div className={`${MONO} mt-1 text-xs text-cyan-300/80`}>{k.delta}</div>
            </motion.div>
          ))}
        </motion.section>

        {/* charts + queue */}
        <section className="mt-6 grid gap-4 lg:grid-cols-5">
          <motion.div
            initial="hidden"
            whileInView="visible"
            viewport={{ once: true, margin: "-60px" }}
            variants={fadeUp}
            className={`${GLASS} p-6 lg:col-span-3`}
          >
            <div className="flex items-baseline justify-between">
              <h2 className="text-lg font-semibold">Knowledge flow</h2>
              <span className={`${MONO} text-xs text-white/40`}>captured vs promoted / week</span>
            </div>
            <div className="mt-4 h-56">
              <ResponsiveContainer>
                <AreaChart data={[...INGESTION_WEEKS]} margin={{ top: 6, right: 4, left: -22, bottom: 0 }}>
                  <defs>
                    <linearGradient id="syn-captured" x1="0" y1="0" x2="0" y2="1">
                      <stop stopColor={CYAN} stopOpacity={0.5} />
                      <stop offset="1" stopColor={CYAN} stopOpacity={0} />
                    </linearGradient>
                    <linearGradient id="syn-promoted" x1="0" y1="0" x2="0" y2="1">
                      <stop stopColor={VIOLET} stopOpacity={0.6} />
                      <stop offset="1" stopColor={VIOLET} stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <XAxis dataKey="week" stroke="rgba(255,255,255,0.25)" fontSize={11} tickLine={false} axisLine={false} />
                  <YAxis stroke="rgba(255,255,255,0.25)" fontSize={11} tickLine={false} axisLine={false} />
                  <Tooltip content={<ChartTooltip />} cursor={{ stroke: "rgba(255,255,255,0.15)" }} />
                  <Area type="monotone" dataKey="captured" stroke={CYAN} strokeWidth={2} fill="url(#syn-captured)" />
                  <Area type="monotone" dataKey="promoted" stroke={VIOLET} strokeWidth={2} fill="url(#syn-promoted)" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </motion.div>

          <motion.div
            initial="hidden"
            whileInView="visible"
            viewport={{ once: true, margin: "-60px" }}
            variants={stagger}
            className={`${GLASS} p-6 lg:col-span-2`}
          >
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold">Needs a human</h2>
              <span className={`${MONO} rounded-full bg-cyan-400/15 px-2 py-0.5 text-xs text-cyan-300`}>{QUEUE.length}</span>
            </div>
            <div className="mt-4 space-y-3">
              {QUEUE.map((q) => (
                <motion.div key={q.id} variants={fadeUp} className="rounded-xl border border-white/8 bg-white/[0.03] p-3.5 transition hover:border-violet-300/30">
                  <div className={`${MONO} flex items-center gap-2 text-[10px] uppercase tracking-widest text-white/40`}>
                    <span className="text-violet-300">{q.kind}</span>
                    <span>· {q.team}</span>
                    <span className="ml-auto">{q.age}</span>
                  </div>
                  <p className="mt-1.5 line-clamp-2 text-sm leading-snug text-white/80">{q.content}</p>
                  <div className="mt-2.5 flex gap-2">
                    <button className="rounded-lg bg-cyan-400/15 px-3 py-1 text-xs font-semibold text-cyan-300 transition hover:bg-cyan-400/25">
                      Promote
                    </button>
                    <button className="rounded-lg border border-white/10 px-3 py-1 text-xs text-white/60 transition hover:text-white">
                      Reject
                    </button>
                  </div>
                </motion.div>
              ))}
            </div>
          </motion.div>
        </section>

        {/* thesis row: canonical entity + contradiction */}
        <motion.section
          initial="hidden"
          whileInView="visible"
          viewport={{ once: true, margin: "-60px" }}
          variants={stagger}
          className="mt-4 grid gap-4 pb-16 lg:grid-cols-2"
        >
          <motion.div variants={fadeUp} className={`${GLASS} p-6`}>
            <div className="flex items-center gap-2 text-white/50">
              <GitBranch size={15} className="text-cyan-300" />
              <span className={`${MONO} text-[11px] uppercase tracking-widest`}>one entity, three dialects</span>
            </div>
            <div className="mt-4 flex items-center gap-3">
              <span className="rounded-xl bg-gradient-to-r from-cyan-400/20 to-violet-400/20 px-4 py-2 text-xl font-semibold text-cyan-200">
                {CANONICAL_DEMO.name}
              </span>
              <span className="text-white/30">=</span>
              <div className="flex flex-wrap gap-2">
                {CANONICAL_DEMO.aliases.map((a) => (
                  <span key={a.team} className={`${MONO} rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-xs text-white/70`}>
                    <span className="text-violet-300">{a.team}:</span> {a.name}
                  </span>
                ))}
              </div>
            </div>
            <p className="mt-4 text-sm leading-relaxed text-white/50">
              Teams keep their own names. Brainiac links them under one canonical node —
              reversible, auditable, never destructive.
            </p>
          </motion.div>

          <motion.div variants={fadeUp} className={`${GLASS} p-6`}>
            <div className="flex items-center gap-2 text-white/50">
              <ShieldCheck size={15} className="text-violet-300" />
              <span className={`${MONO} text-[11px] uppercase tracking-widest`}>contradiction detected</span>
            </div>
            <div className="mt-4 space-y-2">
              <div className="rounded-lg border border-white/8 bg-white/[0.03] px-3 py-2 text-sm text-white/60 line-through decoration-white/30">
                {CONTRADICTION.a}
              </div>
              <div className="rounded-lg border border-cyan-400/25 bg-cyan-400/5 px-3 py-2 text-sm text-white/85">
                {CONTRADICTION.b}
              </div>
            </div>
            <div className={`${MONO} mt-3 flex items-center gap-2 text-xs text-cyan-300/80`}>
              <Sparkles size={12} />
              {CONTRADICTION.suggestion}
            </div>
          </motion.div>
        </motion.section>

        <footer className={`${MONO} flex items-center justify-between border-t border-white/8 py-6 text-xs text-white/35`}>
          <span>brainiac · gitops for organizational AI knowledge</span>
          <span>0 RLS leaks · every fact traceable</span>
        </footer>
      </div>
    </div>
  );
}
