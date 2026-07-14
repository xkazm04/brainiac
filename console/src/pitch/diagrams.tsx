"use client";

/*
 * The pitch's figures.
 *
 * The page was arguing in paragraphs — which is the wrong medium for the claims
 * it is making. Every concept here is spatial ("the fact is on the other side of
 * a boundary", "the claim has no author", "the scan never sees the row"), so it
 * should be drawn, not described. Each figure carries the idea; the prose beside
 * it is now a caption, not the argument.
 *
 * Conventions, shared with the rest of the console (src/design/theme.ts):
 *   gold (gamma)  canonical / signed / constructive
 *   magenta       contradiction, staleness, the thing that is wrong
 *   alpha (cyan)  governance machinery
 *   dashed stroke not trusted yet — a claim nobody has signed
 *   mono labels   instrument microcopy
 *
 * Motion: entry only, once, on scroll-into-view (`whileInView`), per the motion
 * policy — no infinite loops on utility surfaces.
 */

import { motion } from "framer-motion";

import { band, MAGENTA } from "../design/theme";

const GOLD = band("gamma");
const ALPHA = band("alpha");
const MINT = band("beta");
const INK = "rgba(233,237,255,0.75)";
const FAINT = "rgba(233,237,255,0.30)";
const HAIR = "rgba(233,237,255,0.14)";

const MONO = "var(--font-mono)";

/** Standard figure frame: fixed aspect, scales to its column. */
function Figure({
  children,
  label,
  viewBox = "0 0 420 260",
}: {
  children: React.ReactNode;
  label: string;
  viewBox?: string;
}) {
  return (
    <motion.svg
      viewBox={viewBox}
      role="img"
      aria-label={label}
      className="w-full"
      initial="hidden"
      whileInView="visible"
      viewport={{ once: true, amount: 0.4 }}
      variants={{ visible: { transition: { staggerChildren: 0.09 } } }}
    >
      {children}
    </motion.svg>
  );
}

const fade = {
  hidden: { opacity: 0 },
  visible: { opacity: 1, transition: { duration: 0.5 } },
};

const drawLine = {
  hidden: { pathLength: 0, opacity: 0 },
  visible: { pathLength: 1, opacity: 1, transition: { duration: 0.8 } },
};

function Label({
  x,
  y,
  children,
  fill = FAINT,
  anchor = "start",
  size = 9,
}: {
  x: number;
  y: number;
  children: string;
  fill?: string;
  anchor?: "start" | "middle" | "end";
  size?: number;
}) {
  return (
    <motion.text
      variants={fade}
      x={x}
      y={y}
      fill={fill}
      fontSize={size}
      textAnchor={anchor}
      fontFamily={MONO}
      letterSpacing="0.08em"
    >
      {children}
    </motion.text>
  );
}

/** A "document / claim" card. Dashed when unattested. */
function Card({
  x,
  y,
  w = 120,
  h = 44,
  tone = FAINT,
  dashed = false,
  fill = "rgba(255,255,255,0.02)",
  lines = 2,
}: {
  x: number;
  y: number;
  w?: number;
  h?: number;
  tone?: string;
  dashed?: boolean;
  fill?: string;
  lines?: number;
}) {
  return (
    <motion.g variants={fade}>
      <rect
        x={x}
        y={y}
        width={w}
        height={h}
        rx={5}
        fill={fill}
        stroke={tone}
        strokeWidth={1.1}
        strokeDasharray={dashed ? "4 3" : undefined}
      />
      {Array.from({ length: lines }).map((_, i) => (
        <rect
          key={i}
          x={x + 10}
          y={y + 13 + i * 9}
          width={(w - 26) * (i === lines - 1 ? 0.55 : 0.85)}
          height={2.5}
          rx={1}
          fill={tone}
          opacity={0.45}
        />
      ))}
    </motion.g>
  );
}

/* ══════════════════════════════════════════════════════════════════════════
   PROBLEM FIGURES
   ══════════════════════════════════════════════════════════════════════════ */

