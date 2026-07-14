"use client";

/*
 * The asymmetry, drawn.
 *
 * Left: the canonical memory graph — the only source of truth. Right: a composed
 * page. One solid path runs left→right (compose). One dashed path runs right→left
 * and it is forced through the review gate (a human edit re-enters as candidate
 * memories). The direct right→left path is drawn struck out, because the fact
 * that it does NOT exist is the entire design.
 *
 * Deterministic SVG, no dependencies, no LLM — the same rule we apply to the
 * diagrams the product itself is allowed to generate (KB-PLAN D9).
 */

import { useReducedMotion } from "framer-motion";

import { FONT_MONO, GOLD, MAGENTA, band } from "../design/theme";

const MINT = band("beta");
const ALPHA = band("alpha");
const dim = (a: number) => `rgba(233,237,255,${a})`;

function MemoryNode({ y, label, lifecycle }: { y: number; label: string; lifecycle: string }) {
  const tone = lifecycle === "shipped" ? MINT : lifecycle === "in_flight" ? GOLD : dim(0.4);
  return (
    <g>
      <rect x={18} y={y} width={168} height={40} rx={6} fill="rgba(255,255,255,0.03)" stroke={dim(0.14)} />
      <circle cx={34} cy={y + 20} r={3.5} fill={tone} />
      <text x={46} y={y + 17} fontSize={9} fill={dim(0.75)} fontFamily="var(--font-mono)">
        {label}
      </text>
      <text x={46} y={y + 30} fontSize={8} fill={tone} fontFamily="var(--font-mono)">
        {lifecycle}
      </text>
    </g>
  );
}

