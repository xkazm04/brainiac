"use client";

/*
 * Brainiac console home — the fused identity. Structure and interactivity
 * from the "Interference" prototype (live wave field, draggable team
 * emitters, cursor as fourth source, phase-lock, sine-spine scroll story);
 * theme and typography from "Spectrum" (Space Grotesk / JetBrains Mono,
 * band hues with glow — gold gamma as the canonical/constructive color,
 * magenta as the contradiction color). See src/design/theme.ts.
 */

import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { motion, useReducedMotion, useScroll, useSpring } from "framer-motion";

import { CANONICAL_DEMO } from "../design/demo-data";
import { PRODUCT_ROUTES } from "../design/routes";
import {
  band,
  BG,
  FONT_DISPLAY,
  FONT_MONO,
  GOLD,
  GOLD_GLOW,
  LABEL,
  MAGENTA,
} from "../design/theme";
import StationModule from "./StationModules";

export interface LiveStats {
  pendingPromotions: number;
  openContradictions: number;
  canonicalCount: number;
  embeddingModel: string;
  /** Real team lobes (name + memory volume), busiest first. */
  teams: { name: string; memories: number }[];
  /** The most-bound canonical entity — what best sells "constructive". */
  topCanonical: { name: string; teams: number } | null;
  /** Org-wide memory total (sum over team lobes). */
  totalMemories: number;
  /** Knowledge Health composite — null when that endpoint is unreachable. */
  health: { score: number; grade: string; crossTeamContradictions: number } | null;
  /** The standardization board: count + the highest-impact divergence. */
  divergence: { count: number; top: { practice: string; impact: string } | null } | null;
  /** Pages composed from the canonical graph — null when the docs API is off. */
  docsPages: number | null;
}

const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;

interface Emitter {
  id: string;
  label: string;
  x: number;
  y: number;
  phase: number;
  targetPhase: number;
}

const SPACING = 17;
const WAVELEN = 96;
const OMEGA = 2.4;

/**
 * "operator" — inside the gate: the nav is the product nav, and the stats strip
 *   tells the truth about the real org.
 * "public"  — the landing page: the same wave field, but every link points at a
 *   surface an anonymous visitor can actually reach (the deck, the showcase, the
 *   wiki, the sign-in). Pointing the operator nav at a visitor gives them a wall
 *   of links that all bounce to the passcode screen.
 */
export type HomeVariant = "operator" | "public";

const PUBLIC_NAV = [
  { path: "/pitch", label: "the pitch" },
  { path: "/demo", label: "live demo" },
  { path: "/kb", label: "knowledge base" },
  { path: "/console", label: "console →" },
];