/** 01 — the boundary: the fact exists, and it is on the other side of a wall. */
export function BoundaryFigure() {
  return (
    <Figure label="The payments repo holds the decision. The web repo's file cannot reach across the repository boundary to see it, so the web team ships a bug.">
      <Label x={64} y={22} anchor="middle" fill={FAINT}>
        payments repo
      </Label>
      <Label x={356} y={22} anchor="middle" fill={FAINT}>
        web repo
      </Label>

      {/* the two repos */}
      <motion.rect variants={fade} x={12} y={34} width={104} height={190} rx={8} fill="rgba(255,255,255,0.02)" stroke={HAIR} />
      <motion.rect variants={fade} x={304} y={34} width={104} height={190} rx={8} fill="rgba(255,255,255,0.02)" stroke={HAIR} />

      {/* the fact that matters */}
      <Card x={22} y={60} w={84} h={46} tone={GOLD} fill="hsla(46,90%,60%,0.06)" lines={2} />
      <Label x={64} y={122} anchor="middle" fill={GOLD} size={8}>
        PSP timeout = 30s
      </Label>

      {/* the file that needed it */}
      <Card x={314} y={60} w={84} h={46} tone={FAINT} lines={2} />
      <Label x={356} y={122} anchor="middle" fill={FAINT} size={8}>
        abort = 15s
      </Label>

      {/* THE WALL — the whole point */}
      <motion.line
        variants={drawLine}
        x1={210}
        y1={30}
        x2={210}
        y2={230}
        stroke={MAGENTA}
        strokeWidth={1.4}
        strokeDasharray="6 5"
        opacity={0.85}
      />
      <Label x={210} y={22} anchor="middle" fill={MAGENTA} size={8}>
        REPOSITORY BOUNDARY
      </Label>

      {/* the reach that fails */}
      <motion.path
        variants={drawLine}
        d="M112 83 L196 83"
        stroke={MAGENTA}
        strokeWidth={1.2}
        fill="none"
      />
      <motion.g variants={fade}>
        <line x1={202} y1={77} x2={214} y2={89} stroke={MAGENTA} strokeWidth={1.6} />
        <line x1={214} y1={77} x2={202} y2={89} stroke={MAGENTA} strokeWidth={1.6} />
      </motion.g>

      {/* the consequence */}
      <motion.rect
        variants={fade}
        x={296}
        y={170}
        width={120}
        height={30}
        rx={5}
        fill="rgba(255,93,162,0.08)"
        stroke={MAGENTA}
        strokeWidth={1}
      />
      <Label x={356} y={189} anchor="middle" fill={MAGENTA} size={8}>
        silent double charge
      </Label>
    </Figure>
  );
}

/** 02 — the retraction: the world moves, the document does not. */
export function RetractionFigure() {
  return (
    <Figure label="Reality is a timeline: a decision is made, then reversed. The document was written once and never moves, so it goes on confidently asserting the dead value.">
      <Label x={12} y={40} fill={FAINT}>
        REALITY
      </Label>
      <motion.line variants={drawLine} x1={12} y1={70} x2={408} y2={70} stroke={HAIR} strokeWidth={1} />

      {/* events on the timeline */}
      <motion.g variants={fade}>
        <circle cx={110} cy={70} r={4} fill={GOLD} />
        <circle cx={280} cy={70} r={4} fill={MAGENTA} />
      </motion.g>
      <Label x={110} y={58} anchor="middle" fill={GOLD} size={8}>
        cap → 30s
      </Label>
      <Label x={280} y={58} anchor="middle" fill={MAGENTA} size={8}>
        reverted → 2s
      </Label>
      <Label x={396} y={86} anchor="end" fill={FAINT} size={8}>
        now
      </Label>

      {/* the document, frozen at the moment it was written */}
      <Label x={12} y={126} fill={FAINT}>
        THE DOCUMENT
      </Label>
      <motion.line
        variants={drawLine}
        x1={110}
        y1={150}
        x2={408}
        y2={150}
        stroke={MAGENTA}
        strokeWidth={1.2}
        strokeDasharray="5 4"
      />
      <Card x={60} y={128} w={100} h={44} tone={MAGENTA} fill="rgba(255,93,162,0.05)" lines={2} />
      <Label x={110} y={188} anchor="middle" fill={MAGENTA} size={8}>
        “the cap is 30s”
      </Label>

      {/* the gap: everything to the right is confidently wrong */}
      <motion.rect
        variants={fade}
        x={280}
        y={100}
        width={128}
        height={100}
        rx={5}
        fill="rgba(255,93,162,0.07)"
        stroke={MAGENTA}
        strokeWidth={0.8}
        strokeDasharray="3 3"
      />
      <Label x={344} y={218} anchor="middle" fill={MAGENTA} size={8}>
        confidently wrong
      </Label>
      <Label x={344} y={232} anchor="middle" fill={FAINT} size={8}>
        and nothing knows it
      </Label>
    </Figure>
  );
}

