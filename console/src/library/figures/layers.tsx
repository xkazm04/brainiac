"use client";

/* The three layers — memories at the base, pages compiled from them, the
   Library ratified from them through one human gate — and the provenance
   arrows pointing back down. */

import { useReducedMotion } from "framer-motion";

import { GOLD } from "../../design/theme";
import { THETA, dim } from "../primitives";
import { Frame, MONO } from "./frame";

export function LayersFigure() {
  const reduce = !!useReducedMotion();
  return (
    <Frame viewBox="0 0 640 262" label="Memories at the base; pages compiled from them; the Library ratified from them through one human gate." minWidth={520}>
      {/* base: memories — the descriptive layer */}
      <rect x={20} y={186} width={600} height={56} rx={9} fill="hsla(262,90%,60%,0.05)" stroke="hsla(262,90%,72%,0.35)" />
      <text x={40} y={210} fontSize={11} fill="hsla(262,90%,78%,0.9)" fontFamily={MONO} letterSpacing="1.5">
        MEMORIES
      </text>
      <text x={40} y={227} fontSize={9} fill={dim(0.45)} fontFamily={MONO}>
        the descriptive layer — what happened, what is true · governed, permission-aware
      </text>
      {/* a few memory nodes */}
      {[380, 425, 470, 515, 560].map((x, i) => (
        <circle key={x} cx={x} cy={214} r={4} fill={i === 2 ? GOLD : dim(0.35)} />
      ))}

      {/* left: the knowledge base — compiled */}
      <rect x={20} y={24} width={280} height={92} rx={9} fill="hsla(46,90%,60%,0.04)" stroke="hsla(46,90%,68%,0.35)" />
      <text x={40} y={50} fontSize={11} fill={GOLD} fontFamily={MONO} letterSpacing="1.5">
        KNOWLEDGE BASE
      </text>
      <text x={40} y={67} fontSize={9} fill={dim(0.45)} fontFamily={MONO}>
        the compiled layer — what we know,
      </text>
      <text x={40} y={80} fontSize={9} fill={dim(0.45)} fontFamily={MONO}>
        assembled into pages that rebuild
      </text>
      <rect x={40} y={90} width={150} height={5} rx={2.5} fill={dim(0.2)} />
      <rect x={40} y={101} width={110} height={5} rx={2.5} fill={dim(0.14)} />

      {/* right: the library — normative */}
      <rect x={340} y={24} width={280} height={92} rx={9} fill="hsla(224,90%,60%,0.05)" stroke="hsla(224,90%,72%,0.4)" />
      <text x={360} y={50} fontSize={11} fill={THETA} fontFamily={MONO} letterSpacing="1.5">
        LIBRARY
      </text>
      <text x={360} y={67} fontSize={9} fill={dim(0.45)} fontFamily={MONO}>
        the normative layer — what we should
      </text>
      <text x={360} y={80} fontSize={9} fill={dim(0.45)} fontFamily={MONO}>
        do · standards per stack + agent skills
      </text>
      <rect x={360} y={90} width={64} height={12} rx={6} fill="hsla(224,90%,60%,0.12)" stroke={THETA} strokeWidth={0.8} />
      <text x={392} y={99} fontSize={7.5} textAnchor="middle" fill={THETA} fontFamily={MONO}>
        rule
      </text>
      <rect x={432} y={90} width={64} height={12} rx={6} fill="hsla(224,90%,60%,0.12)" stroke={THETA} strokeWidth={0.8} />
      <text x={464} y={99} fontSize={7.5} textAnchor="middle" fill={THETA} fontFamily={MONO}>
        skill
      </text>

      {/* compile flow: memories → pages (automatic, gold) */}
      <path d="M120,186 C 120,160 130,140 148,116" fill="none" stroke={GOLD} strokeWidth={1.4} markerEnd="url(#lib-arrow-gold)" />
      <text x={64} y={152} fontSize={8.5} fill={GOLD} fontFamily={MONO}>
        compiled
      </text>

      {/* ratify flow: memories → library, THROUGH the gate */}
      <path d="M480,186 C 480,170 480,162 480,152" fill="none" stroke={dim(0.35)} strokeWidth={1.3} />
      {/* the gate: a named human */}
      <rect x={448} y={130} width={64} height={22} rx={11} fill="#08070c" stroke={GOLD} strokeWidth={1.3} />
      <circle cx={464} cy={141} r={4.5} fill="none" stroke={GOLD} strokeWidth={1.2} />
      <path d="M457,148 a7,5 0 0 1 14,0" fill="none" stroke={GOLD} strokeWidth={1.2} />
      <text x={480} y={145} fontSize={8} fill={GOLD} fontFamily={MONO}>
        gate
      </text>
      <path d="M480,130 C 480,126 480,122 480,116" fill="none" stroke={GOLD} strokeWidth={1.4} markerEnd="url(#lib-arrow-gold)" />
      <text x={496} y={168} fontSize={8.5} fill={dim(0.5)} fontFamily={MONO}>
        ratified by a named human
      </text>

      {/* provenance: dashed, library → memories (a rule can say why) */}
      <path d="M420,116 C 408,146 402,166 400,184" fill="none" stroke={THETA} strokeWidth={1} strokeDasharray="3 4" markerEnd="url(#lib-arrow-theta)">
        {!reduce && <animate attributeName="stroke-dashoffset" from="28" to="0" dur="2.6s" repeatCount="indefinite" />}
      </path>
      <text x={296} y={168} fontSize={8.5} fill={THETA} fontFamily={MONO}>
        provenance
      </text>

      <defs>
        <marker id="lib-arrow-gold" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="7" markerHeight="7" orient="auto">
          <path d="M1,1 L7,4 L1,7 Z" fill={GOLD} />
        </marker>
        <marker id="lib-arrow-theta" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="7" markerHeight="7" orient="auto">
          <path d="M1,1 L7,4 L1,7 Z" fill={THETA} />
        </marker>
      </defs>
    </Frame>
  );
}