export default function Home({
  live,
  variant = "operator",
}: {
  live: LiveStats | null;
  variant?: HomeVariant;
}) {
  const isPublic = variant === "public";
  const reduce = !!useReducedMotion();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const emittersRef = useRef<Emitter[]>([
    { id: "payments", label: "payments", x: 0.26, y: 0.4, phase: Math.PI, targetPhase: Math.PI },
    { id: "platform", label: "platform", x: 0.62, y: 0.3, phase: 0, targetPhase: 0 },
    { id: "data", label: "data", x: 0.5, y: 0.68, phase: 0, targetPhase: 0 },
  ]);
  const cursorRef = useRef<{ x: number; y: number; on: boolean }>({ x: 0, y: 0, on: false });
  const dragRef = useRef<string | null>(null);
  const [locked, setLocked] = useState(false);
  const [, forceTick] = useState(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let raf = 0;
    let disposed = false;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();

    const k = (Math.PI * 2) / WAVELEN;

    const draw = (now: number) => {
      if (disposed) return;
      const rect = canvas.getBoundingClientRect();
      const W = rect.width;
      const H = rect.height;
      const t = now / 1000;
      ctx.clearRect(0, 0, W, H);

      const emitters = emittersRef.current;
      for (const e of emitters) {
        e.phase += (e.targetPhase - e.phase) * 0.045;
      }
      const cursor = cursorRef.current;
      const sources = [
        ...emitters.map((e) => ({ x: e.x * W, y: e.y * H, phase: e.phase, amp: 1 })),
        ...(cursor.on ? [{ x: cursor.x, y: cursor.y, phase: 0, amp: 0.65 }] : []),
      ];
      const norm = sources.reduce((s, src) => s + src.amp, 0) || 1;

      for (let gy = SPACING / 2; gy < H; gy += SPACING) {
        for (let gx = SPACING / 2; gx < W; gx += SPACING) {
          let sum = 0;
          for (const s of sources) {
            const dx = gx - s.x;
            const dy = gy - s.y;
            const d = Math.sqrt(dx * dx + dy * dy);
            const falloff = 1 / (1 + d / 420);
            sum += Math.sin(k * d - t * OMEGA + s.phase) * s.amp * falloff;
          }
          const b = sum / norm;
          if (b > 0.18) {
            // constructive — gamma gold
            const a = Math.min(1, (b - 0.18) * 1.7);
            ctx.fillStyle = `hsla(46, 90%, ${62 + a * 16}%, ${0.14 + a * 0.8})`;
            const r = 1 + a * 1.9;
            ctx.fillRect(gx - r / 2, gy - r / 2, r, r);
          } else if (b < -0.42) {
            // destructive — the contradiction seam
            const a = Math.min(1, (-b - 0.42) * 2.2);
            ctx.fillStyle = `rgba(255, 93, 162, ${0.06 + a * 0.34})`;
            ctx.fillRect(gx - 0.75, gy - 0.75, 1.5, 1.5);
          } else {
            ctx.fillStyle = "rgba(120, 130, 190, 0.10)";
            ctx.fillRect(gx - 0.5, gy - 0.5, 1, 1);
          }
        }
      }

      for (const e of emitters) {
        const ex = e.x * W;
        const ey = e.y * H;
        const anti = Math.abs(e.phase) > 0.6;
        ctx.strokeStyle = anti ? MAGENTA : GOLD;
        ctx.lineWidth = 1.4;
        ctx.beginPath();
        ctx.arc(ex, ey, 7, 0, Math.PI * 2);
        ctx.stroke();
        if (!reduce) {
          const pulse = 10 + ((now / 18) % WAVELEN) / 4;
          ctx.globalAlpha = Math.max(0, 0.5 - pulse / 60);
          ctx.beginPath();
          ctx.arc(ex, ey, pulse, 0, Math.PI * 2);
          ctx.stroke();
          ctx.globalAlpha = 1;
        }
      }

      if (!reduce) raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);

    // Observed AFTER `draw` exists, because a resize has to be able to repaint.
    //
    // Assigning canvas.width/height RESETS and clears the surface. With motion
    // allowed the running RAF chain repaints on the next frame, so a bare
    // `resize` was fine. Under prefers-reduced-motion `draw` runs exactly once
    // (the tail's `!reduce` guard means it never reschedules), so ANY resize —
    // window, devtools, orientation, or just the ResizeObserver's own initial
    // fire after fonts settle — cleared the hero and nothing ever redrew it. The
    // primary visual of the whole pitch silently vanished, leaving the emitter
    // labels floating over an empty box.
    //
    // Repaint once here in that case; with motion allowed the chain still owns
    // the repaint, so we must NOT call draw and spawn a second chain.
    const ro = new ResizeObserver(() => {
      resize();
      if (reduce && !disposed) draw(performance.now());
    });
    ro.observe(canvas);

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [reduce]);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    if (dragRef.current) {
      const em = emittersRef.current.find((m) => m.id === dragRef.current);
      if (em) {
        em.x = Math.min(0.96, Math.max(0.04, px / rect.width));
        em.y = Math.min(0.92, Math.max(0.08, py / rect.height));
        forceTick((n) => n + 1);
      }
      return;
    }
    cursorRef.current = { x: px, y: py, on: true };
  }, []);

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    for (const em of emittersRef.current) {
      const dx = px - em.x * rect.width;
      const dy = py - em.y * rect.height;
      if (Math.sqrt(dx * dx + dy * dy) < 26) {
        dragRef.current = em.id;
        (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
        return;
      }
    }
  }, []);

  const phaseLock = useCallback(() => {
    setLocked((prev) => {
      const next = !prev;
      emittersRef.current.forEach((em) => {
        em.targetPhase = next ? 0 : em.id === "payments" ? Math.PI : 0;
      });
      return next;
    });
  }, []);

  const trackRef = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({ target: trackRef, offset: ["start center", "end end"] });
  const pathLength = useSpring(scrollYProgress, { stiffness: 120, damping: 30 });

  const emitters = emittersRef.current;

  // The scroll story tells the live truth when the server is up: real team
  // names + counts, the real open-contradiction tally, the actual canonical
  // graph. The demo fiction (labelled as such in the stats strip + footer)
  // survives only when `live` is null. The wave art itself is untouched.
  const teamNames =
    live && live.teams.length > 0
      ? live.teams.slice(0, 3).map((t) => t.name).join(", ")
      : "payments, platform, data";

  // Each station's `artifact` is the caption under its module figure. In live
  // mode it is the only place on this page that speaks about the reader's real
  // org — the figure above it is always the example. So in demo mode the caption
  // must not merely restate the figure: it says the thing the figure cannot draw.
  const STATIONS = [
    {
      n: "01",
      title: "Every team is a source.",
      body: live
        ? `${teamNames} — each session broadcasts what it learned. Raw memories, provenance attached, nothing lost in the noise floor.`
        : "Payments, platform, data — each session broadcasts what it learned. Raw memories, provenance attached, nothing lost in the noise floor.",
      artifact:
        live && live.teams.length > 0
          ? live.teams
              .slice(0, 3)
              .map((t) => `${t.name} · ${t.memories} mem`)
              .join("   ")
          : live
            ? `${live.totalMemories} memories captured across the org`
            : "each proposal carries its provenance — the session it came from, the model that extracted it, the policy rule that routed it here.",
      tone: GOLD,
      wave: "in" as const,
      module: "gate" as const,
      href: isPublic ? "/demo?m=reviews" : "/console?m=reviews",
      cta: isPublic ? "see the review queue" : "open the review queue",
    },
    {
      n: "02",
      title: "Disagreement is visible.",
      body: "Two claims out of phase don't average out — they cancel. Brainiac finds the dark seams and files them as contradictions, with a suggested resolution.",
      artifact: live
        ? live.openContradictions > 0
          ? `${plural(live.openContradictions, "open contradiction")} in the review queue · each with a suggested resolution`
          : "0 open contradictions — every source currently in phase"
        : "the scan finds the seam; a named human closes it. the loser is superseded, never deleted.",
      tone: MAGENTA,
      wave: "anti" as const,
      module: "contradiction" as const,
      href: isPublic ? "/demo?m=disputes" : "/console?m=reviews",
      cta: isPublic ? "see a contradiction" : "resolve contradictions",
    },
    {
      n: "03",
      title: "Phase-lock makes it canonical.",
      body: live
        ? `A maintainer signs the resolution and the sources align: ${plural(live.canonicalCount, "canonical node")} the whole org can see, each bound from the teams that named it.`
        : `A maintainer signs the resolution and the sources align: ${CANONICAL_DEMO.aliases.map((a) => `“${a.name}”`).join(", ")} — one bright node, ${CANONICAL_DEMO.name}, visible to everyone cleared to see it.`,
      artifact: live
        ? live.topCanonical
          ? `${live.topCanonical.name} ⇐ bound across ${plural(live.topCanonical.teams, "team")} · constructive`
          : `${plural(live.canonicalCount, "canonical node")} · constructive`
        : "one node, three dialects — a query in any of them finds it, and permission decides who sees it.",
      tone: "#f6ecd0",
      wave: "locked" as const,
      module: "cortex" as const,
      href: isPublic ? "/demo?m=graph" : "/console?m=graph",
      cta: "explore the graph",
    },
    // ── the second movement: what the field computes that no session can ──
    {
      n: "04",
      title: "Same practice, out of tune.",
      body: "This is not a contradiction. It is a detune. Two teams solve the same problem at slightly different frequencies, each locally reasonable, and the beat is only audible org-wide. A scheduled sweep listens across the field, names the practice, and proposes one standard.",
      artifact:
        live && live.divergence
          ? live.divergence.top
            ? `${live.divergence.top.practice} · ${live.divergence.top.impact} impact → one recommended standard`
            : "0 divergences — every shared practice in tune"
          : "adjudicated by qwen-max on a scheduled sweep: it proposes the standard, a platform lead ratifies it.",
      tone: band("theta", 74),
      wave: "beat" as const,
      module: "divergence" as const,
      href: isPublic ? "/demo?m=divergence" : "/console?m=divergence",
      cta: isPublic ? "see the standards board" : "open the standards board",
    },
    {
      n: "05",
      title: "Pages that cannot rot.",
      body: "Canonical memories compose into living pages — a wiki recompiled from the governed graph, not written beside it. Supersede a memory and its page follows on the next compose; a stale sentence has nowhere to hide.",
      artifact:
        live && live.docsPages !== null
          ? `${plural(live.docsPages, "page")} composed from the canonical graph · recompiled on change`
          : "every sentence traces back to the canonical memory it was compiled from — there is no unsourced line.",
      tone: band("delta", 74),
      wave: "composed" as const,
      module: "page" as const,
      href: isPublic ? "/kb" : "/console?m=docs",
      cta: isPublic ? "browse the knowledge base" : "read the pages",
    },
    {
      n: "06",
      title: "The org has a vital sign.",
      body: "Consistency, currency, liquidity, governance — four pillars folded into one score a leader can hold the org to. One unresolved cross-team contradiction caps the grade: no volume of good memories can outvote it. Track the trend, and ignore any single reading.",
      artifact:
        live && live.health
          ? `${live.health.score} · ${live.health.grade}${
              live.health.crossTeamContradictions > 0
                ? ` — capped by ${plural(live.health.crossTeamContradictions, "cross-team contradiction")}`
                : " — no cross-team contradictions in the field"
            }`
          : "the score is a gate, not a dashboard — publishing to the company wiki pauses while it sits below threshold.",
      tone: band("alpha"),
      wave: "trace" as const,
      module: "health" as const,
      href: isPublic ? "/demo?m=health" : "/console?m=health",
      cta: "read the health report",
    },
  ];

  return (
    <div className={`${FONT_DISPLAY} min-h-screen text-[#e9edff]`} style={{ background: BG }}>
      {/* header */}
      <header className="mx-auto flex max-w-7xl items-center justify-between px-6 py-5">
        <div className="flex items-center gap-3">
          <span className="text-xl font-semibold tracking-tight text-white">Brainiac</span>
          <span className={LABEL} style={{ color: GOLD }}>
            γ · binding console
          </span>
        </div>
        <nav className={`${FONT_MONO} flex flex-wrap items-center justify-end gap-x-5 gap-y-2 text-xs uppercase tracking-widest text-[#e9edff]/45`}>
          {(isPublic ? PUBLIC_NAV : PRODUCT_ROUTES).map((r) => (
            <Link key={r.path} href={r.path} className="transition hover:text-[#f3c74f]">
              {r.label}
            </Link>
          ))}
          {/* The two doors, side by side and deliberately not alike.
              `console →` is the operator gate: an existing deployment, one shared
              passcode. This is the other one: sign in with Google and get a
              project of your own. They lead somewhere different, so the visitor
              should not have to read carefully to tell them apart — hence a pill
              rather than a fifth identical link. Public surface only; an operator
              already inside has no use for it. */}
          {isPublic && (
            <Link
              href="/signup"
              className="rounded-full border px-3.5 py-1.5 transition hover:scale-[1.03]"
              style={{ borderColor: GOLD, color: GOLD }}
            >
              sign in
            </Link>
          )}
        </nav>
      </header>

      {/* field */}
      <section className="mx-auto max-w-7xl px-6">
        <div className="relative overflow-hidden rounded-xl border border-white/10">
          <canvas
            ref={canvasRef}
            className="h-[62vh] min-h-[420px] w-full touch-none"
            style={{ cursor: "crosshair" }}
            onPointerMove={onPointerMove}
            onPointerDown={onPointerDown}
            onPointerUp={() => (dragRef.current = null)}
            onPointerLeave={() => {
              cursorRef.current.on = false;
              dragRef.current = null;
            }}
            role="img"
            aria-label="Interference field of three team wave sources. Drag the emitters; your cursor is a fourth source."
          />
          {emitters.map((em) => (
            <span
              key={em.id}
              className={`${LABEL} pointer-events-none absolute -translate-x-1/2`}
              style={{
                left: `${em.x * 100}%`,
                top: `calc(${em.y * 100}% + 14px)`,
                color: Math.abs(em.phase) > 0.6 ? MAGENTA : GOLD,
              }}
            >
              {em.label}
              {em.id === "payments" && !locked && " · out of phase"}
            </span>
          ))}
          <div className="pointer-events-none absolute left-8 top-8 max-w-md">
            <motion.h1
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.8 }}
              className="text-5xl font-semibold leading-[1.03] tracking-tight text-white lg:text-6xl"
            >
              Three teams.
              <br />
              <span style={{ color: GOLD, textShadow: `0 0 42px ${GOLD_GLOW}` }}>One wave.</span>
            </motion.h1>
            <motion.p
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ delay: 0.5 }}
              className={`${FONT_MONO} mt-4 text-sm leading-relaxed text-[#e9edff]/55`}
            >
              Where their knowledge agrees, it amplifies. Where it conflicts, it cancels —
              and Brainiac makes the seam visible. Then it reads the whole field: the drift,
              the health, the standard. Drag an emitter. Add your own wave.
            </motion.p>
          </div>
          <div className="absolute bottom-6 left-8 flex flex-wrap items-center gap-4">
            <button
              onClick={phaseLock}
              className={`${FONT_MONO} rounded-full border px-5 py-2.5 text-sm font-medium transition`}
              style={{
                borderColor: locked ? GOLD : MAGENTA,
                color: locked ? GOLD : MAGENTA,
                background: locked ? "hsla(46,90%,60%,0.08)" : "rgba(255,93,162,0.08)",
              }}
            >
              {locked ? "⟲ reintroduce the contradiction" : "◉ phase-lock — resolve the contradiction"}
            </button>
            <span className={LABEL} style={{ color: "rgba(233,237,255,0.35)" }}>
              {locked ? "constructive · canonical" : "1 contradiction · payments out of phase"}
            </span>
          </div>
        </div>
        {/* live stats strip — real numbers when the brainiac server is up */}
        <div className={`${LABEL} mt-2 flex flex-wrap items-center justify-between gap-2 text-[#e9edff]/35`}>
          <span>drag emitters · your cursor is a wave source</span>
          <span>
            {live ? (
              <>
                <span style={{ color: GOLD }}>{live.canonicalCount}</span> canonical ·{" "}
                <Link href="/console?m=reviews" className="underline decoration-dotted underline-offset-4 hover:text-[#f3c74f]">
                  {live.pendingPromotions} pending review
                </Link>{" "}
                · {live.openContradictions} open contradictions · {live.embeddingModel}
              </>
            ) : (
              isPublic ? (
                <>
                  example field ·{" "}
                  <Link href="/demo" className="underline decoration-dotted underline-offset-4 hover:text-[#f3c74f]">
                    walk the demo org
                  </Link>
                </>
              ) : (
                <>demo field · connect BRAINIAC_API_URL for live numbers</>
              )
            )}
          </span>
        </div>
      </section>

      {/* scroll story along the sine spine */}
      <div ref={trackRef} className="relative mx-auto max-w-5xl px-6 pt-20">
        <svg
          aria-hidden
          className="pointer-events-none absolute left-1/2 top-0 hidden h-full w-[120px] -translate-x-1/2 md:block"
          viewBox="0 0 120 900"
          preserveAspectRatio="none"
          fill="none"
        >
          <path
            d="M60 0 C 100 50, 100 100, 60 150 C 20 200, 20 250, 60 300 C 100 350, 100 400, 60 450 C 20 500, 20 550, 60 600 C 100 650, 100 700, 60 750 C 20 800, 20 850, 60 900"
            stroke="hsla(46,90%,68%,0.12)"
            strokeWidth="2"
          />
          <motion.path
            d="M60 0 C 100 50, 100 100, 60 150 C 20 200, 20 250, 60 300 C 100 350, 100 400, 60 450 C 20 500, 20 550, 60 600 C 100 650, 100 700, 60 750 C 20 800, 20 850, 60 900"
            stroke={GOLD}
            strokeWidth="2"
            style={{ pathLength }}
          />
        </svg>

        {STATIONS.map((s, i) => (
          <Fragment key={s.n}>
            {/* The movement break: 01–03 are the mechanism, 04–06 are what the
                whole field computes that no single session can. */}
            {s.n === "04" && (
              <motion.div
                initial={{ opacity: 0 }}
                whileInView={{ opacity: 1 }}
                viewport={{ once: false, amount: 0.6 }}
                transition={{ duration: 0.6 }}
                className="relative py-16 text-center"
              >
                <div className={LABEL} style={{ color: GOLD }}>
                  · the second movement ·
                </div>
                <h2 className="mx-auto mt-4 max-w-2xl text-3xl font-semibold leading-tight tracking-tight text-white">
                  Single sessions make memories.
                  <br />
                  The field makes intelligence.
                </h2>
                <p className={`${FONT_MONO} mx-auto mt-4 max-w-md text-sm leading-relaxed text-[#e9edff]/50`}>
                  Past the mechanism, Brainiac reads the whole interference
                  pattern — the reports no single team could write.
                </p>
              </motion.div>
            )}
            <motion.section
              initial={{ opacity: 0.1, scale: 0.92 }}
              whileInView={{ opacity: 1, scale: 1 }}
              viewport={{ once: false, amount: 0.5 }}
              transition={{ duration: 0.5 }}
              className="relative grid min-h-[64vh] items-center gap-8 py-10 md:grid-cols-2"
            >
            <div className={i % 2 ? "md:order-2" : ""}>
              <div className={LABEL} style={{ color: s.tone }}>
                station {s.n}
              </div>
              <h2 className="mt-3 text-4xl font-semibold leading-tight tracking-tight text-white">
                {s.title}
              </h2>
              <p className={`${FONT_MONO} mt-4 max-w-md text-sm leading-relaxed text-[#e9edff]/55`}>
                {s.body}
              </p>
              {/* The station's module, minimized: a working figure of the very
                  surface the CTA below opens. `artifact` — the line that used to
                  be the whole artifact, and is still the only place live numbers
                  are told — is now its caption. */}
              <StationModule kind={s.module} tone={s.tone} caption={s.artifact} />
              <Link
                href={s.href}
                className={`${FONT_MONO} mt-4 inline-block text-xs uppercase tracking-[0.18em] underline decoration-dotted underline-offset-4 transition hover:text-[#f3c74f]`}
                style={{ color: s.tone }}
              >
                → {s.cta}
              </Link>
            </div>
            <div className={`flex justify-center ${i % 2 ? "md:order-1" : ""}`}>
              <StationWave kind={s.wave} tone={s.tone} />
            </div>
            </motion.section>
          </Fragment>
        ))}

        <footer className={`${LABEL} flex flex-wrap items-center justify-between gap-3 border-t border-white/10 py-6 text-[#e9edff]/35`}>
          <span>brainiac · constructive by design</span>
          <Link href="/pitch" className="transition hover:text-[#f3c74f]">
            → why brainiac · the evidence
          </Link>
          <Link href="/demo" className="transition hover:text-[#f3c74f]">
            → live demo · fixture org
          </Link>
          {/* No existing endpoint exposes eval/gate/RLS-leak results, so the
              "0 leaks" claim is unverifiable — in live mode we state real,
              checkable numbers instead. The fiction stays only in demo mode. */}
          <span style={{ color: GOLD }}>
            {live
              ? `${plural(live.canonicalCount, "canonical")} · ${plural(live.openContradictions, "open contradiction")}`
              : "0 leaks · every phase-lock signed"}
          </span>
        </footer>
      </div>
    </div>
  );
}