/** 03 — attestation: a claim with no author, no source, no expiry. */
export function AttestationFigure() {
  return (
    <Figure label="A machine-written claim enters the record carrying institutional authority, but its author, source, model and validity fields are all empty.">
      {/* the machine */}
      <motion.rect variants={fade} x={14} y={104} width={78} height={44} rx={6} fill="rgba(255,255,255,0.02)" stroke={HAIR} />
      <Label x={53} y={130} anchor="middle" fill={FAINT} size={8}>
        LLM
      </Label>

      <motion.path variants={drawLine} d="M96 126 L146 126" stroke={FAINT} strokeWidth={1.1} fill="none" />
      <motion.path variants={fade} d="M142 122 L150 126 L142 130 Z" fill={FAINT} />

      {/* the claim it writes — asserted with full authority */}
      <motion.rect
        variants={fade}
        x={152}
        y={78}
        width={150}
        height={44}
        rx={5}
        fill="rgba(255,255,255,0.03)"
        stroke={INK}
        strokeWidth={1.2}
      />
      <Label x={162} y={96} fill={INK} size={9}>
        “retry policy is 2s”
      </Label>
      <Label x={162} y={112} fill={FAINT} size={8}>
        status: authoritative
      </Label>

      {/* the fields that do not exist */}
      {[
        { y: 140, k: "who asserted this?" },
        { y: 164, k: "from which session?" },
        { y: 188, k: "with which model?" },
        { y: 212, k: "is it still true?" },
      ].map((row) => (
        <motion.g key={row.k} variants={fade}>
          <line x1={152} y1={row.y} x2={302} y2={row.y} stroke={MAGENTA} strokeWidth={0.7} strokeDasharray="3 3" opacity={0.5} />
          <text x={152} y={row.y - 5} fill={FAINT} fontSize={8.5} fontFamily={MONO}>
            {row.k}
          </text>
          <text x={302} y={row.y - 5} fill={MAGENTA} fontSize={11} fontFamily={MONO} textAnchor="end">
            —
          </text>
        </motion.g>
      ))}

      <motion.rect
        variants={fade}
        x={318}
        y={78}
        width={90}
        height={44}
        rx={5}
        fill="rgba(255,93,162,0.06)"
        stroke={MAGENTA}
        strokeWidth={1}
      />
      <Label x={363} y={97} anchor="middle" fill={MAGENTA} size={8}>
        next dev
      </Label>
      <Label x={363} y={110} anchor="middle" fill={MAGENTA} size={8}>
        believes it
      </Label>
      <motion.path variants={drawLine} d="M304 100 L314 100" stroke={MAGENTA} strokeWidth={1.1} fill="none" />
    </Figure>
  );
}

/* ══════════════════════════════════════════════════════════════════════════
   MECHANISM FIGURES
   ══════════════════════════════════════════════════════════════════════════ */

