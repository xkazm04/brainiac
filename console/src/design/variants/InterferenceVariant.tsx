"use client";

/*
 * Variant F — "Interference". Wave physics as the brand: every team is a
 * wave source. Where their knowledge agrees the waves add — bright,
 * canonical. Where it conflicts they cancel — a visible dark seam in the
 * field. The hero is a live dot-matrix interference field: drag the team
 * emitters, lend it your cursor as a fourth source, then hit PHASE-LOCK and
 * watch a contradiction resolve into constructive light. Below, the story
 * scrolls along a sine spine, kp-about style.
 * Fixed art direction → literal hexes.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { motion, useReducedMotion, useScroll, useSpring } from "framer-motion";

import { CANONICAL_DEMO, CONTRADICTION, QUEUE } from "../demo-data";

const DISPLAY = "font-[family-name:var(--font-cortex-display)]";
const TEXT = "font-[family-name:var(--font-cortex-text)]";

const BG = "#05060b";
const CYAN = "#8ad8ff";
const MAGENTA = "#ff5da2";

interface Emitter {
  id: string;
  label: string;
  x: number; // 0..1
  y: number; // 0..1
  phase: number; // radians; π = anti-phase (the contradiction)
  targetPhase: number;
}

const SPACING = 17;
const WAVELEN = 96; // px
const OMEGA = 2.4; // rad/s

export default function InterferenceVariant() {
  const reduce = !!useReducedMotion();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const emittersRef = useRef<Emitter[]>([
    { id: "payments", label: "payments", x: 0.26, y: 0.4, phase: Math.PI, targetPhase: Math.PI },
    { id: "platform", label: "platform", x: 0.62, y: 0.3, phase: 0, targetPhase: 0 },
    { id: "data", label: "data", x: 0.5, y: 0.68, phase: 0, targetPhase: 0 },
  ]);
  const cursorRef = useRef<{ x: number; y: number; on: boolean }>({ x: 0, y: 0, on: false });
  const dragRef = useRef<string | null>(null);
  const [locked, setLocked] = useState(false);
  const [, forceTick] = useState(0); // re-render labels while dragging

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
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const k = (Math.PI * 2) / WAVELEN;

    const draw = (now: number) => {
      if (disposed) return;
      const rect = canvas.getBoundingClientRect();
      const W = rect.width;
      const H = rect.height;
      const t = now / 1000;
      ctx.clearRect(0, 0, W, H);

      const emitters = emittersRef.current;
      // ease phases toward their target (the phase-lock animation)
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
          const b = sum / norm; // ≈ −1..1
          if (b > 0.18) {
            // constructive — signal light
            const a = Math.min(1, (b - 0.18) * 1.7);
            ctx.fillStyle = `rgba(${138 + a * 100}, ${216 + a * 39}, 255, ${0.16 + a * 0.8})`;
            const r = 1 + a * 1.9;
            ctx.fillRect(gx - r / 2, gy - r / 2, r, r);
          } else if (b < -0.42) {
            // deep destructive — the seam
            const a = Math.min(1, (-b - 0.42) * 2.2);
            ctx.fillStyle = `rgba(255, 93, 162, ${0.06 + a * 0.34})`;
            ctx.fillRect(gx - 0.75, gy - 0.75, 1.5, 1.5);
          } else {
            ctx.fillStyle = "rgba(120, 138, 210, 0.10)";
            ctx.fillRect(gx - 0.5, gy - 0.5, 1, 1);
          }
        }
      }

      // emitter rings
      for (const e of emitters) {
        const ex = e.x * W;
        const ey = e.y * H;
        const anti = Math.abs(e.phase) > 0.6;
        ctx.strokeStyle = anti ? MAGENTA : CYAN;
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
    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [reduce]);

  // pointer interaction: drag emitters, otherwise be a wave source
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

  // scroll spine
  const trackRef = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({ target: trackRef, offset: ["start center", "end end"] });
  const pathLength = useSpring(scrollYProgress, { stiffness: 120, damping: 30 });

  const emitters = emittersRef.current;

  const STATIONS = [
    {
      n: "01",
      title: "Every team is a source.",
      body: "Payments, platform, data — each session broadcasts what it learned. Raw memories, provenance attached, nothing lost in the noise floor.",
      artifact: QUEUE[0].content,
      tone: CYAN,
      wave: "in" as const,
    },
    {
      n: "02",
      title: "Disagreement is visible.",
      body: "Two claims out of phase don't average out — they cancel. Brainiac finds the dark seams and files them as contradictions, with a suggested resolution.",
      artifact: `− ${CONTRADICTION.a}   + ${CONTRADICTION.b}`,
      tone: MAGENTA,
      wave: "anti" as const,
    },
    {
      n: "03",
      title: "Phase-lock makes it canonical.",
      body: `A maintainer signs the resolution and the sources align: ${CANONICAL_DEMO.aliases.map((a) => `“${a.name}”`).join(", ")} — one bright node, ${CANONICAL_DEMO.name}, visible to everyone cleared to see it.`,
      artifact: `${CANONICAL_DEMO.name} ⇐ ${CANONICAL_DEMO.aliases.map((a) => a.team).join(" + ")} · constructive`,
      tone: "#dff3ff",
      wave: "locked" as const,
    },
  ];

  return (
    <div className={`${TEXT} min-h-screen text-[#e8ecff]`} style={{ background: BG }}>
      {/* header */}
      <header className="mx-auto flex max-w-7xl items-center justify-between px-6 py-5">
        <div className="flex items-baseline gap-3">
          <span className={`${DISPLAY} text-2xl tracking-tight text-white`}>Brainiac</span>
          <span className="text-[11px] uppercase tracking-[0.22em] text-[#8ad8ff]/70">interference lab</span>
        </div>
        <nav className="flex items-center gap-6 text-sm text-white/45">
          <span className="cursor-pointer transition hover:text-[#8ad8ff]">Reviews</span>
          <span className="cursor-pointer transition hover:text-[#8ad8ff]">Graph</span>
          <span className="cursor-pointer transition hover:text-[#8ad8ff]">Analytics</span>
        </nav>
      </header>

      {/* field */}
      <section className="mx-auto max-w-7xl px-6">
        <div ref={wrapRef} className="relative overflow-hidden rounded-xl border border-white/10">
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
          {/* emitter labels */}
          {emitters.map((em) => (
            <span
              key={em.id}
              className="pointer-events-none absolute -translate-x-1/2 text-[10px] uppercase tracking-[0.2em]"
              style={{
                left: `${em.x * 100}%`,
                top: `calc(${em.y * 100}% + 14px)`,
                color: Math.abs(em.phase) > 0.6 ? MAGENTA : CYAN,
              }}
            >
              {em.label}
              {em.id === "payments" && !locked && " · out of phase"}
            </span>
          ))}
          {/* copy */}
          <div className="pointer-events-none absolute left-8 top-8 max-w-md">
            <motion.h1
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.8 }}
              className={`${DISPLAY} text-5xl leading-[1.03] tracking-tight text-white lg:text-6xl`}
            >
              Three teams.
              <br />
              <em style={{ color: CYAN }}>One wave.</em>
            </motion.h1>
            <motion.p
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ delay: 0.5 }}
              className="mt-4 text-sm leading-relaxed text-white/55"
            >
              Where their knowledge agrees, it amplifies. Where it conflicts, it cancels —
              and Brainiac makes the seam visible. Drag an emitter. Add your own wave.
            </motion.p>
          </div>
          {/* phase-lock control */}
          <div className="absolute bottom-6 left-8 flex items-center gap-4">
            <button
              onClick={phaseLock}
              className="rounded-full border px-5 py-2.5 text-sm font-medium transition"
              style={{
                borderColor: locked ? CYAN : MAGENTA,
                color: locked ? CYAN : MAGENTA,
                background: locked ? "rgba(138,216,255,0.08)" : "rgba(255,93,162,0.08)",
              }}
            >
              {locked ? "⟲ reintroduce the contradiction" : "◉ phase-lock — resolve the contradiction"}
            </button>
            <span className="text-[11px] uppercase tracking-[0.18em] text-white/35">
              {locked ? "constructive · canonical" : "1 contradiction · payments out of phase"}
            </span>
          </div>
        </div>
      </section>

      {/* scroll story along a sine spine */}
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
            stroke="rgba(138,216,255,0.12)"
            strokeWidth="2"
          />
          <motion.path
            d="M60 0 C 100 50, 100 100, 60 150 C 20 200, 20 250, 60 300 C 100 350, 100 400, 60 450 C 20 500, 20 550, 60 600 C 100 650, 100 700, 60 750 C 20 800, 20 850, 60 900"
            stroke={CYAN}
            strokeWidth="2"
            style={{ pathLength }}
          />
        </svg>

        {STATIONS.map((s, i) => (
          <motion.section
            key={s.n}
            initial={{ opacity: 0.1, scale: 0.92 }}
            whileInView={{ opacity: 1, scale: 1 }}
            viewport={{ once: false, amount: 0.5 }}
            transition={{ duration: 0.5 }}
            className={`relative grid min-h-[64vh] items-center gap-8 py-10 md:grid-cols-2 ${i % 2 ? "" : ""}`}
          >
            <div className={i % 2 ? "md:order-2" : ""}>
              <div className="text-[11px] uppercase tracking-[0.24em]" style={{ color: s.tone }}>
                station {s.n}
              </div>
              <h2 className={`${DISPLAY} mt-3 text-4xl leading-tight tracking-tight text-white`}>{s.title}</h2>
              <p className="mt-4 max-w-md text-sm leading-relaxed text-white/55">{s.body}</p>
              <div className="mt-5 rounded-lg border border-white/10 bg-white/[0.03] p-4 text-sm leading-relaxed text-white/70">
                {s.artifact}
              </div>
            </div>
            <div className={`flex justify-center ${i % 2 ? "md:order-1" : ""}`}>
              <StationWave kind={s.wave} tone={s.tone} />
            </div>
          </motion.section>
        ))}

        <footer className="flex items-center justify-between border-t border-white/10 py-6 text-[11px] uppercase tracking-[0.18em] text-white/35">
          <span>brainiac · constructive by design</span>
          <span style={{ color: CYAN }}>0 leaks · every phase-lock signed</span>
        </footer>
      </div>
    </div>
  );
}

