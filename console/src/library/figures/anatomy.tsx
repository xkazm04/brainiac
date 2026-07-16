"use client";

/* The five anatomy figures, one per property card:
   detector · rule-as-atom · provenance chain · skill shelf · vitals. */

import { useReducedMotion } from "framer-motion";

import { GOLD, MAGENTA } from "../../design/theme";
import { MINT, THETA, dim } from "../primitives";
import { Frame, MONO, plot } from "./frame";

/* ── The detector: two detuned practices, one recommended standard ────────── */

export function DetectorBeat() {
  const w = 340;
  const beat = (x: number) => 62 + Math.sin(x / 9) * Math.cos(x / 46) * 20;
  return (
    <Frame viewBox="0 0 340 132" label="Two slightly detuned practices produce a beat; the sweep names it and recommends one standard.">
      <path d={plot((x) => 26 + Math.sin(x / 10) * 9, 12, w - 116)} fill="none" stroke={dim(0.3)} strokeWidth={1.2} />
      <path d={plot((x) => 46 + Math.sin(x / 8.2) * 9, 12, w - 116)} fill="none" stroke={dim(0.3)} strokeWidth={1.2} />
      <path d={plot(beat, 12, w - 116)} fill="none" stroke={THETA} strokeWidth={1.8} />
      <text x={12} y={104} fontSize={8} fill={dim(0.45)} fontFamily={MONO}>
        same practice · two frequencies
      </text>
      <text x={12} y={116} fontSize={8} fill={THETA} fontFamily={MONO}>
        the beat — drift only audible org-wide
      </text>
      {/* the sweep's output */}
      <path d={`M${w - 112},62 L${w - 96},62`} stroke={MINT} strokeWidth={1.3} markerEnd="url(#det-arrow)" />
      <rect x={w - 92} y={40} width={84} height={44} rx={7} fill="hsla(158,90%,60%,0.06)" stroke={MINT} strokeWidth={1} />
      <text x={w - 50} y={58} fontSize={8} textAnchor="middle" fill={MINT} fontFamily={MONO}>
        divergence filed
      </text>
      <text x={w - 50} y={72} fontSize={8} textAnchor="middle" fill={dim(0.55)} fontFamily={MONO}>
        + one standard
      </text>
      <defs>
        <marker id="det-arrow" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="7" markerHeight="7" orient="auto">
          <path d="M1,1 L7,4 L1,7 Z" fill={MINT} />
        </marker>
      </defs>
    </Frame>
  );
}

/* ── The rule is the atom ─────────────────────────────────────────────────── */