/** RLS — the scan never sees the row. */
export function RlsFigure() {
  const rows = [
    { y: 60, ok: true, t: "payments · team" },
    { y: 92, ok: false, t: "data · team" },
    { y: 124, ok: true, t: "platform · org" },
    { y: 156, ok: false, t: "legal · private" },
    { y: 188, ok: true, t: "payments · org" },
  ];
  return (
    <Figure label="The vector scan runs inside the caller's row-level security transaction, so rows outside their permission are never candidates at all — they are invisible to the query planner, not filtered out afterwards.">
      <Label x={12} y={30} fill={ALPHA}>
        AGENT QUERY
      </Label>
      <motion.rect variants={fade} x={12} y={40} width={86} height={34} rx={5} fill="hsla(190,90%,60%,0.06)" stroke={ALPHA} strokeWidth={1} />
      <Label x={55} y={61} anchor="middle" fill={ALPHA} size={8}>
        “retry cap?”
      </Label>

      <motion.path variants={drawLine} d="M102 57 L134 57 L134 124 L150 124" stroke={ALPHA} strokeWidth={1.1} fill="none" />

      <Label x={160} y={30} fill={FAINT}>
        MEMORIES TABLE — UNDER RLS
      </Label>
      <motion.rect variants={fade} x={156} y={40} width={168} height={172} rx={6} fill="rgba(255,255,255,0.02)" stroke={HAIR} />

      {rows.map((r) => (
        <motion.g key={r.t} variants={fade}>
          <rect
            x={166}
            y={r.y - 10}
            width={148}
            height={22}
            rx={3}
            fill={r.ok ? "hsla(46,90%,60%,0.07)" : "rgba(233,237,255,0.02)"}
            stroke={r.ok ? GOLD : "rgba(233,237,255,0.10)"}
            strokeWidth={0.9}
            strokeDasharray={r.ok ? undefined : "3 3"}
          />
          <text
            x={176}
            y={r.y + 5}
            fill={r.ok ? INK : "rgba(233,237,255,0.22)"}
            fontSize={8.5}
            fontFamily={MONO}
          >
            {r.t}
          </text>
          {!r.ok && (
            <text x={306} y={r.y + 5} fill="rgba(233,237,255,0.25)" fontSize={9} fontFamily={MONO} textAnchor="end">
              ∅
            </text>
          )}
        </motion.g>
      ))}

      <Label x={240} y={230} anchor="middle" fill={FAINT} size={8}>
        invisible rows are not filtered — they are never candidates
      </Label>

      {/* what comes back */}
      <motion.path variants={drawLine} d="M330 124 L352 124" stroke={GOLD} strokeWidth={1.1} fill="none" />
      <motion.rect variants={fade} x={356} y={104} width={54} height={40} rx={5} fill="hsla(46,90%,60%,0.08)" stroke={GOLD} strokeWidth={1} />
      <Label x={383} y={128} anchor="middle" fill={GOLD} size={8}>
        3 hits
      </Label>
    </Figure>
  );
}

/** Provenance — every claim has a chain back to its origin. */
export function ProvenanceFigure() {
  const chain = [
    { x: 20, label: "claim", tone: GOLD },
    { x: 122, label: "session", tone: ALPHA },
    { x: 224, label: "model", tone: ALPHA },
    { x: 326, label: "human", tone: MINT },
  ];
  return (
    <Figure label="A claim links to the session it came from, the model that wrote it, and the human who signed it — a chain you can walk backwards." viewBox="0 0 420 200">
      {chain.map((c, i) => (
        <motion.g key={c.label} variants={fade}>
          <rect
            x={c.x}
            y={70}
            width={74}
            height={48}
            rx={6}
            fill={`${c.tone}12`}
            stroke={c.tone}
            strokeWidth={1.1}
          />
          <text x={c.x + 37} y={99} fill={c.tone} fontSize={9} fontFamily={MONO} textAnchor="middle">
            {c.label}
          </text>
          {i < chain.length - 1 && (
            <>
              <line x1={c.x + 74} y1={94} x2={c.x + 96} y2={94} stroke={FAINT} strokeWidth={1} />
              <path d={`M${c.x + 92} 90 L${c.x + 100} 94 L${c.x + 92} 98 Z`} fill={FAINT} />
            </>
          )}
        </motion.g>
      ))}

      <Label x={57} y={60} anchor="middle" fill={FAINT} size={8}>
        what
      </Label>
      <Label x={159} y={60} anchor="middle" fill={FAINT} size={8}>
        from where
      </Label>
      <Label x={261} y={60} anchor="middle" fill={FAINT} size={8}>
        by what
      </Label>
      <Label x={363} y={60} anchor="middle" fill={FAINT} size={8}>
        signed by
      </Label>

      <motion.path
        variants={drawLine}
        d="M57 128 L57 152 L363 152 L363 128"
        stroke={GOLD}
        strokeWidth={1}
        strokeDasharray="4 3"
        fill="none"
        opacity={0.7}
      />
      <Label x={210} y={170} anchor="middle" fill={GOLD} size={8}>
        replayable — ask any claim where it came from
      </Label>
    </Figure>
  );
}

