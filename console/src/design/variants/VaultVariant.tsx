"use client";

/*
 * Variant C — "Vault". Terminal-archive brutalism: the GitOps angle taken
 * literally. Phosphor green + amber on graphite, IBM Plex Mono everywhere,
 * commit-log rhythm, status stamps, a circuit-brain with packets running
 * its traces. Knowledge as infrastructure you can diff, sign and roll back.
 */

import { useEffect, useState } from "react";
import { motion, useReducedMotion } from "framer-motion";
import {
  Bar,
  BarChart,
  Cell,
  ReferenceLine,
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

const MONO = "font-[family-name:var(--font-vault-mono)]";

const BG = "#0b0e0c";
const PANEL = "#101512";
const GREEN = "#4ade80";
const AMBER = "#fbbf24";
const DIM = "rgba(74,222,128,0.45)";
const BORDER = "border border-[#4ade80]/20";

const rise = {
  hidden: { opacity: 0, y: 10 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.35, ease: "easeOut" as const } },
};
const stagger = { visible: { transition: { staggerChildren: 0.06 } } };

function Stamp({ children, tone = "green" }: { children: string; tone?: "green" | "amber" | "red" }) {
  const c = tone === "green" ? GREEN : tone === "amber" ? AMBER : "#f87171";
  return (
    <span
      className="inline-block border px-1.5 py-px text-[10px] font-semibold uppercase tracking-[0.14em]"
      style={{ color: c, borderColor: `${c}66` }}
    >
      {children}
    </span>
  );
}

/** Circuit-brain: rectilinear traces in a hemisphere silhouette, packets running them. */
function CircuitBrain({ animate }: { animate: boolean }) {
  const traces = [
    "M60 180 L60 120 L120 120 L120 70 L190 70",
    "M190 70 L260 70 L260 110 L320 110 L320 170",
    "M60 180 L60 240 L110 240 L110 290 L180 290",
    "M180 290 L250 290 L250 250 L320 250 L320 170",
    "M120 120 L120 180 L190 180 L190 240 L250 250",
    "M190 70 L190 130 L250 130 L250 190 L320 190",
  ];
  const pads = [
    [60, 180], [120, 120], [190, 70], [260, 70], [320, 110], [320, 170],
    [60, 240], [110, 290], [180, 290], [250, 290], [320, 250], [190, 180],
    [250, 130], [250, 190],
  ] as const;
  return (
    <svg viewBox="0 0 380 360" className="h-full w-full" role="img" aria-label="Circuit brain — knowledge as infrastructure">
      {/* hemisphere outline, rectilinear */}
      <path
        d="M100 40 L280 40 L340 100 L340 260 L280 320 L100 320 L40 260 L40 100 Z"
        fill="none"
        stroke={DIM}
        strokeWidth="1"
        strokeDasharray="6 4"
      />
      {traces.map((d, i) => (
        <path key={`t${i}`} d={d} fill="none" stroke={DIM} strokeWidth="1.2" />
      ))}
      {animate &&
        traces.map((d, i) => (
          <circle key={`p${i}`} r="3" fill={i % 3 === 2 ? AMBER : GREEN}>
            <animateMotion dur={`${3 + (i % 3) * 0.8}s`} repeatCount="indefinite" begin={`${i * 0.5}s`} path={d} />
          </circle>
        ))}
      {pads.map(([x, y], i) => (
        <rect key={`pad${i}`} x={x - 3} y={y - 3} width="6" height="6" fill={BG} stroke={GREEN} strokeWidth="1" />
      ))}
      {/* the canonical register */}
      <g>
        <rect x="176" y="166" width="28" height="28" fill="none" stroke={AMBER} strokeWidth="1.5" />
        <text x="190" y="216" textAnchor="middle" fontSize="9" fill={AMBER} className={MONO}>
          CANON
        </text>
        {animate && (
          <rect x="176" y="166" width="28" height="28" fill="none" stroke={AMBER} strokeWidth="1">
            <animate attributeName="opacity" values="1;0.2;1" dur="2s" repeatCount="indefinite" />
          </rect>
        )}
      </g>
    </svg>
  );
}