/** Station mini-figure: two waves in phase / anti-phase / locked, with their sum. */
function StationWave({ kind, tone }: { kind: "in" | "anti" | "locked"; tone: string }) {
  const w = 380;
  const mk = (amp: number, phase: number, mid: number) => {
    let d = "";
    for (let x = 0; x <= w; x += 4) {
      const y = mid + Math.sin((x / w) * Math.PI * 4 + phase) * amp;
      d += x === 0 ? `M${x} ${y}` : ` L${x} ${y}`;
    }
    return d;
  };
  const phaseB = kind === "anti" ? Math.PI : 0;
  const sumAmp = kind === "anti" ? 2 : 36;
  return (
    <svg viewBox={`0 0 ${w} 220`} className="w-full max-w-md" role="img" aria-label={`Two waves ${kind === "anti" ? "cancelling" : "adding"}`}>
      <motion.path
        d={mk(18, 0, 50)}
        fill="none"
        stroke="rgba(138,216,255,0.5)"
        strokeWidth="1.5"
        initial={{ pathLength: 0 }}
        whileInView={{ pathLength: 1 }}
        viewport={{ once: false, amount: 0.6 }}
        transition={{ duration: 0.9 }}
      />
      <motion.path
        d={mk(18, phaseB, 95)}
        fill="none"
        stroke={kind === "anti" ? "rgba(255,93,162,0.6)" : "rgba(138,216,255,0.5)"}
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
      <text x="4" y="20" fontSize="10" fill="rgba(255,255,255,0.4)" style={{ textTransform: "uppercase", letterSpacing: "0.2em" }}>
        {kind === "anti" ? "claims out of phase" : "claims in phase"}
      </text>
      <text x="4" y="152" fontSize="10" fill={tone} style={{ textTransform: "uppercase", letterSpacing: "0.2em" }}>
        {kind === "anti" ? "sum ≈ 0 — contradiction" : "sum amplified — canonical"}
      </text>
    </svg>
  );
}