/** Contradiction — the system stops and asks. */
export function ContradictionFigure() {
  return (
    <Figure label="Two claims collide. Instead of silently overwriting one, the system opens a contradiction and escalates to a human, who supersedes the loser without deleting it." viewBox="0 0 420 230">
      <Card x={16} y={30} w={130} h={44} tone={GOLD} fill="hsla(46,90%,60%,0.06)" />
      <Label x={81} y={90} anchor="middle" fill={GOLD} size={8}>
        “cap is 30s”
      </Label>

      <Card x={16} y={130} w={130} h={44} tone={MAGENTA} fill="rgba(255,93,162,0.06)" />
      <Label x={81} y={190} anchor="middle" fill={MAGENTA} size={8}>
        “cap is 2s”
      </Label>

      {/* they collide */}
      <motion.path variants={drawLine} d="M152 52 L196 96" stroke={FAINT} strokeWidth={1.1} fill="none" />
      <motion.path variants={drawLine} d="M152 152 L196 108" stroke={FAINT} strokeWidth={1.1} fill="none" />

      {/* the gate: stop */}
      <motion.g variants={fade}>
        <circle cx={214} cy={102} r={20} fill="rgba(255,93,162,0.10)" stroke={MAGENTA} strokeWidth={1.3} />
        <line x1={206} y1={102} x2={222} y2={102} stroke={MAGENTA} strokeWidth={1.6} />
      </motion.g>
      <Label x={214} y={140} anchor="middle" fill={MAGENTA} size={8}>
        CONTESTED
      </Label>
      <Label x={214} y={153} anchor="middle" fill={FAINT} size={8}>
        not served as fact
      </Label>

      {/* escalate to a human */}
      <motion.path variants={drawLine} d="M238 102 L276 102" stroke={ALPHA} strokeWidth={1.1} fill="none" />
      <motion.path variants={fade} d="M272 98 L280 102 L272 106 Z" fill={ALPHA} />

      <motion.rect variants={fade} x={284} y={80} width={122} height={44} rx={6} fill="hsla(190,90%,60%,0.07)" stroke={ALPHA} strokeWidth={1.1} />
      <Label x={345} y={100} anchor="middle" fill={ALPHA} size={8.5}>
        a human decides
      </Label>
      <Label x={345} y={113} anchor="middle" fill={FAINT} size={8}>
        supersede · coexist
      </Label>

      <Label x={345} y={150} anchor="middle" fill={GOLD} size={8}>
        the loser is superseded,
      </Label>
      <Label x={345} y={163} anchor="middle" fill={GOLD} size={8}>
        never deleted
      </Label>
    </Figure>
  );
}