function Cursor() {
  return <span className="inline-block h-[1.1em] w-[0.55em] translate-y-[0.18em] bg-[#4ade80]" style={{ animation: "blink 1.1s step-end infinite" }} />;
}

const COMMITS = [
  { sha: "8bb8f7c", msg: "promote(payments): pitfall — decline code 05 is issuer-side", who: "pay-lead", stamp: "canonical" as const },
  { sha: "7ceaa0c", msg: "supersede(payments): psp timeout 10s → 30s", who: "pipeline", stamp: "deprecated" as const },
  { sha: "8263b53", msg: "link(org): kafka ⇐ MSK cluster (platform)", who: "resolver", stamp: "linked" as const },
  { sha: "dfff2f7", msg: "reject(data): hallway rumor re: dagster migration", who: "data-lead", stamp: "rejected" as const },
];

export default function VaultVariant() {
  const reduce = useReducedMotion();
  const animate = !reduce;
  const [typed, setTyped] = useState(animate ? 0 : 999);
  const headline = "the org remembers.";
  useEffect(() => {
    if (!animate) return;
    if (typed >= headline.length) return;
    const t = setTimeout(() => setTyped((n) => n + 1), 55 + Math.random() * 45);
    return () => clearTimeout(t);
  }, [typed, animate]);

  return (
    <div className={`${MONO} min-h-screen text-[#d8e8dc]`} style={{ background: BG }}>
      {/* scanline + grid texture */}
      <div className="pointer-events-none fixed inset-0" aria-hidden>
        <div
          className="absolute inset-0 opacity-[0.04]"
          style={{ backgroundImage: `linear-gradient(${GREEN} 1px, transparent 1px), linear-gradient(90deg, ${GREEN} 1px, transparent 1px)`, backgroundSize: "42px 42px" }}
        />
        {animate && (
          <div
            className="absolute left-0 h-24 w-full opacity-[0.05]"
            style={{ background: `linear-gradient(transparent, ${GREEN}, transparent)`, animation: "scanline 9s linear infinite" }}
          />
        )}
      </div>

      <div className="relative mx-auto max-w-6xl px-6">
        {/* prompt bar */}
        <nav className={`flex items-center justify-between ${BORDER} mt-6 bg-[#101512] px-4 py-3`} aria-label="Vault">
          <div className="flex items-center gap-3 text-sm">
            <span className="text-[#4ade80]">▚</span>
            <span className="font-semibold text-[#4ade80]">brainiac</span>
            <span className="text-[#d8e8dc]/40">@meridian:~$</span>
            <span className="hidden text-[#d8e8dc]/70 sm:inline">memory status --governed</span>
          </div>
          <div className="flex items-center gap-5 text-xs uppercase tracking-widest text-[#d8e8dc]/50">
            <span className="cursor-pointer transition hover:text-[#4ade80]">[r]eviews</span>
            <span className="cursor-pointer transition hover:text-[#4ade80]">[g]raph</span>
            <span className="cursor-pointer transition hover:text-[#4ade80]">[a]nalytics</span>
            <Stamp tone="green">0 leaks</Stamp>
          </div>
        </nav>

        {/* hero */}
        <motion.section initial="hidden" animate="visible" variants={stagger} className="grid items-center gap-10 py-14 lg:grid-cols-[1.1fr_0.9fr]">
          <div>
            <motion.div variants={rise} className="text-xs uppercase tracking-[0.2em] text-[#d8e8dc]/40">
              // gitops for organizational AI knowledge
            </motion.div>
            <motion.h1 variants={rise} className="mt-6 text-5xl font-semibold leading-tight tracking-tight text-[#4ade80] lg:text-6xl">
              &gt; {headline.slice(0, typed)}
              <Cursor />
            </motion.h1>
            <motion.p variants={rise} className="mt-6 max-w-md text-base leading-relaxed text-[#d8e8dc]/60">
              Session learnings become commits. Commits get reviewed, signed and
              versioned. Agents pull exactly what their operator is cleared to read —
              nothing more, ever.
            </motion.p>
            <motion.div variants={rise} className="mt-8 flex flex-wrap gap-3">
              <button className="border border-[#4ade80] bg-[#4ade80]/10 px-5 py-2.5 text-sm font-semibold text-[#4ade80] transition hover:bg-[#4ade80] hover:text-[#0b0e0c]">
                $ review --pending 3
              </button>
              <button className="border border-[#d8e8dc]/20 px-5 py-2.5 text-sm text-[#d8e8dc]/70 transition hover:border-[#fbbf24]/60 hover:text-[#fbbf24]">
                $ memory context --task
              </button>
            </motion.div>
            {/* KPI registers */}
            <motion.div variants={rise} className="mt-10 grid grid-cols-2 gap-px bg-[#4ade80]/20 sm:grid-cols-4">
              {KPIS.map((k) => (
                <div key={k.label} className="bg-[#0b0e0c] p-3.5">
                  <div className="text-2xl font-semibold text-[#4ade80]">{k.value}</div>
                  <div className="mt-1 text-[10px] uppercase tracking-widest text-[#d8e8dc]/40">{k.label}</div>
                </div>
              ))}
            </motion.div>
          </div>
          <motion.div variants={rise} className={`${BORDER} relative mx-auto aspect-square w-full max-w-[380px] bg-[#101512] p-4`}>
            <div className="absolute left-3 top-2 text-[10px] uppercase tracking-widest text-[#d8e8dc]/30">fig — memory bus</div>
            <CircuitBrain animate={animate} />
          </motion.div>
        </motion.section>

        {/* commit log + charts */}
        <section className="grid gap-4 pb-4 lg:grid-cols-5">
          <motion.div initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className={`${BORDER} bg-[#101512] p-5 lg:col-span-3`}>
            <div className="flex items-center justify-between text-xs uppercase tracking-widest text-[#d8e8dc]/40">
              <span>$ git log --knowledge --oneline</span>
              <Stamp tone="amber">live</Stamp>
            </div>
            <div className="mt-4 space-y-2.5">
              {COMMITS.map((c) => (
                <motion.div key={c.sha} variants={rise} className="group flex items-start gap-3 border-b border-[#4ade80]/10 pb-2.5 text-sm last:border-0">
                  <span className="shrink-0 text-[#fbbf24]">{c.sha}</span>
                  <span className="flex-1 text-[#d8e8dc]/80 transition group-hover:text-[#d8e8dc]">{c.msg}</span>
                  <span className="hidden shrink-0 text-[#d8e8dc]/35 sm:inline">{c.who}</span>
                  <Stamp tone={c.stamp === "canonical" || c.stamp === "linked" ? "green" : c.stamp === "deprecated" ? "amber" : "red"}>
                    {c.stamp}
                  </Stamp>
                </motion.div>
              ))}
            </div>
            <div className="mt-4 h-40">
              <div className="mb-1 text-[10px] uppercase tracking-widest text-[#d8e8dc]/35">throughput: captured ░ vs promoted █ / week</div>
              <ResponsiveContainer>
                <BarChart data={[...INGESTION_WEEKS]} margin={{ top: 4, right: 0, left: -28, bottom: 0 }} barGap={2}>
                  <XAxis dataKey="week" tickLine={false} axisLine={{ stroke: DIM }} fontSize={10} stroke={DIM} />
                  <YAxis tickLine={false} axisLine={false} fontSize={10} stroke={DIM} />
                  <Bar dataKey="captured" fill={GREEN} opacity={0.25} />
                  <Bar dataKey="promoted" fill={GREEN} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </motion.div>

          {/* review queue as processes */}
          <motion.div initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className={`${BORDER} bg-[#101512] p-5 lg:col-span-2`}>
            <div className="text-xs uppercase tracking-widest text-[#d8e8dc]/40">$ review queue — 3 blocked on human</div>
            <div className="mt-4 space-y-3">
              {QUEUE.map((q, i) => (
                <motion.div key={q.id} variants={rise} className="border border-[#d8e8dc]/10 p-3 transition hover:border-[#fbbf24]/40">
                  <div className="flex items-center gap-2 text-[10px] uppercase tracking-widest text-[#d8e8dc]/40">
                    <span className="text-[#fbbf24]">PID {1040 + i}</span>
                    <span>{q.kind}/{q.team}</span>
                    <span className="ml-auto">{q.age}</span>
                  </div>
                  <p className="mt-1.5 text-sm leading-snug text-[#d8e8dc]/80">{q.content}</p>
                  <div className="mt-2 flex gap-2 text-xs">
                    <button className="border border-[#4ade80]/50 px-2.5 py-0.5 text-[#4ade80] transition hover:bg-[#4ade80]/15">y — promote</button>
                    <button className="border border-[#d8e8dc]/15 px-2.5 py-0.5 text-[#d8e8dc]/50 transition hover:border-[#f87171]/50 hover:text-[#f87171]">n — reject</button>
                  </div>
                </motion.div>
              ))}
            </div>
          </motion.div>
        </section>

        {/* diff + retrieval benchmark */}
        <motion.section initial="hidden" whileInView="visible" viewport={{ once: true, margin: "-60px" }} variants={stagger} className="grid gap-4 pb-14 lg:grid-cols-2">
          <motion.div variants={rise} className={`${BORDER} bg-[#101512] p-5`}>
            <div className="text-xs uppercase tracking-widest text-[#d8e8dc]/40">$ memory diff — contradiction #114</div>
            <div className="mt-4 space-y-1 text-sm">
              <div className="bg-[#f87171]/10 px-3 py-1.5 text-[#f87171]">- {CONTRADICTION.a}</div>
              <div className="bg-[#4ade80]/10 px-3 py-1.5 text-[#4ade80]">+ {CONTRADICTION.b}</div>
            </div>
            <div className="mt-3 text-xs text-[#fbbf24]">→ {CONTRADICTION.suggestion}</div>
            <div className="mt-4 border-t border-[#4ade80]/10 pt-3 text-xs text-[#d8e8dc]/50">
              <span className="text-[#d8e8dc]/70">$ entity resolve &quot;{CANONICAL_DEMO.aliases[1].name}&quot;</span>
              <br />
              <span className="text-[#4ade80]">canonical: {CANONICAL_DEMO.name}</span> · known as:{" "}
              {CANONICAL_DEMO.aliases.map((a) => `${a.team}:"${a.name}"`).join(" ")}
            </div>
          </motion.div>

          <motion.div variants={rise} className={`${BORDER} bg-[#101512] p-5`}>
            <div className="text-xs uppercase tracking-widest text-[#d8e8dc]/40">$ eval retrieval — NDCG@10 per stratum · gate ≥ 0.70</div>
            <div className="mt-4 h-56">
              <ResponsiveContainer>
                <BarChart data={[...STRATA]} layout="vertical" margin={{ top: 0, right: 34, left: 4, bottom: 0 }}>
                  <XAxis type="number" domain={[0, 1]} hide />
                  <YAxis type="category" dataKey="name" width={78} tickLine={false} axisLine={false} fontSize={11} stroke={DIM} />
                  <ReferenceLine x={0.7} stroke={AMBER} strokeDasharray="4 3" />
                  <Bar dataKey="qwen" barSize={10}>
                    {STRATA.map((s) => (
                      <Cell key={s.name} fill={s.qwen >= 0.7 ? GREEN : AMBER} />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
            <div className="text-xs text-[#d8e8dc]/50">
              all strata above gate · <span className="text-[#4ade80]">qwen:text-embedding-v4</span> · run 2026-07-10
            </div>
          </motion.div>
        </motion.section>

        <footer className="flex items-center justify-between border-t border-[#4ade80]/15 py-5 text-[11px] uppercase tracking-widest text-[#d8e8dc]/35">
          <span>brainiac vault · every fact signed · every merge reversible</span>
          <span className="text-[#4ade80]">uptime 99.98% · rls enforced</span>
        </footer>
      </div>
    </div>
  );
}
