"use client";

/*
 * The /kb concept drawings — one per section, all deterministic SVG.
 *
 * The rule these follow is the product's own rule (KB-PLAN D9): a diagram is
 * COMPILED from structure, never imagined. Every drawing here is hand-authored
 * geometry illustrating a mechanism — no generated artwork, no stock metaphors,
 * and nothing that could be mistaken for a screenshot of live data. Where a
 * drawing animates, it animates the mechanism (a change propagating, a dot
 * moving through a gate) and respects prefers-reduced-motion.
 *
 * Visual grammar, shared with the rest of the console:
 *   gold    — the constructive path (compose, publish)
 *   mint    — verified / in production
 *   cyan    — governance, the calm band
 *   magenta — the forbidden path, the rot
 */

import { useReducedMotion } from "framer-motion";

import { GOLD, MAGENTA, band } from "../design/theme";
import type { Stage } from "./kb-data";

const MINT = band("beta");
const ALPHA = band("alpha");
const dim = (a: number) => `rgba(233,237,255,${a})`;
const MONO = "var(--font-mono)";

/* Shared frame so every card illustration sits on the same quiet surface. */
function Frame({
  viewBox,
  label,
  children,
  minWidth = 0,
}: {
  viewBox: string;
  label: string;
  children: React.ReactNode;
  minWidth?: number;
}) {
  return (
    <div
      className="w-full overflow-x-auto rounded-lg border"
      style={{ borderColor: dim(0.08), background: "rgba(255,255,255,0.015)" }}
    >
      <svg
        viewBox={viewBox}
        role="img"
        aria-label={label}
        className="h-auto w-full"
        style={minWidth ? { minWidth } : undefined}
      >
        {children}
      </svg>
    </div>
  );
}

/* ── 1. Propagation: one memory changes, every citing page rebuilds ───────── */

export function PropagationSpark() {
  const reduce = !!useReducedMotion();
  const pages = [22, 56, 90];
  return (
    <Frame viewBox="0 0 340 132" label="One memory changes and every page that cited it rebuilds itself.">
      {/* the memory that changes */}
      <circle cx={56} cy={66} r={13} fill="hsla(46,90%,60%,0.12)" stroke={GOLD} strokeWidth={1.4} />
      <circle cx={56} cy={66} r={4} fill={GOLD} />
      {!reduce && (
        <circle cx={56} cy={66} r={13} fill="none" stroke={GOLD} strokeWidth={1}>
          <animate attributeName="r" values="13;30" dur="2.4s" repeatCount="indefinite" />
          <animate attributeName="opacity" values="0.6;0" dur="2.4s" repeatCount="indefinite" />
        </circle>
      )}
      <text x={56} y={104} fontSize={8.5} textAnchor="middle" fill={dim(0.55)} fontFamily={MONO}>
        one memory
      </text>
      <text x={56} y={115} fontSize={8.5} textAnchor="middle" fill={dim(0.55)} fontFamily={MONO}>
        is superseded
      </text>

      {/* dependency edges */}
      {pages.map((y, i) => (
        <path
          key={y}
          d={`M72,66 C 130,66 150,${y + 12} 208,${y + 12}`}
          fill="none"
          stroke={dim(0.22)}
          strokeWidth={1}
          strokeDasharray="3 5"
        >
          {!reduce && (
            <animate
              attributeName="stroke-dashoffset"
              from="32"
              to="0"
              dur="2.4s"
              begin={`${i * 0.25}s`}
              repeatCount="indefinite"
            />
          )}
        </path>
      ))}

      {/* the pages: flash stale, come back verified */}
      {pages.map((y, i) => (
        <g key={y}>
          <rect x={208} y={y} width={104} height={24} rx={5} fill="rgba(255,255,255,0.03)" stroke={dim(0.14)} />
          <circle cx={222} cy={y + 12} r={3.5} fill={MINT}>
            {!reduce && (
              <animate
                attributeName="fill"
                values={`${MINT};${GOLD};${GOLD};${MINT};${MINT}`}
                keyTimes="0;0.15;0.4;0.55;1"
                dur="4.8s"
                begin={`${0.4 + i * 0.35}s`}
                repeatCount="indefinite"
              />
            )}
          </circle>
          <text x={232} y={y + 15.5} fontSize={8.5} fill={dim(0.6)} fontFamily={MONO}>
            page that cited it
          </text>
        </g>
      ))}
      <text x={260} y={126} fontSize={8.5} textAnchor="middle" fill={dim(0.4)} fontFamily={MONO}>
        marked stale → rebuilt · untouched by hand
      </text>
    </Frame>
  );
}