/** Temporal — as-of is a query. */
export function TemporalFigure() {
  return (
    <Figure label="Each memory has a validity window and a pointer to whatever superseded it, so asking what the org believed on a past date is a query rather than an excavation." viewBox="0 0 420 200">
      <motion.line variants={drawLine} x1={20} y1={150} x2={400} y2={150} stroke={HAIR} strokeWidth={1} />
      {["MAR", "APR", "MAY", "JUN", "JUL"].map((m, i) => (
        <Label key={m} x={40 + i * 82} y={168} anchor="middle" fill={FAINT} size={8}>
          {m}
        </Label>
      ))}

      {/* v1: valid, then superseded */}
      <motion.g variants={fade}>
        <rect x={30} y={54} width={148} height={26} rx={4} fill="rgba(233,237,255,0.05)" stroke="rgba(233,237,255,0.25)" strokeWidth={1} strokeDasharray="4 3" />
        <text x={104} y={71} fill="rgba(233,237,255,0.5)" fontSize={8.5} fontFamily={MONO} textAnchor="middle">
          timeout = 15s
        </text>
      </motion.g>
      <Label x={30} y={46} fill={FAINT} size={8}>
        valid_from
      </Label>
      <Label x={178} y={46} anchor="end" fill={MAGENTA} size={8}>
        valid_to
      </Label>

      {/* v2: current */}
      <motion.g variants={fade}>
        <rect x={178} y={98} width={222} height={26} rx={4} fill="hsla(46,90%,60%,0.08)" stroke={GOLD} strokeWidth={1.1} />
        <text x={289} y={115} fill={GOLD} fontSize={8.5} fontFamily={MONO} textAnchor="middle">
          timeout = 30s
        </text>
      </motion.g>

      {/* the supersession link */}
      <motion.path variants={drawLine} d="M178 80 L178 98" stroke={MAGENTA} strokeWidth={1} strokeDasharray="3 2" fill="none" />
      <Label x={186} y={92} fill={MAGENTA} size={8}>
        superseded_by →
      </Label>

      {/* the as-of probe */}
      <motion.g variants={fade}>
        <line x1={104} y1={40} x2={104} y2={150} stroke={ALPHA} strokeWidth={1} strokeDasharray="2 3" />
        <circle cx={104} cy={150} r={3.5} fill={ALPHA} />
      </motion.g>
      <Label x={104} y={32} anchor="middle" fill={ALPHA} size={8}>
        as_of = 2026-04-02
      </Label>
      <Label x={104} y={188} anchor="middle" fill={ALPHA} size={8}>
        → returns 15s, not 30s
      </Label>
    </Figure>
  );
}

/** The gate — the status ladder, and where the human stands. */
export function GateFigure() {
  const steps = [
    { x: 20, label: "raw", tone: "rgba(233,237,255,0.35)", sub: "machine wrote it" },
    { x: 160, label: "candidate", tone: ALPHA, sub: "policy allowed it" },
    { x: 300, label: "canonical", tone: GOLD, sub: "a human signed it" },
  ];
  return (
    <Figure label="A memory climbs from raw to candidate to canonical. Policy can promote the first hop automatically, but reaching canonical always requires a named human." viewBox="0 0 420 200">
      {steps.map((s, i) => (
        <motion.g key={s.label} variants={fade}>
          <rect
            x={s.x}
            y={70}
            width={100}
            height={46}
            rx={6}
            fill={`${s.tone}14`}
            stroke={s.tone}
            strokeWidth={1.2}
            strokeDasharray={i === 0 ? "4 3" : undefined}
          />
          <text x={s.x + 50} y={91} fill={s.tone} fontSize={10} fontFamily={MONO} textAnchor="middle">
            {s.label}
          </text>
          <text x={s.x + 50} y={105} fill={FAINT} fontSize={8} fontFamily={MONO} textAnchor="middle">
            {s.sub}
          </text>
        </motion.g>
      ))}

      {/* hop 1: automatic */}
      <motion.path variants={drawLine} d="M124 93 L152 93" stroke={FAINT} strokeWidth={1.1} fill="none" />
      <Label x={138} y={62} anchor="middle" fill={FAINT} size={8}>
        policy
      </Label>

      {/* hop 2: the human. This is the product. */}
      <motion.path variants={drawLine} d="M264 93 L292 93" stroke={GOLD} strokeWidth={1.4} fill="none" />
      <motion.g variants={fade}>
        <circle cx={278} cy={44} r={13} fill="hsla(46,90%,60%,0.12)" stroke={GOLD} strokeWidth={1.2} />
        <path d="M272 44 L277 49 L285 39" stroke={GOLD} strokeWidth={1.6} fill="none" />
        <line x1={278} y1={57} x2={278} y2={84} stroke={GOLD} strokeWidth={1} strokeDasharray="2 2" />
      </motion.g>
      <Label x={278} y={22} anchor="middle" fill={GOLD} size={8}>
        A NAMED HUMAN
      </Label>

      <motion.rect
        variants={fade}
        x={20}
        y={148}
        width={380}
        height={30}
        rx={5}
        fill="hsla(46,90%,60%,0.04)"
        stroke="hsla(46,90%,68%,0.25)"
        strokeWidth={0.9}
      />
      <Label x={210} y={167} anchor="middle" fill={GOLD} size={8.5}>
        the promotion row records which rule fired, and who signed
      </Label>
    </Figure>
  );
}