export function RuleAtom() {
  return (
    <Frame viewBox="0 0 340 132" label="One rule as a card: statement, examples, binding strength, lifecycle, provenance, and its adoption pulse.">
      <rect x={12} y={10} width={316} height={112} rx={8} fill="rgba(255,255,255,0.02)" stroke="hsla(224,90%,72%,0.3)" />
      {/* lifecycle + enforcement chips */}
      <rect x={24} y={20} width={58} height={13} rx={6.5} fill="hsla(158,90%,60%,0.1)" stroke={MINT} strokeWidth={0.8} />
      <text x={53} y={29.5} fontSize={7.5} textAnchor="middle" fill={MINT} fontFamily={MONO}>
        adopted
      </text>
      <rect x={88} y={20} width={66} height={13} rx={6.5} fill="hsla(46,90%,60%,0.08)" stroke={GOLD} strokeWidth={0.8} />
      <text x={121} y={29.5} fontSize={7.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        mandatory
      </text>
      <text x={316} y={30} fontSize={7.5} textAnchor="end" fill={dim(0.4)} fontFamily={MONO}>
        stack / category / rule
      </text>

      {/* the statement */}
      <rect x={24} y={42} width={220} height={7} rx={3.5} fill={dim(0.32)} />
      <text x={24} y={64} fontSize={7.5} fill={dim(0.4)} fontFamily={MONO}>
        one sentence. one rule. individually addressed.
      </text>

      {/* good / bad examples */}
      <rect x={24} y={72} width={130} height={20} rx={4} fill="hsla(158,90%,60%,0.05)" stroke="hsla(158,90%,68%,0.3)" strokeWidth={0.8} />
      <text x={32} y={85} fontSize={7.5} fill={MINT} fontFamily={MONO}>
        ✓ the example to copy
      </text>
      <rect x={162} y={72} width={130} height={20} rx={4} fill="rgba(255,93,162,0.04)" strokeDasharray="3 2" stroke="rgba(255,93,162,0.4)" strokeWidth={0.8} />
      <text x={170} y={85} fontSize={7.5} fill={MAGENTA} fontFamily={MONO}>
        ✕ the one to retire
      </text>

      {/* provenance + pulse */}
      {[30, 42, 54].map((x) => (
        <circle key={x} cx={x} cy={106} r={3.2} fill="none" stroke="hsla(262,90%,72%,0.7)" strokeWidth={1} />
      ))}
      <text x={64} y={109} fontSize={7.5} fill={dim(0.45)} fontFamily={MONO}>
        the memories behind it
      </text>
      <path d={plot((x) => 106 - Math.max(0, Math.sin((x - 216) / 16)) * 7 - (x - 216) * 0.05, 216, 300, 3)} fill="none" stroke={THETA} strokeWidth={1.4} />
      <text x={316} y={109} fontSize={7.5} textAnchor="end" fill={THETA} fontFamily={MONO}>
        pulse
      </text>
    </Frame>
  );
}

/* ── No unattributed rules ────────────────────────────────────────────────── */

export function ProvenanceChain() {
  return (
    <Frame viewBox="0 0 340 132" label="A rule traces back to the incident and memories that motivated it — or carries the name of the human who decreed it.">
      {/* the incident */}
      <circle cx={40} cy={46} r={12} fill="rgba(255,93,162,0.08)" stroke={MAGENTA} strokeWidth={1.2} />
      <text x={40} y={50} fontSize={9} textAnchor="middle" fill={MAGENTA} fontFamily={MONO}>
        !
      </text>
      <text x={40} y={72} fontSize={7.5} textAnchor="middle" fill={dim(0.5)} fontFamily={MONO}>
        the incident
      </text>

      <path d="M56,46 L96,46" stroke={dim(0.3)} strokeWidth={1.2} markerEnd="url(#prov-arrow)" />

      {/* the memory */}
      <circle cx={116} cy={46} r={11} fill="hsla(262,90%,60%,0.08)" stroke="hsla(262,90%,72%,0.7)" strokeWidth={1.2} />
      <text x={116} y={72} fontSize={7.5} textAnchor="middle" fill={dim(0.5)} fontFamily={MONO}>
        what we learned
      </text>

      <path d="M131,46 L171,46" stroke={dim(0.3)} strokeWidth={1.2} markerEnd="url(#prov-arrow)" />

      {/* the rule */}
      <rect x={178} y={30} width={140} height={32} rx={7} fill="hsla(224,90%,60%,0.07)" stroke={THETA} strokeWidth={1.2} />
      <text x={248} y={45} fontSize={8.5} textAnchor="middle" fill={THETA} fontFamily={MONO}>
        the rule
      </text>
      <text x={248} y={56} fontSize={7} textAnchor="middle" fill={dim(0.4)} fontFamily={MONO}>
        …and it can say why
      </text>

      {/* the only alternative: a named decree */}
      <text x={40} y={104} fontSize={8} fill={dim(0.45)} fontFamily={MONO}>
        the only other kind:
      </text>
      <rect x={160} y={92} width={158} height={18} rx={9} fill="hsla(46,90%,60%,0.05)" stroke={GOLD} strokeWidth={0.9} />
      <text x={239} y={104} fontSize={7.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        decreed — signed with a name
      </text>
      <defs>
        <marker id="prov-arrow" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="7" markerHeight="7" orient="auto">
          <path d="M1,1 L7,4 L1,7 Z" fill={dim(0.45)} />
        </marker>
      </defs>
    </Frame>
  );
}

/* ── Skills are versioned bundles ─────────────────────────────────────────── */

export function SkillShelf() {
  const reduce = !!useReducedMotion();
  const bundles = [
    { x: 24, v: "v1.2", faded: true },
    { x: 116, v: "v1.3", faded: true },
    { x: 208, v: "v2.0", faded: false },
  ];
  return (
    <Frame viewBox="0 0 340 132" label="Versioned skill bundles on a shelf; an agent fetches the current one and usage flows back, counted by team.">
      {/* the shelf */}
      <line x1={16} y1={58} x2={324} y2={58} stroke={dim(0.2)} strokeWidth={1.2} />
      {bundles.map((b) => (
        <g key={b.v} opacity={b.faded ? 0.42 : 1}>
          <rect x={b.x} y={20} width={84} height={38} rx={6} fill={b.faded ? "rgba(255,255,255,0.02)" : "hsla(224,90%,60%,0.08)"} stroke={b.faded ? dim(0.25) : THETA} strokeWidth={b.faded ? 0.9 : 1.3} />
          <text x={b.x + 10} y={36} fontSize={8} fill={b.faded ? dim(0.5) : THETA} fontFamily={MONO}>
            skill · {b.v}
          </text>
          <rect x={b.x + 10} y={42} width={48} height={4} rx={2} fill={dim(0.18)} />
          <rect x={b.x + 10} y={49} width={34} height={4} rx={2} fill={dim(0.12)} />
        </g>
      ))}

      {/* fetch: current version down to the agent */}
      <path d="M250,58 C 250,74 236,84 214,90" fill="none" stroke={THETA} strokeWidth={1.4} markerEnd="url(#shelf-arrow)" />
      <text x={262} y={80} fontSize={7.5} fill={THETA} fontFamily={MONO}>
        fetched by name
      </text>

      {/* the agent */}
      <rect x={132} y={84} width={78} height={28} rx={7} fill="rgba(255,255,255,0.02)" stroke={dim(0.3)} />
      <text x={171} y={101} fontSize={8} textAnchor="middle" fill={dim(0.65)} fontFamily={MONO}>
        coding agent
      </text>

      {/* usage ticks flowing back, per team */}
      <path d="M132,96 C 92,96 72,80 66,62" fill="none" stroke={MINT} strokeWidth={1} strokeDasharray="2 4" markerEnd="url(#shelf-mint)">
        {!reduce && <animate attributeName="stroke-dashoffset" from="24" to="0" dur="2.2s" repeatCount="indefinite" />}
      </path>
      <text x={20} y={124} fontSize={7.5} fill={MINT} fontFamily={MONO}>
        usage flows back — counted by team, never by name
      </text>
      <defs>
        <marker id="shelf-arrow" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="7" markerHeight="7" orient="auto">
          <path d="M1,1 L7,4 L1,7 Z" fill={THETA} />
        </marker>
        <marker id="shelf-mint" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="6" markerHeight="6" orient="auto">
          <path d="M1,1 L7,4 L1,7 Z" fill={MINT} />
        </marker>
      </defs>
    </Frame>
  );
}

/* ── Adoption is a vital sign ─────────────────────────────────────────────── */

export function VitalsFigure() {
  return (
    <Frame viewBox="0 0 340 132" label="One rule's adoption rises; another flatlines and is flagged as a deprecation candidate on its own.">
      {/* alive rule */}
      <text x={16} y={26} fontSize={8} fill={dim(0.5)} fontFamily={MONO}>
        rule a — practice follows it
      </text>
      <path d={plot((x) => 48 - (x - 16) * 0.09 - Math.max(0, Math.sin(x / 14)) * 4, 16, 236, 3)} fill="none" stroke={MINT} strokeWidth={1.6} />
      <circle cx={236} cy={48 - 220 * 0.09 - 0} r={3} fill={MINT} />
      <text x={248} y={32} fontSize={7.5} fill={MINT} fontFamily={MONO}>
        adopted, and
      </text>
      <text x={248} y={42} fontSize={7.5} fill={MINT} fontFamily={MONO}>
        provably alive
      </text>

      {/* dead rule */}
      <text x={16} y={82} fontSize={8} fill={dim(0.5)} fontFamily={MONO}>
        rule b — nobody has checked it in a quarter
      </text>
      <path d="M16,100 L236,100" stroke={dim(0.28)} strokeWidth={1.4} strokeDasharray="1 0" />
      <path d="M160,100 L236,100" stroke={dim(0.14)} strokeWidth={1.4} />
      <rect x={244} y={90} width={82} height={20} rx={10} fill="rgba(255,93,162,0.05)" stroke={MAGENTA} strokeWidth={0.9} strokeDasharray="3 2" />
      <text x={285} y={103} fontSize={7} textAnchor="middle" fill={MAGENTA} fontFamily={MONO}>
        retire me — out loud
      </text>

      <text x={16} y={124} fontSize={7.5} fill={dim(0.35)} fontFamily={MONO}>
        rot has a number here. a library rots silently only when nothing is counting.
      </text>
    </Frame>
  );
}
