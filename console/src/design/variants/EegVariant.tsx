"use client";

/*
 * Variant D — "EEG". The page is not decorated with brain waves; the page IS
 * the instrument. A live six-channel recording — one channel per pipeline
 * stage — runs the full hero. The cursor injects spikes into the nearest
 * channel; the STIMULUS button fires a knowledge event and you watch it
 * conduct down the channels, capture → distribute. Sections below are
 * annotations on the recording, not cards floating in space.
 * Fixed art direction → literal hexes.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { motion, useReducedMotion } from "framer-motion";

import { CONTRADICTION, KPIS, PIPELINE_STAGES, QUEUE } from "../demo-data";

const MONO = "font-[family-name:var(--font-synapse-mono)]";
const DISPLAY = "font-[family-name:var(--font-synapse-display)]";

const BG = "#07080a";
const TRACE = "rgba(214, 232, 255, 0.72)";
const TRACE_DIM = "rgba(214, 232, 255, 0.28)";
const MINT = "#6ef3c5";
const AMBER = "#f5c451";
const GRID = "rgba(214, 232, 255, 0.05)";

interface Spike {
  x: number; // 0..1 position along the strip
  born: number; // ms timestamp
  energy: number;
}

interface Channel {
  freq: number;
  amp: number;
  phase: number;
  spikes: Spike[];
  excite: number; // 0..1 hover boost, eased in the loop
  target: number;
}

const CHANNEL_DEFS = PIPELINE_STAGES.map((stage, i) => ({
  stage,
  freq: 1.6 + i * 0.9,
  amp: 8 + (i % 3) * 4,
}));

function useEegCanvas(reduce: boolean) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const channelsRef = useRef<Channel[]>(
    CHANNEL_DEFS.map((c) => ({
      freq: c.freq,
      amp: c.amp,
      phase: Math.random() * Math.PI * 2,
      spikes: [],
      excite: 0,
      target: 0,
    })),
  );
  const cascadeRef = useRef<{ started: number } | null>(null);

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

    const draw = (now: number) => {
      if (disposed) return;
      const rect = canvas.getBoundingClientRect();
      const W = rect.width;
      const H = rect.height;
      const chans = channelsRef.current;
      const n = chans.length;
      const laneH = H / n;
      const t = now / 1000;

      ctx.clearRect(0, 0, W, H);

      // calibration grid
      ctx.strokeStyle = GRID;
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let gx = 0; gx < W; gx += 56) {
        ctx.moveTo(gx, 0);
        ctx.lineTo(gx, H);
      }
      for (let gy = 0; gy < H; gy += laneH / 2) {
        ctx.moveTo(0, gy);
        ctx.lineTo(W, gy);
      }
      ctx.stroke();

      // cascade: a travelling excitation front, one lane after another
      const cascade = cascadeRef.current;

      for (let c = 0; c < n; c++) {
        const ch = chans[c];
        ch.excite += (ch.target - ch.excite) * 0.08;
        const mid = laneH * c + laneH / 2;

        // cascade injects a spike into lane c at its conduction time
        if (cascade) {
          const fireAt = cascade.started + c * 260;
          if (now >= fireAt && now < fireAt + 40) {
            const last = ch.spikes[ch.spikes.length - 1];
            if (!last || now - last.born > 200) {
              ch.spikes.push({ x: 0.24 + c * 0.1, born: now, energy: 34 });
            }
          }
        }
        // retire old spikes
        ch.spikes = ch.spikes.filter((s) => now - s.born < 2600);

        const boost = 1 + ch.excite * 1.6;
        const active = ch.excite > 0.25;

        ctx.beginPath();
        const step = 3;
        for (let px = 0; px <= W; px += step) {
          const u = px / W;
          const base =
            Math.sin(u * Math.PI * 2 * ch.freq + ch.phase + t * 2.2) * ch.amp +
            Math.sin(u * Math.PI * 2 * ch.freq * 2.7 + t * 3.1) * (ch.amp * 0.28);
          let spikeY = 0;
          for (const s of ch.spikes) {
            const age = (now - s.born) / 1000;
            const travel = u - s.x - age * 0.16; // spikes drift right
            const g = Math.exp(-((travel * W) ** 2) / (2 * 14 ** 2));
            spikeY += -Math.abs(Math.sin(age * 18)) * s.energy * Math.exp(-age * 1.8) * g;
          }
          const y = mid + (base * boost + spikeY) * (reduce ? 0.6 : 1);
          if (px === 0) ctx.moveTo(px, y);
          else ctx.lineTo(px, y);
        }
        ctx.strokeStyle = active ? MINT : ch.spikes.length ? TRACE : TRACE_DIM;
        ctx.lineWidth = active ? 1.6 : 1.1;
        ctx.shadowColor = active ? MINT : "transparent";
        ctx.shadowBlur = active ? 10 : 0;
        ctx.stroke();
        ctx.shadowBlur = 0;
      }

      // sweep cursor
      if (!reduce) {
        const sweepX = ((now / 24) % (W + 120)) - 60;
        const grad = ctx.createLinearGradient(sweepX - 50, 0, sweepX, 0);
        grad.addColorStop(0, "rgba(110,243,197,0)");
        grad.addColorStop(1, "rgba(110,243,197,0.12)");
        ctx.fillStyle = grad;
        ctx.fillRect(sweepX - 50, 0, 50, H);
      }

      if (cascade && now - cascade.started > n * 260 + 2800) cascadeRef.current = null;
      raf = requestAnimationFrame(draw);
    };

    if (reduce) {
      // one static frame
      draw(performance.now());
      disposed = true;
    } else {
      raf = requestAnimationFrame(draw);
    }
    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [reduce]);

  const inject = useCallback((clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const u = (clientX - rect.left) / rect.width;
    const lane = Math.min(
      CHANNEL_DEFS.length - 1,
      Math.max(0, Math.floor(((clientY - rect.top) / rect.height) * CHANNEL_DEFS.length)),
    );
    const ch = channelsRef.current[lane];
    const last = ch.spikes[ch.spikes.length - 1];
    const now = performance.now();
    if (!last || now - last.born > 90) {
      ch.spikes.push({ x: u, born: now, energy: 16 });
    }
  }, []);

  const excite = useCallback((lane: number | null) => {
    channelsRef.current.forEach((ch, i) => {
      ch.target = lane === i ? 1 : 0;
    });
  }, []);

  const cascade = useCallback(() => {
    cascadeRef.current = { started: performance.now() };
  }, []);

  return { canvasRef, inject, excite, cascade };
}

const SECTIONS = [
  {
    stage: 0,
    tag: "ch 01 · capture",
    title: "Every session leaves a trace.",
    body: "Claude Code, Cursor, CI agents — each one emits signal. Brainiac records it raw, with full provenance: who, which model, which session.",
    artifact: QUEUE[0].content,
    label: "captured 26m ago · payments",
  },
  {
    stage: 3,
    tag: "ch 04 · contradict",
    title: "Artifacts don't hide in this recording.",
    body: "When two memories oscillate out of phase, the instrument flags it. A human resolves it — supersede, coexist, or dismiss — and the correction is signed.",
    artifact: `− ${CONTRADICTION.a}   → + ${CONTRADICTION.b}`,
    label: "supersession suggested · psp-gateway",
  },
  {
    stage: 5,
    tag: "ch 06 · distribute",
    title: "The next session starts already knowing.",
    body: "memory_context() at session start: canonical, permission-filtered, cited. An agent can never read more than its operator — RLS holds at any amplitude.",
    artifact: "$ memory_context --task \"checkout refund flow\"  →  8 canonical · 3 teams · 0 leaks",
    label: "served in 41ms · rls enforced",
  },
] as const;

export default function EegVariant() {
  const reduce = !!useReducedMotion();
  const { canvasRef, inject, excite, cascade } = useEegCanvas(reduce);
  const [fired, setFired] = useState(0);

  return (
    <div className={`${MONO} min-h-screen text-[#d6e8ff]`} style={{ background: BG }}>
      {/* header — instrument titlebar */}
      <header className="mx-auto flex max-w-7xl items-center justify-between px-6 py-4 text-xs">
        <div className="flex items-center gap-3">
          <span className="inline-block h-2 w-2 animate-[pulse-glow_2s_ease-in-out_infinite] rounded-full bg-[#6ef3c5]" />
          <span className={`${DISPLAY} text-base font-semibold tracking-tight text-white`}>Brainiac</span>
          <span className="text-[#d6e8ff]/35">EEG · org=meridian · 6 ch · rec ●</span>
        </div>
        <nav className="flex items-center gap-5 uppercase tracking-widest text-[#d6e8ff]/45">
          <span className="cursor-pointer transition hover:text-[#6ef3c5]">reviews</span>
          <span className="cursor-pointer transition hover:text-[#6ef3c5]">graph</span>
          <span className="cursor-pointer transition hover:text-[#6ef3c5]">analytics</span>
        </nav>
      </header>

      {/* the instrument */}
      <section className="relative mx-auto max-w-7xl px-6">
        <div className="relative overflow-hidden rounded-lg border border-[#d6e8ff]/10">
          {/* channel labels */}
          <div className="pointer-events-none absolute left-0 top-0 z-10 flex h-full flex-col justify-around py-2 pl-3 text-[10px] uppercase tracking-[0.18em] text-[#d6e8ff]/40">
            {CHANNEL_DEFS.map((c, i) => (
              <span key={c.stage}>
                <span className="text-[#d6e8ff]/25">{String(i + 1).padStart(2, "0")}</span> {c.stage}
              </span>
            ))}
          </div>
          <canvas
            ref={canvasRef}
            className="h-[56vh] min-h-[380px] w-full cursor-crosshair"
            onMouseMove={(e) => !reduce && inject(e.clientX, e.clientY)}
            onClick={(e) => inject(e.clientX, e.clientY)}
            aria-label="Live six-channel recording of the knowledge pipeline. Move the cursor to inject a signal."
            role="img"
          />
          {/* hero copy overlaid on the quietest corner */}
          <div className="pointer-events-none absolute right-6 top-6 z-10 max-w-sm text-right">
            <motion.h1
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.7 }}
              className={`${DISPLAY} text-4xl font-semibold leading-[1.05] tracking-tight text-white lg:text-5xl`}
            >
              Your org is
              <br />
              always <span className="text-[#6ef3c5]">emitting.</span>
            </motion.h1>
            <motion.p
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ delay: 0.4 }}
              className="mt-3 text-sm leading-relaxed text-[#d6e8ff]/55"
            >
              Brainiac is the instrument that records it, reviews it, and plays it back
              into every agent session.
            </motion.p>
          </div>
          {/* stimulus */}
          <div className="absolute bottom-4 right-6 z-10 flex items-center gap-3">
            {fired > 0 && (
              <span className="text-[10px] uppercase tracking-widest text-[#f5c451]">
                event #{String(113 + fired).padStart(3, "0")} conducting…
              </span>
            )}
            <button
              onClick={() => {
                cascade();
                setFired((n) => n + 1);
              }}
              className="pointer-events-auto rounded border border-[#f5c451]/60 bg-[#f5c451]/10 px-4 py-2 text-xs font-semibold uppercase tracking-widest text-[#f5c451] transition hover:bg-[#f5c451]/25"
            >
              ⚡ stimulus — capture a learning
            </button>
          </div>
        </div>
        <div className="mt-2 flex items-center justify-between text-[10px] uppercase tracking-widest text-[#d6e8ff]/30">
          <span>gain 7.5 µV/mm · sweep 30 mm/s · move your cursor across the strip</span>
          <span>
            {KPIS.map((k) => `${k.label}: ${k.value}`).join(" · ")}
          </span>
        </div>
      </section>

      {/* annotations — each section is an event on a channel */}
      <div className="mx-auto max-w-7xl px-6 pb-20 pt-16">
        {SECTIONS.map((s, i) => (
          <motion.section
            key={s.tag}
            initial={{ opacity: 0, y: 26 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.5 }}
            transition={{ duration: 0.55 }}
            onViewportEnter={() => excite(s.stage)}
            onViewportLeave={() => excite(null)}
            onMouseEnter={() => excite(s.stage)}
            onMouseLeave={() => excite(null)}
            className="group grid gap-6 border-t border-[#d6e8ff]/10 py-14 lg:grid-cols-[220px_1fr_1fr]"
          >
            <div className="text-[11px] uppercase tracking-[0.2em] text-[#6ef3c5]">
              {s.tag}
              <div className="mt-2 h-px w-10 bg-[#6ef3c5]/50 transition-all duration-500 group-hover:w-full" />
            </div>
            <div>
              <h2 className={`${DISPLAY} text-3xl font-semibold leading-tight tracking-tight text-white`}>
                {s.title}
              </h2>
              <p className="mt-3 max-w-md text-sm leading-relaxed text-[#d6e8ff]/55">{s.body}</p>
            </div>
            <div className="self-center">
              <div className="rounded border border-[#f5c451]/25 bg-[#f5c451]/[0.04] p-4">
                <div className="text-[10px] uppercase tracking-widest text-[#f5c451]/80">
                  ▼ annotation · {s.label}
                </div>
                <p className="mt-2 text-sm leading-relaxed text-[#d6e8ff]/80">{s.artifact}</p>
              </div>
            </div>
          </motion.section>
        ))}

        <footer className="flex items-center justify-between border-t border-[#d6e8ff]/10 pt-6 text-[10px] uppercase tracking-widest text-[#d6e8ff]/30">
          <span>brainiac eeg · recording since W23 · every event signed</span>
          <span className="text-[#6ef3c5]">0 leaks at any amplitude</span>
        </footer>
      </div>
    </div>
  );
}