/* ── 2. Lifecycle: a page that refuses to blend reality with intent ───────── */

export function LifecycleSplit() {
  return (
    <Frame viewBox="0 0 340 132" label="A composed page renders shipped reality and unshipped intent as visibly different things.">
      <rect x={12} y={10} width={316} height={112} rx={8} fill="rgba(255,255,255,0.02)" stroke={dim(0.12)} />

      {/* shipped claims */}
      <circle cx={30} cy={32} r={3.5} fill={MINT} />
      <rect x={42} y={28} width={168} height={7} rx={3.5} fill={dim(0.28)} />
      <rect x={236} y={26} width={80} height={12} rx={6} fill="hsla(158,90%,60%,0.10)" stroke={MINT} strokeWidth={0.8} />
      <text x={276} y={35} fontSize={7.5} textAnchor="middle" fill={MINT} fontFamily={MONO}>
        in production
      </text>

      <circle cx={30} cy={52} r={3.5} fill={MINT} />
      <rect x={42} y={48} width={132} height={7} rx={3.5} fill={dim(0.28)} />

      {/* the split — intent gets its own box, its own colour */}
      <rect x={22} y={66} width={296} height={30} rx={5} fill="hsla(46,90%,60%,0.05)" stroke={GOLD} strokeWidth={0.9} strokeDasharray="4 3" />
      <text x={34} y={79} fontSize={8} fill={GOLD} fontFamily={MONO}>
        ◐ on its way — decided, not yet deployed
      </text>
      <rect x={34} y={85} width={148} height={5} rx={2.5} fill="hsla(46,90%,68%,0.25)" />

      <text x={30} y={112} fontSize={8} fill={dim(0.3)} fontFamily={MONO}>
        ○ proposed — an idea, and the page says so
      </text>
    </Frame>
  );
}

/* ── 3. The artifact survives the summary ─────────────────────────────────── */

export function ArtifactSurvives() {
  return (
    <Frame viewBox="0 0 340 132" label="Beside the distilled sentence, the memory keeps the config itself — shown on the page verbatim.">
      {/* left: flattened */}
      <rect x={12} y={18} width={146} height={96} rx={7} fill="rgba(255,255,255,0.02)" stroke={dim(0.12)} />
      <text x={24} y={36} fontSize={8} letterSpacing="1.5" fill={dim(0.4)} fontFamily={MONO}>
        SUMMARY ONLY
      </text>
      <rect x={24} y={46} width={110} height={6} rx={3} fill={dim(0.3)} />
      <g opacity={0.35}>
        <rect x={24} y={62} width={122} height={40} rx={4} fill="rgba(255,255,255,0.02)" stroke={dim(0.15)} />
        <text x={32} y={78} fontSize={7.5} fill={dim(0.4)} fontFamily={MONO}>max_backoff: 30s</text>
        <text x={32} y={92} fontSize={7.5} fill={dim(0.4)} fontFamily={MONO}>jitter: full</text>
      </g>
      <line x1={24} y1={62} x2={146} y2={102} stroke={MAGENTA} strokeWidth={1.4} opacity={0.8} />
      <line x1={146} y1={62} x2={24} y2={102} stroke={MAGENTA} strokeWidth={1.4} opacity={0.8} />

      {/* arrow */}
      <path d="M166,66 L 178,66" stroke={dim(0.35)} strokeWidth={1.2} />
      <path d="M178,62 L 184,66 L 178,70 Z" fill={dim(0.35)} />

      {/* right: preserved */}
      <rect x={190} y={18} width={140} height={96} rx={7} fill="rgba(255,255,255,0.02)" stroke={dim(0.12)} />
      <text x={202} y={36} fontSize={8} letterSpacing="1.5" fill={MINT} fontFamily={MONO}>
        + THE ARTIFACT
      </text>
      <rect x={202} y={46} width={104} height={6} rx={3} fill={dim(0.3)} />
      <rect x={202} y={62} width={116} height={40} rx={4} fill="hsla(158,90%,60%,0.04)" stroke={MINT} strokeWidth={0.9} />
      <text x={210} y={78} fontSize={7.5} fill={dim(0.75)} fontFamily={MONO}>max_backoff: 30s</text>
      <text x={210} y={92} fontSize={7.5} fill={dim(0.75)} fontFamily={MONO}>jitter: full</text>
      <text x={260} y={126} fontSize={8} textAnchor="middle" fill={dim(0.4)} fontFamily={MONO}>
        copied character-for-character, never retyped
      </text>
    </Frame>
  );
}