/** BYOM — nothing leaves the box. */
export function ByomFigure() {
  return (
    <Figure label="The Rust binary, Postgres and your own model endpoint all sit inside your infrastructure. The only outbound calls are to endpoints you control; transcripts never leave." viewBox="0 0 420 200">
      <motion.rect
        variants={fade}
        x={14}
        y={20}
        width={300}
        height={162}
        rx={8}
        fill="rgba(255,255,255,0.015)"
        stroke={MINT}
        strokeWidth={1.2}
        strokeDasharray="6 4"
      />
      <Label x={24} y={38} fill={MINT} size={8}>
        YOUR INFRASTRUCTURE
      </Label>

      {[
        { x: 32, y: 62, label: "sessions", tone: FAINT },
        { x: 32, y: 118, label: "postgres", tone: FAINT },
        { x: 152, y: 62, label: "brainiac", tone: GOLD },
        { x: 152, y: 118, label: "your model", tone: MINT },
      ].map((n) => (
        <motion.g key={n.label} variants={fade}>
          <rect x={n.x} y={n.y} width={112} height={42} rx={5} fill={`${n.tone}10`} stroke={n.tone} strokeWidth={1.1} />
          <text x={n.x + 56} y={n.y + 26} fill={n.tone} fontSize={9} fontFamily={MONO} textAnchor="middle">
            {n.label}
          </text>
        </motion.g>
      ))}

      <motion.path variants={drawLine} d="M144 83 L152 83" stroke={FAINT} strokeWidth={1} fill="none" />
      <motion.path variants={drawLine} d="M144 139 L152 139" stroke={FAINT} strokeWidth={1} fill="none" />
      <motion.path variants={drawLine} d="M208 104 L208 118" stroke={MINT} strokeWidth={1} fill="none" />

      {/* the outside world — and the line nothing crosses */}
      <motion.path variants={drawLine} d="M318 101 L360 101" stroke={MAGENTA} strokeWidth={1.2} fill="none" />
      <motion.g variants={fade}>
        <line x1={330} y1={92} x2={342} y2={110} stroke={MAGENTA} strokeWidth={1.6} />
        <line x1={342} y1={92} x2={330} y2={110} stroke={MAGENTA} strokeWidth={1.6} />
      </motion.g>
      <Label x={366} y={92} fill={MAGENTA} size={8}>
        no vendor
      </Label>
      <Label x={366} y={106} fill={MAGENTA} size={8}>
        cloud
      </Label>
      <Label x={366} y={120} fill={FAINT} size={8}>
        no telemetry
      </Label>
    </Figure>
  );
}

/** Registry: mechanism key → figure. */
export const MECHANISM_FIGURES: Record<string, () => React.JSX.Element> = {
  rls: RlsFigure,
  provenance: ProvenanceFigure,
  contradiction: ContradictionFigure,
  temporal: TemporalFigure,
  gate: GateFigure,
  byom: ByomFigure,
};

export const PROBLEM_FIGURES: Record<string, () => React.JSX.Element> = {
  boundary: BoundaryFigure,
  retraction: RetractionFigure,
  attestation: AttestationFigure,
};