type WaveKind = "in" | "anti" | "locked" | "beat" | "composed" | "trace";

/**
 * Station mini-figures, one physics idea each:
 * - in / anti / locked — two waves adding, cancelling, phase-locked (01–03)
 * - beat     — two detuned frequencies; the beat envelope IS the divergence (04)
 * - composed — harmonics binding into one waveform; the page from the graph (05)
 * - trace    — the org's EEG: the composite vital sign, capped by one event (06)
 */
function StationWave({ kind, tone }: { kind: WaveKind; tone: string }) {
  const w = 380;
  const mkFn = (fn: (x: number) => number) => {
    let d = "";
    for (let x = 0; x <= w; x += 4) {
      const y = fn(x);
      d += x === 0 ? `M${x} ${y}` : ` L${x} ${y}`;
    }
    return d;
  };
  const sine =
    (freq: number, amp: number, phase: number, mid: number) => (x: number) =>
      mid + Math.sin((x / w) * Math.PI * freq + phase) * amp;
  const mk = (amp: number, phase: number, mid: number) => mkFn(sine(4, amp, phase, mid));

  const wave = { fill: "none" as const, strokeWidth: 1.5 };
  const inView = {
    initial: { pathLength: 0 },
    whileInView: { pathLength: 1 },
    viewport: { once: false, amount: 0.6 },
  };
  const sumView = {
    initial: { pathLength: 0, opacity: 0 },
    whileInView: { pathLength: 1, opacity: 1 },
    viewport: { once: false, amount: 0.6 },
  };
  const labelStyle = { textTransform: "uppercase" as const, letterSpacing: "0.2em" };

  if (kind === "beat") {
    // Two detuned frequencies (not opposite phase — contradiction owns that);
    // their sum is the classic beat envelope: drift you can only hear org-wide.
    const beatSum = (x: number) =>
      175 + Math.sin((x / w) * Math.PI * 10) * Math.cos((x / w) * Math.PI * 2) * 34;
    return (
      <svg viewBox={`0 0 ${w} 220`} className="w-full max-w-md" role="img" aria-label="Two slightly detuned waves producing a beat envelope">
        <motion.path d={mkFn(sine(8, 18, 0, 50))} {...wave} stroke="hsla(224,90%,72%,0.5)" {...inView} transition={{ duration: 0.9 }} />
        <motion.path d={mkFn(sine(12, 18, 0, 95))} {...wave} stroke="hsla(224,90%,72%,0.5)" {...inView} transition={{ duration: 0.9, delay: 0.15 }} />
        <line x1="0" y1="132" x2={w} y2="132" stroke="rgba(255,255,255,0.12)" strokeDasharray="3 5" />
        <motion.path d={mkFn(beatSum)} fill="none" stroke={tone} strokeWidth="2.4" {...sumView} transition={{ duration: 1.1, delay: 0.5 }} />
        <text x="4" y="20" fontSize="10" fill="rgba(255,255,255,0.4)" style={labelStyle}>
          same practice · detuned
        </text>
        <text x="4" y="152" fontSize="10" fill={tone} style={labelStyle}>
          beat — drift only audible org-wide
        </text>
      </svg>
    );
  }

  if (kind === "composed") {
    // Harmonics binding into one waveform — many teams' canonical memories
    // composing a single page that recompiles when any harmonic changes.
    const composedSum = (x: number) =>
      175 +
      (Math.sin((x / w) * Math.PI * 4) * 14 +
        Math.sin((x / w) * Math.PI * 8) * 8 +
        Math.sin((x / w) * Math.PI * 16) * 4) *
        1.3;
    return (
      <svg viewBox={`0 0 ${w} 220`} className="w-full max-w-md" role="img" aria-label="Three harmonics composing into one waveform">
        <motion.path d={mkFn(sine(4, 12, 0, 36))} {...wave} stroke="hsla(262,90%,72%,0.45)" {...inView} transition={{ duration: 0.9 }} />
        <motion.path d={mkFn(sine(8, 8, 0, 68))} {...wave} stroke="hsla(262,90%,72%,0.45)" {...inView} transition={{ duration: 0.9, delay: 0.12 }} />
        <motion.path d={mkFn(sine(16, 5, 0, 100))} {...wave} stroke="hsla(262,90%,72%,0.45)" {...inView} transition={{ duration: 0.9, delay: 0.24 }} />
        <line x1="0" y1="132" x2={w} y2="132" stroke="rgba(255,255,255,0.12)" strokeDasharray="3 5" />
        <motion.path d={mkFn(composedSum)} fill="none" stroke={tone} strokeWidth="2.4" {...sumView} transition={{ duration: 1.1, delay: 0.5 }} />
        <text x="4" y="20" fontSize="10" fill="rgba(255,255,255,0.4)" style={labelStyle}>
          canonical memories · many teams
        </text>
        <text x="4" y="152" fontSize="10" fill={tone} style={labelStyle}>
          one page — recompiled on change
        </text>
      </svg>
    );
  }

  if (kind === "trace") {
    // The org's EEG: a calm composite trace, one sharp magenta dip — the
    // cross-team contradiction that caps the score — then recovery.
    const dip = (x: number) => -46 * Math.exp(-(((x - w * 0.62) / 20) ** 2));
    const eeg = (x: number) =>
      118 +
      Math.sin((x / w) * Math.PI * 14) * 5 +
      Math.sin((x / w) * Math.PI * 3.6 + 1.3) * 9 +
      Math.sin((x / w) * Math.PI * 23 + 0.5) * 2.5 -
      dip(x);
    return (
      <svg viewBox={`0 0 ${w} 220`} className="w-full max-w-md" role="img" aria-label="A composite vital-sign trace with one sharp dip">
        <line x1="0" y1="142" x2={w} y2="142" stroke="rgba(255,255,255,0.14)" strokeDasharray="3 5" />
        <text x="4" y="156" fontSize="10" fill="rgba(255,255,255,0.35)" style={labelStyle}>
          healthy above this line
        </text>
        <motion.path d={mkFn(eeg)} fill="none" stroke={tone} strokeWidth="2.2" {...sumView} transition={{ duration: 1.4 }} />
        <motion.circle
          cx={w * 0.62}
          cy={118 + 46}
          r={3.5}
          fill={MAGENTA}
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: false, amount: 0.6 }}
          transition={{ delay: 1.1 }}
        />
        <text x={Math.min(w * 0.62 + 10, w - 120)} y={118 + 50} fontSize="10" fill={MAGENTA} style={labelStyle}>
          the cap
        </text>
        <text x="4" y="20" fontSize="10" fill="rgba(255,255,255,0.4)" style={labelStyle}>
          four pillars · one composite
        </text>
        <text x="4" y="210" fontSize="10" fill={tone} style={labelStyle}>
          trend over any single reading
        </text>
      </svg>
    );
  }

  // 01–03: the original figures, unchanged.
  const phaseB = kind === "anti" ? Math.PI : 0;
  const sumAmp = kind === "anti" ? 2 : 36;
  return (
    <svg
      viewBox={`0 0 ${w} 220`}
      className="w-full max-w-md"
      role="img"
      aria-label={`Two waves ${kind === "anti" ? "cancelling" : "adding"}`}
    >
      <motion.path
        d={mk(18, 0, 50)}
        fill="none"
        stroke="hsla(46,90%,68%,0.5)"
        strokeWidth="1.5"
        initial={{ pathLength: 0 }}
        whileInView={{ pathLength: 1 }}
        viewport={{ once: false, amount: 0.6 }}
        transition={{ duration: 0.9 }}
      />
      <motion.path
        d={mk(18, phaseB, 95)}
        fill="none"
        stroke={kind === "anti" ? "rgba(255,93,162,0.6)" : "hsla(46,90%,68%,0.5)"}
        strokeWidth="1.5"
        initial={{ pathLength: 0 }}
        whileInView={{ pathLength: 1 }}
        viewport={{ once: false, amount: 0.6 }}
        transition={{ duration: 0.9, delay: 0.15 }}
      />
      <line x1="0" y1="132" x2={w} y2="132" stroke="rgba(255,255,255,0.12)" strokeDasharray="3 5" />
      <motion.path
        d={mk(sumAmp, 0, 175)}
        fill="none"
        stroke={tone}
        strokeWidth="2.4"
        initial={{ pathLength: 0, opacity: 0 }}
        whileInView={{ pathLength: 1, opacity: 1 }}
        viewport={{ once: false, amount: 0.6 }}
        transition={{ duration: 1.1, delay: 0.5 }}
      />
      <text x="4" y="20" fontSize="10" fill="rgba(255,255,255,0.4)" style={labelStyle}>
        {kind === "anti" ? "claims out of phase" : "claims in phase"}
      </text>
      <text x="4" y="152" fontSize="10" fill={tone} style={labelStyle}>
        {kind === "anti" ? "sum ≈ 0 — contradiction" : "sum amplified — canonical"}
      </text>
    </svg>
  );
}