/* ── 4. The health gate: a meter with a floor, and what happens below it ──── */

export function GateMeter() {
  const reduce = !!useReducedMotion();
  const trackX = 22;
  const trackW = 296;
  const floorX = trackX + trackW * 0.7; // publish floor at 70
  const needleX = trackX + trackW * 0.58; // currency 58 — below the floor
  return (
    <Frame viewBox="0 0 340 132" label="When the corpus health score drops below the publish floor, external publishing pauses and pages hold their last approved version.">
      <text x={trackX} y={26} fontSize={8} letterSpacing="1.5" fill={dim(0.4)} fontFamily={MONO}>
        CORPUS CURRENCY — HOW MUCH OF WHAT WE KNOW IS STILL TRUE
      </text>

      {/* track */}
      <rect x={trackX} y={40} width={trackW} height={9} rx={4.5} fill="rgba(255,255,255,0.05)" />
      <rect x={trackX} y={40} width={trackW * 0.58} height={9} rx={4.5} fill="hsla(158,90%,60%,0.22)" />

      {/* the floor */}
      <line x1={floorX} y1={32} x2={floorX} y2={58} stroke={GOLD} strokeWidth={1.2} strokeDasharray="3 3" />
      <text x={floorX} y={70} fontSize={8} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        publish floor · 70
      </text>

      {/* the needle, below the floor */}
      <circle cx={needleX} cy={44.5} r={5.5} fill="#08070c" stroke={MAGENTA} strokeWidth={1.6}>
        {!reduce && <animate attributeName="stroke-opacity" values="1;0.45;1" dur="1.8s" repeatCount="indefinite" />}
      </circle>
      <text x={needleX} y={70} fontSize={8} textAnchor="middle" fill={MAGENTA} fontFamily={MONO}>
        today · 58
      </text>

      {/* the consequence */}
      <rect x={trackX} y={84} width={202} height={26} rx={13} fill="transparent" stroke={GOLD} strokeWidth={1} />
      <text x={trackX + 101} y={100} fontSize={8.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        ◍ publishing paused — nothing goes out
      </text>
      <text x={trackX} y={124} fontSize={8} fill={dim(0.4)} fontFamily={MONO}>
        pages hold the last human-approved version · silence beats confident staleness
      </text>
    </Frame>
  );
}

/* ── 5. The round trip: an edit becomes knowledge, never prose ────────────── */

export function RoundTripLoop() {
  const reduce = !!useReducedMotion();
  // one closed loop, drawn as a rounded rectangle circuit
  const loop = "M 262,30 L 92,30 Q 70,30 70,52 L 70,78 Q 70,100 92,100 L 248,100 Q 270,100 270,78 L 270,52 Q 270,30 262,30";
  return (
    <Frame viewBox="0 0 340 132" label="A page edit is captured as proposed knowledge, faces the human review gate, becomes memory, and the page rebuilds.">
      <path d={loop} fill="none" stroke={dim(0.14)} strokeWidth={1.2} />
      {!reduce && (
        <circle r={3} fill={ALPHA}>
          <animateMotion dur="6s" repeatCount="indefinite" path={loop} />
        </circle>
      )}

      {/* stations on the loop */}
      <g>
        <rect x={228} y={20} width={92} height={22} rx={11} fill="#08070c" stroke={ALPHA} strokeWidth={1} />
        <text x={274} y={34} fontSize={8} textAnchor="middle" fill={ALPHA} fontFamily={MONO}>
          you edit a page
        </text>
      </g>
      <g>
        <rect x={22} y={20} width={104} height={22} rx={11} fill="#08070c" stroke={dim(0.25)} strokeWidth={1} />
        <text x={74} y={34} fontSize={8} textAnchor="middle" fill={dim(0.6)} fontFamily={MONO}>
          captured — not saved
        </text>
      </g>
      <g>
        <rect x={22} y={90} width={104} height={22} rx={11} fill="#08070c" stroke={GOLD} strokeWidth={1.2} />
        <text x={74} y={104} fontSize={8} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
          a human signs it
        </text>
      </g>
      <g>
        <rect x={196} y={90} width={124} height={22} rx={11} fill="#08070c" stroke={MINT} strokeWidth={1} />
        <text x={258} y={104} fontSize={8} textAnchor="middle" fill={MINT} fontFamily={MONO}>
          memory → page rebuilds
        </text>
      </g>

      <text x={170} y={68} fontSize={8.5} textAnchor="middle" fill={dim(0.38)} fontFamily={MONO}>
        the same gate every agent proposal faces
      </text>
      <text x={170} y={126} fontSize={8} textAnchor="middle" fill={dim(0.35)} fontFamily={MONO}>
        your reason travels with the edit — the part a diff can never recover
      </text>
    </Frame>
  );
}

/* ── The compose rail: the whole rebuild, one drawing ─────────────────────────
 *
 * Six stations on one line, driven by the same COMPOSE_STAGES data the honesty
 * tests pin, so the figure cannot drift from the stamps. The two external
 * stations sit on the far side of a physical gap in the rail — the breaker —
 * inside a tinted zone labelled with their true state. Station details ride as
 * native tooltips (<title>), so depth exists without weight.
 */

export function PipelineFigure({ stages }: { stages: Stage[] }) {
  const reduce = !!useReducedMotion();
  const Y = 108;
  const XS = [120, 216, 312, 408, 560, 668]; // six stations; the gap is the breaker
  const running = stages.slice(0, 4);

  return (
    <Frame
      viewBox="0 0 760 220"
      label="A memory change flows through bind, cap, compose, and diff — then stops at the breaker; the gate and publish stations exist but are switched off."
      minWidth={640}
    >
      {/* the change that starts everything */}
      <circle cx={44} cy={Y} r={10} fill="hsla(46,90%,60%,0.12)" stroke={GOLD} strokeWidth={1.3} />
      <circle cx={44} cy={Y} r={3} fill={GOLD} />
      {!reduce && (
        <circle cx={44} cy={Y} r={10} fill="none" stroke={GOLD} strokeWidth={1}>
          <animate attributeName="r" values="10;22" dur="2.6s" repeatCount="indefinite" />
          <animate attributeName="opacity" values="0.6;0" dur="2.6s" repeatCount="indefinite" />
        </circle>
      )}
      <text x={44} y={Y + 34} fontSize={8.5} textAnchor="middle" fill={dim(0.5)} fontFamily={MONO}>
        a memory
      </text>
      <text x={44} y={Y + 45} fontSize={8.5} textAnchor="middle" fill={dim(0.5)} fontFamily={MONO}>
        changes
      </text>

      {/* the running rail */}
      <line x1={56} y1={Y} x2={XS[3]} y2={Y} stroke={dim(0.18)} strokeWidth={1.2} />
      {!reduce && (
        <circle r={3} fill={GOLD}>
          <animateMotion dur="4s" repeatCount="indefinite" path={`M56,${Y} L ${XS[3] - 20},${Y}`} />
        </circle>
      )}

      {running.map((s, i) => (
        <g key={s.n}>
          <title>{s.body}</title>
          <circle cx={XS[i]} cy={Y} r={17} fill="#08070c" stroke={MINT} strokeWidth={1.3} />
          <text x={XS[i]} y={Y + 3.5} fontSize={9} textAnchor="middle" fill={MINT} fontFamily={MONO}>
            {s.n}
          </text>
          <text x={XS[i]} y={Y - 28} fontSize={9.5} textAnchor="middle" fill="#ffffff" fontFamily={MONO}>
            {s.name.toLowerCase()}
          </text>
        </g>
      ))}

      {/* the fork at diff: additive self-publishes, a dropped claim gets a human */}
      <path d={`M${XS[3]},${Y - 17} C ${XS[3]},70 ${XS[3] + 36},64 ${XS[3] + 58},64`} fill="none" stroke={MINT} strokeWidth={1} />
      <text x={XS[3] + 64} y={60} fontSize={8} fill={MINT} fontFamily={MONO}>
        additive → publishes itself
      </text>
      <text x={XS[3] + 64} y={71} fontSize={8} fill={dim(0.35)} fontFamily={MONO}>
        every claim cited, none dropped
      </text>
      <path d={`M${XS[3]},${Y + 17} C ${XS[3]},156 ${XS[3] + 36},162 ${XS[3] + 58},162`} fill="none" stroke={GOLD} strokeWidth={1} strokeDasharray="3 3" />
      <text x={XS[3] + 64} y={158} fontSize={8} fill={GOLD} fontFamily={MONO}>
        drops a claim → a human decides
      </text>
      <text x={XS[3] + 64} y={169} fontSize={8} fill={dim(0.35)} fontFamily={MONO}>
        the same queue as every promotion
      </text>

      {/* the breaker: a literal gap in the rail */}
      <line x1={488} y1={Y - 26} x2={488} y2={Y + 26} stroke={GOLD} strokeWidth={1.3} strokeDasharray="4 3" />
      <text x={488} y={Y - 34} fontSize={8.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        the breaker
      </text>

      {/* the dark side: built, and off */}
      <rect x={508} y={44} width={236} height={132} rx={10} fill="hsla(46,90%,60%,0.03)" stroke="hsla(46,90%,68%,0.25)" strokeDasharray="5 4" />
      <text x={626} y={34} fontSize={8.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        ◍ built · not enabled
      </text>
      <line x1={508} y1={Y} x2={XS[4] - 17} y2={Y} stroke={dim(0.14)} strokeWidth={1.2} strokeDasharray="3 4" />
      <line x1={XS[4] + 17} y1={Y} x2={XS[5] - 17} y2={Y} stroke={dim(0.14)} strokeWidth={1.2} strokeDasharray="3 4" />
      <line x1={XS[5] + 17} y1={Y} x2={744} y2={Y} stroke={dim(0.14)} strokeWidth={1.2} strokeDasharray="3 4" />

      {stages.slice(4).map((s, i) => (
        <g key={s.n} opacity={0.9}>
          <title>{s.body}</title>
          <circle cx={XS[4 + i]} cy={Y} r={17} fill="#08070c" stroke={GOLD} strokeWidth={1} />
          <circle cx={XS[4 + i]} cy={Y} r={13} fill="none" stroke={GOLD} strokeWidth={0.8} />
          <text x={XS[4 + i]} y={Y + 3.5} fontSize={9} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
            {s.n}
          </text>
          <text x={XS[4 + i]} y={Y - 28} fontSize={9.5} textAnchor="middle" fill="#ffffff" fontFamily={MONO}>
            {s.name.toLowerCase()}
          </text>
        </g>
      ))}
      <text x={626} y={Y + 40} fontSize={8} textAnchor="middle" fill={dim(0.4)} fontFamily={MONO}>
        → your wiki, when the org flips the switch
      </text>
    </Frame>
  );
}

/* ── The refusals: a panel of switches that do not exist ──────────────────────
 *
 * Four blanked-off switch positions, riveted shut. Not "off" — ABSENT: there is
 * no lever to flip, because each of these would put the rot back. The captions
 * beside the figure carry the one-line why.
 */

const REFUSED = [
  "BIDIRECTIONAL SYNC",
  "AGENT PAGE-WRITE",
  "INVENTED DIAGRAMS",
  "PRIVATE DATA OUTBOUND",
];

export function NoSwitches() {
  return (
    <Frame viewBox="0 0 320 232" label="Four switches this product refuses to have: bidirectional sync, agents writing pages, invented diagrams, private data outbound.">
      <rect x={12} y={10} width={296} height={212} rx={10} fill="rgba(255,255,255,0.015)" stroke={dim(0.14)} />
      {/* corner rivets */}
      {[
        [26, 24],
        [294, 24],
        [26, 208],
        [294, 208],
      ].map(([x, y]) => (
        <circle key={`${x}-${y}`} cx={x} cy={y} r={2.2} fill={dim(0.18)} />
      ))}

      {REFUSED.map((label, i) => {
        const y = 40 + i * 46;
        return (
          <g key={label}>
            {/* the blanked switch position: a housing with a plate where the
                lever would be, and a weld across it */}
            <rect x={34} y={y} width={44} height={26} rx={6} fill="rgba(0,0,0,0.3)" stroke={dim(0.2)} />
            <rect x={40} y={y + 5} width={32} height={16} rx={4} fill="rgba(255,255,255,0.03)" stroke={dim(0.1)} />
            <line x1={40} y1={y + 5} x2={72} y2={y + 21} stroke={MAGENTA} strokeWidth={1.6} />
            <line x1={72} y1={y + 5} x2={40} y2={y + 21} stroke={MAGENTA} strokeWidth={1.6} />
            <text x={92} y={y + 12} fontSize={9} letterSpacing="1.2" fill={dim(0.75)} fontFamily={MONO}>
              {label}
            </text>
            <text x={92} y={y + 24} fontSize={7.5} fill={MAGENTA} fontFamily={MONO}>
              no lever fitted — not a setting, not a tier
            </text>
          </g>
        );
      })}
    </Frame>
  );
}

/* ── 6. Publishing: one-way into the wiki you already read ────────────────── */

export function OneWayPublish() {
  const reduce = !!useReducedMotion();
  return (
    <Frame
      viewBox="0 0 760 216"
      label="Brainiac pushes compiled pages one-way into Confluence with a generated-content banner; edits made there never become truth."
      minWidth={640}
    >
      <defs>
        <marker id="owp-fwd" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
          <path d="M0,0 L8,4 L0,8 Z" fill={GOLD} />
        </marker>
      </defs>

      {/* left: the compiled page */}
      <text x={16} y={24} fontSize={9} letterSpacing="2" fill={GOLD} fontFamily={MONO}>
        BRAINIAC · THE COMPILED PAGE
      </text>
      <rect x={10} y={34} width={250} height={150} rx={10} fill="hsla(46,90%,60%,0.03)" stroke="hsla(46,90%,68%,0.22)" />
      <text x={26} y={58} fontSize={10} fill="#fff" fontFamily={MONO}>payment retries</text>
      <line x1={26} y1={66} x2={244} y2={66} stroke={dim(0.1)} />
      <rect x={26} y={78} width={150} height={6} rx={3} fill={dim(0.28)} />
      <rect x={182} y={76} width={38} height={10} rx={5} fill="hsla(46,90%,60%,0.12)" stroke={GOLD} strokeWidth={0.7} />
      <text x={201} y={84} fontSize={6.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>source</text>
      <rect x={26} y={96} width={128} height={6} rx={3} fill={dim(0.28)} />
      <rect x={160} y={94} width={38} height={10} rx={5} fill="hsla(46,90%,60%,0.12)" stroke={GOLD} strokeWidth={0.7} />
      <text x={179} y={102} fontSize={6.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>source</text>
      <circle cx={32} cy={124} r={3} fill={MINT} />
      <text x={42} y={127} fontSize={8} fill={MINT} fontFamily={MONO}>recomposed 4 minutes ago</text>
      <text x={26} y={150} fontSize={8} fill={dim(0.4)} fontFamily={MONO}>every claim signed by a named human</text>
      <text x={26} y={172} fontSize={8} fill={dim(0.3)} fontFamily={MONO}>org-visible knowledge only</text>

      {/* forward path, through the health gate */}
      <path d="M264,84 L 356,84" fill="none" stroke={GOLD} strokeWidth={1.5} />
      <path d="M404,84 L 494,84" fill="none" stroke={GOLD} strokeWidth={1.5} markerEnd="url(#owp-fwd)" />
      {!reduce && (
        <circle r={3} fill={GOLD}>
          <animateMotion dur="3.4s" repeatCount="indefinite" path="M264,84 L 494,84" />
        </circle>
      )}
      <g>
        <rect x={356} y={68} width={48} height={32} rx={7} fill="#08070c" stroke={GOLD} strokeWidth={1.1} />
        <text x={380} y={82} fontSize={7} textAnchor="middle" fill={GOLD} fontFamily={MONO}>health</text>
        <text x={380} y={92} fontSize={7} textAnchor="middle" fill={GOLD} fontFamily={MONO}>gate</text>
      </g>
      <text x={380} y={54} fontSize={8} textAnchor="middle" fill={dim(0.5)} fontFamily={MONO}>
        one-way push
      </text>
      <text x={380} y={114} fontSize={7.5} textAnchor="middle" fill={dim(0.38)} fontFamily={MONO}>
        pauses if the corpus degrades
      </text>

      {/* the way back, refused */}
      <path d="M494,150 L 264,150" fill="none" stroke={MAGENTA} strokeWidth={1.1} strokeDasharray="3 4" opacity={0.55} />
      <line x1={366} y1={140} x2={392} y2={160} stroke={MAGENTA} strokeWidth={2} />
      <line x1={392} y1={140} x2={366} y2={160} stroke={MAGENTA} strokeWidth={2} />
      <text x={380} y={176} fontSize={8} textAnchor="middle" fill={MAGENTA} fontFamily={MONO}>
        edits made here never become truth
      </text>
      <text x={380} y={188} fontSize={7.5} textAnchor="middle" fill={dim(0.35)} fontFamily={MONO}>
        overwritten on the next push — the banner warns you first
      </text>

      {/* right: their wiki */}
      <text x={506} y={24} fontSize={9} letterSpacing="2" fill={ALPHA} fontFamily={MONO}>
        YOUR CONFLUENCE · UNTOUCHED WORKFLOW
      </text>
      <rect x={500} y={34} width={250} height={150} rx={10} fill="rgba(255,255,255,0.02)" stroke={dim(0.12)} />
      <circle cx={516} cy={48} r={3} fill={dim(0.2)} />
      <circle cx={526} cy={48} r={3} fill={dim(0.2)} />
      <circle cx={536} cy={48} r={3} fill={dim(0.2)} />
      <rect x={512} y={58} width={226} height={20} rx={4} fill="hsla(46,90%,60%,0.08)" stroke="hsla(46,90%,68%,0.25)" strokeWidth={0.8} />
      <text x={625} y={71} fontSize={7.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>
        ⚠ generated — do not edit here
      </text>
      <text x={516} y={96} fontSize={9} fill={dim(0.75)} fontFamily={MONO}>payment retries</text>
      <rect x={516} y={106} width={168} height={6} rx={3} fill={dim(0.25)} />
      <rect x={690} y={104} width={38} height={10} rx={5} fill="hsla(46,90%,60%,0.12)" stroke={GOLD} strokeWidth={0.7} />
      <text x={709} y={112} fontSize={6.5} textAnchor="middle" fill={GOLD} fontFamily={MONO}>source</text>
      <rect x={516} y={122} width={140} height={6} rx={3} fill={dim(0.25)} />
      <text x={516} y={150} fontSize={8} fill={dim(0.4)} fontFamily={MONO}>
        the page your team already reads —
      </text>
      <text x={516} y={162} fontSize={8} fill={dim(0.4)} fontFamily={MONO}>
        now it maintains itself
      </text>
    </Frame>
  );
}