export default function ProjectionDiagram() {
  const reduce = !!useReducedMotion();

  return (
    <div className="w-full overflow-x-auto">
      <svg
        viewBox="0 0 760 300"
        className="h-auto w-full min-w-[640px]"
        role="img"
        aria-label="Canonical memories compose into a page; a human edit to a page re-enters through extraction and the review gate; there is no direct write-back from a page to canonical memory."
      >
        <defs>
          <marker id="kb-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" fill={GOLD} />
          </marker>
          <marker id="kb-arrow-dim" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" fill={ALPHA} />
          </marker>
        </defs>

        {/* ── left: the source of truth ───────────────────────────────────── */}
        <text x={18} y={26} fontSize={9} letterSpacing="2" fill={GOLD} fontFamily="var(--font-mono)">
          CANONICAL MEMORY
        </text>
        <text x={18} y={40} fontSize={8} fill={dim(0.35)} fontFamily="var(--font-mono)">
          the only source of truth
        </text>
        <rect x={6} y={50} width={192} height={196} rx={10} fill="hsla(46,90%,60%,0.03)" stroke="hsla(46,90%,68%,0.22)" />
        <MemoryNode y={64} label="mem-pay-0043 · retry cap" lifecycle="shipped" />
        <MemoryNode y={116} label="mem-pay-0044 · psp swap" lifecycle="in_flight" />
        <MemoryNode y={168} label="mem-plat-0102 · dedup" lifecycle="shipped" />
        <text x={18} y={232} fontSize={8} fill={dim(0.35)} fontFamily="var(--font-mono)">
          signed · provenance · as-of
        </text>

        {/* ── compose: left → right, the only automatic path ──────────────── */}
        <path
          id="kb-compose"
          d="M204,148 C 300,148 320,110 400,110"
          fill="none"
          stroke={GOLD}
          strokeWidth={1.6}
          markerEnd="url(#kb-arrow)"
        />
        <text x={244} y={100} fontSize={9} fill={GOLD} fontFamily="var(--font-mono)">
          compose
        </text>
        <text x={244} y={112} fontSize={8} fill={dim(0.4)} fontFamily="var(--font-mono)">
          visibility-capped retrieval
        </text>
        {!reduce && (
          <circle r={3} fill={GOLD}>
            <animateMotion dur="3.2s" repeatCount="indefinite" path="M204,148 C 300,148 320,110 400,110" />
          </circle>
        )}

        {/* ── the path that does not exist ────────────────────────────────── */}
        <path d="M400,168 L204,168" fill="none" stroke={MAGENTA} strokeWidth={1.2} strokeDasharray="3 4" opacity={0.5} />
        <line x1={288} y1={158} x2={314} y2={178} stroke={MAGENTA} strokeWidth={2} />
        <line x1={314} y1={158} x2={288} y2={178} stroke={MAGENTA} strokeWidth={2} />
        <text x={244} y={196} fontSize={9} fill={MAGENTA} fontFamily="var(--font-mono)">
          no write-back
        </text>
        <text x={244} y={208} fontSize={8} fill={dim(0.4)} fontFamily="var(--font-mono)">
          no bidirectional sync, ever
        </text>

        {/* ── right: the page ────────────────────────────────────────────── */}
        <text x={412} y={26} fontSize={9} letterSpacing="2" fill={ALPHA} fontFamily="var(--font-mono)">
          COMPOSED PAGE
        </text>
        <text x={412} y={40} fontSize={8} fill={dim(0.35)} fontFamily="var(--font-mono)">
          a projection · regenerated, never authored
        </text>
        <rect x={400} y={50} width={352} height={196} rx={10} fill="rgba(255,255,255,0.02)" stroke={dim(0.12)} />

        <text x={416} y={76} fontSize={11} fill="#ffffff" fontFamily="var(--font-mono)">
          payments · refund worker
        </text>
        <line x1={416} y1={86} x2={736} y2={86} stroke={dim(0.1)} />

        <text x={416} y={106} fontSize={9} fill={dim(0.7)} fontFamily="var(--font-mono)">
          The retry cap is 30s.{" "}
          <tspan fill={GOLD}>[m:mem-pay-0043]</tspan>
        </text>
        <text x={416} y={124} fontSize={8} fill={dim(0.4)} fontFamily="var(--font-mono)">
          composed section · rebuilt when its memories change
        </text>

        <rect x={412} y={138} width={332} height={44} rx={5} fill="hsla(46,90%,60%,0.05)" stroke="hsla(46,90%,68%,0.2)" />
        <text x={424} y={156} fontSize={9} fill={GOLD} fontFamily="var(--font-mono)">
          on its way · not in production
        </text>
        <text x={424} y={172} fontSize={8} fill={dim(0.55)} fontFamily="var(--font-mono)">
          PSP swap decided, not shipped. [m:mem-pay-0044] · in_flight
        </text>

        <text x={416} y={204} fontSize={9} fill={dim(0.6)} fontFamily="var(--font-mono)">
          ▸ pinned section — human prose, never regenerated
        </text>
        <text x={416} y={224} fontSize={8} fill={dim(0.3)} fontFamily="var(--font-mono)">
          every claim cites a memory a named human signed for
        </text>

        {/* ── the edit, back through the gate ─────────────────────────────── */}
        <path
          d="M576,250 C 576,282 430,282 400,282 L 208,282"
          fill="none"
          stroke={ALPHA}
          strokeWidth={1.4}
          strokeDasharray="5 4"
          markerEnd="url(#kb-arrow-dim)"
        />
        <rect x={330} y={268} width={132} height={28} rx={5} fill="#08070c" stroke={ALPHA} />
        <text x={396} y={286} fontSize={9} textAnchor="middle" fill={ALPHA} fontFamily="var(--font-mono)">
          extraction → review gate
        </text>
        <text x={594} y={266} fontSize={9} fill={ALPHA} fontFamily="var(--font-mono)">
          a human edits the page
        </text>
        <text x={594} y={278} fontSize={8} fill={dim(0.4)} fontFamily="var(--font-mono)">
          captured as candidate memories
        </text>
      </svg>

      <p className={`${FONT_MONO} mt-4 text-[11px] leading-relaxed`} style={{ color: dim(0.35) }}>
        Drawn deterministically — the same rule the product holds its own diagrams to: compiled
        from structure, never imagined.
      </p>
    </div>
  );
}
