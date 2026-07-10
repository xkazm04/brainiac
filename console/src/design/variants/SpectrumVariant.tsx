"use client";

/*
 * Variant E — "Spectrum". The org mind runs on five bands, and the page is
 * the tuner. One continuous drag — delta to gamma — morphs the wave, the
 * light and the product chapter in lockstep. Gamma is the payoff: in
 * neuroscience gamma oscillations BIND distributed representations into one
 * percept; in Brainiac the canonical graph binds three teams' dialects into
 * one entity. The metaphor is literal, not decorative.
 * Fixed art direction → literal hexes.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";

import { CANONICAL_DEMO, CONTRADICTION, QUEUE, STRATA } from "../demo-data";

const DISPLAY = "font-[family-name:var(--font-synapse-display)]";
const MONO = "font-[family-name:var(--font-synapse-mono)]";

const BG = "#08070c";

interface Band {
  key: string;
  greek: string;
  hz: string;
  name: string;
  title: string;
  body: string;
  artifact: { label: string; text: string };
  // wave params
  cycles: number;
  amp: number;
  hue: number; // hsl hue for the band's light
}

const BANDS: Band[] = [
  {
    key: "delta",
    greek: "δ",
    hz: "0.5–4 Hz",
    name: "deep archive",
    title: "Slow waves keep the record.",
    body: "Nothing is overwritten. Superseded memories keep their validity window, so “what did we know in March?” is a query, not an argument.",
    artifact: { label: "as-of query", text: "psp-gateway timeout @ 2026-04-01 → 10s (deprecated 2026-05-01, superseded)" },
    cycles: 1.4,
    amp: 46,
    hue: 262,
  },
  {
    key: "theta",
    greek: "θ",
    hz: "4–8 Hz",
    name: "reflection",
    title: "Consolidation happens off-peak.",
    body: "The pipeline replays each new memory against its neighbors — entity overlap, vector proximity — and flags what conflicts. Like sleep, but auditable.",
    artifact: { label: "contradiction #114", text: `${CONTRADICTION.a}  ⇄  ${CONTRADICTION.b}` },
    cycles: 2.6,
    amp: 38,
    hue: 224,
  },
  {
    key: "alpha",
    greek: "α",
    hz: "8–12 Hz",
    name: "calm governance",
    title: "Idle is a feature.",
    body: "Promotion is deliberate: a maintainer of the owning team signs every canonical claim. The review queue stays calm because policy auto-handles the obvious.",
    artifact: { label: "awaiting review", text: QUEUE[1].content },
    cycles: 4.2,
    amp: 30,
    hue: 190,
  },
  {
    key: "beta",
    greek: "β",
    hz: "12–35 Hz",
    name: "active recall",
    title: "Recall at working speed.",
    body: "Hybrid retrieval — vectors, full-text, graph hops, recency — under your RLS. NDCG@10 0.876 on the Meridian benchmark, semantic stratum 0.81.",
    artifact: { label: "eval · text-embedding-v4", text: STRATA.map((s) => `${s.name} ${s.qwen.toFixed(2)}`).join(" · ") },
    cycles: 7.5,
    amp: 22,
    hue: 158,
  },
  {
    key: "gamma",
    greek: "γ",
    hz: "35+ Hz",
    name: "binding",
    title: "Gamma binds the org into one mind.",
    body: "Three teams say Kafka, MSK cluster, the event bus. Gamma-band binding links them to one canonical node — so payments' pitfall reaches data's analyst mid-task.",
    artifact: {
      label: "canonical binding",
      text: `${CANONICAL_DEMO.name} ⇐ ${CANONICAL_DEMO.aliases.map((a) => `${a.team}:“${a.name}”`).join("  ")}`,
    },
    cycles: 13,
    amp: 15,
    hue: 46,
  },
];

const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

function bandAt(tune: number) {
  const i = Math.min(BANDS.length - 2, Math.floor(tune));
  const t = tune - i;
  return {
    cycles: lerp(BANDS[i].cycles, BANDS[i + 1].cycles, t),
    amp: lerp(BANDS[i].amp, BANDS[i + 1].amp, t),
    hue: lerp(BANDS[i].hue, BANDS[i + 1].hue, t),
  };
}

export default function SpectrumVariant() {
  const reduce = !!useReducedMotion();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tuneRef = useRef(4); // start on gamma — the brand chapter
  const [tune, setTune] = useState(4);
  const [sweeping, setSweeping] = useState(false);
  const sweepRef = useRef(false);

  const setTuneBoth = useCallback((v: number) => {
    const clamped = Math.min(4, Math.max(0, v));
    tuneRef.current = clamped;
    setTune(clamped);
  }, []);

  // wave loop
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
      if (sweepRef.current) {
        const next = tuneRef.current + 0.006;
        if (next >= 4) {
          sweepRef.current = false;
          setSweeping(false);
          setTuneBoth(4);
        } else {
          setTuneBoth(next);
        }
      }
      const rect = canvas.getBoundingClientRect();
      const W = rect.width;
      const H = rect.height;
      const mid = H / 2;
      const t = now / 1000;
      const { cycles, amp, hue } = bandAt(tuneRef.current);

      ctx.clearRect(0, 0, W, H);

      // three layered traces of the same wave — past, present, glow
      for (let layer = 2; layer >= 0; layer--) {
        ctx.beginPath();
        const phase = t * (1.4 + cycles * 0.24) - layer * 0.35;
        for (let px = 0; px <= W; px += 2) {
          const u = px / W;
          const env = Math.sin(u * Math.PI); // fade at edges
          const y =
            mid +
            env *
              (Math.sin(u * Math.PI * 2 * cycles + phase) * amp +
                Math.sin(u * Math.PI * 2 * cycles * 2 + phase * 1.7) * amp * 0.22);
          if (px === 0) ctx.moveTo(px, y);
          else ctx.lineTo(px, y);
        }
        const alpha = layer === 0 ? 0.95 : layer === 1 ? 0.28 : 0.12;
        ctx.strokeStyle = `hsla(${hue}, 90%, ${layer === 0 ? 72 : 60}%, ${alpha})`;
        ctx.lineWidth = layer === 0 ? 1.8 : 1;
        if (layer === 0 && !reduce) {
          ctx.shadowColor = `hsla(${hue}, 90%, 60%, 0.7)`;
          ctx.shadowBlur = 16;
        }
        ctx.stroke();
        ctx.shadowBlur = 0;
      }

      // frequency ruler ticks
      ctx.fillStyle = "rgba(255,255,255,0.14)";
      for (let px = 0; px < W; px += W / 40) {
        ctx.fillRect(px, H - 8, 1, 8);
      }

      if (!reduce) raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [reduce, setTuneBoth]);

  // dial pointer handling
  const trackRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const tuneFromPointer = useCallback(
    (clientX: number) => {
      const track = trackRef.current;
      if (!track) return;
      const rect = track.getBoundingClientRect();
      setTuneBoth(((clientX - rect.left) / rect.width) * 4);
    },
    [setTuneBoth],
  );

  const active = BANDS[Math.round(tune)];
  const { hue } = bandAt(tune);
  const glow = `hsla(${hue}, 90%, 60%, 0.35)`;
  const bandColor = `hsl(${hue}, 90%, 68%)`;
  const approxHz = (0.5 * Math.pow(2, tune * 1.66)).toFixed(1);

  return (
    <div className={`${DISPLAY} flex min-h-screen flex-col text-white`} style={{ background: BG }}>
      {/* header */}
      <header className="mx-auto flex w-full max-w-7xl items-center justify-between px-6 py-5">
        <div className="flex items-center gap-3">
          <span className="text-xl font-semibold tracking-tight">Brainiac</span>
          <span className={`${MONO} text-[10px] uppercase tracking-[0.22em]`} style={{ color: bandColor }}>
            spectrum console
          </span>
        </div>
        <nav className={`${MONO} flex items-center gap-5 text-xs uppercase tracking-widest text-white/40`}>
          <span className="cursor-pointer transition hover:text-white">reviews</span>
          <span className="cursor-pointer transition hover:text-white">graph</span>
          <span className="cursor-pointer transition hover:text-white">analytics</span>
        </nav>
      </header>

      {/* readout */}
      <section className="mx-auto w-full max-w-7xl px-6 pt-4">
        <div className="flex flex-wrap items-end justify-between gap-6">
          <div>
            <div className={`${MONO} text-[11px] uppercase tracking-[0.24em] text-white/40`}>
              now tuned to
            </div>
            <div className="mt-1 flex items-baseline gap-4">
              <span className="text-7xl font-semibold leading-none tracking-tight lg:text-8xl" style={{ color: bandColor, textShadow: `0 0 42px ${glow}` }}>
                {active.greek}
              </span>
              <div>
                <div className="text-2xl font-medium tracking-tight">{active.name}</div>
                <div className={`${MONO} mt-0.5 text-xs text-white/45`}>
                  {active.hz} · reading {approxHz} Hz
                </div>
              </div>
            </div>
          </div>
          <div className="max-w-md pb-1 text-right">
            <p className={`${MONO} text-xs leading-relaxed text-white/40`}>
              One organization. Five bands. Drag the dial — the instrument, the light
              and the story tune together.
            </p>
          </div>
        </div>
      </section>

      {/* the wave */}
      <section className="relative mx-auto mt-2 w-full max-w-7xl flex-1 px-6">
        <canvas
          ref={canvasRef}
          className="h-[34vh] min-h-[220px] w-full"
          role="img"
          aria-label={`Live waveform tuned to the ${active.name} band`}
        />
        {/* dial */}
        <div className="mt-1 select-none">
          <div
            ref={trackRef}
            role="slider"
            tabIndex={0}
            aria-label="Frequency band tuner"
            aria-valuemin={0}
            aria-valuemax={4}
            aria-valuenow={Math.round(tune * 100) / 100}
            aria-valuetext={`${active.name} band`}
            className="group relative h-14 cursor-ew-resize touch-none"
            onPointerDown={(e) => {
              dragging.current = true;
              (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
              tuneFromPointer(e.clientX);
            }}
            onPointerMove={(e) => dragging.current && tuneFromPointer(e.clientX)}
            onPointerUp={() => (dragging.current = false)}
            onKeyDown={(e) => {
              if (e.key === "ArrowRight") setTuneBoth(tuneRef.current + 0.25);
              if (e.key === "ArrowLeft") setTuneBoth(tuneRef.current - 0.25);
            }}
          >
            {/* track */}
            <div className="absolute left-0 right-0 top-1/2 h-px -translate-y-1/2 bg-white/15" />
            {/* band stops */}
            {BANDS.map((b, i) => (
              <button
                key={b.key}
                onClick={() => setTuneBoth(i)}
                className={`${MONO} absolute top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full px-2 py-4 text-sm transition`}
                style={{ left: `${(i / 4) * 100}%`, color: Math.round(tune) === i ? bandColor : "rgba(255,255,255,0.35)" }}
                aria-label={`Tune to ${b.name}`}
              >
                {b.greek}
              </button>
            ))}
            {/* thumb */}
            <div
              className="pointer-events-none absolute top-1/2 h-9 w-9 -translate-x-1/2 -translate-y-1/2 rounded-full border"
              style={{
                left: `${(tune / 4) * 100}%`,
                borderColor: bandColor,
                boxShadow: `0 0 24px ${glow}, inset 0 0 10px ${glow}`,
                transition: dragging.current ? "none" : "left 0.25s ease",
              }}
            >
              <span className="absolute left-1/2 top-1/2 h-1.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full" style={{ background: bandColor }} />
            </div>
          </div>
          <div className={`${MONO} flex items-center justify-between text-[10px] uppercase tracking-widest text-white/30`}>
            <span>← slower · the archive</span>
            <button
              onClick={() => {
                if (sweeping) {
                  sweepRef.current = false;
                  setSweeping(false);
                } else {
                  setTuneBoth(0);
                  sweepRef.current = true;
                  setSweeping(true);
                }
              }}
              className="rounded border border-white/15 px-3 py-1 uppercase tracking-widest text-white/50 transition hover:border-white/40 hover:text-white"
            >
              {sweeping ? "■ stop sweep" : "▶ auto-sweep δ→γ"}
            </button>
            <span>faster · the insight →</span>
          </div>
        </div>
      </section>

      {/* chapter */}
      <section className="mx-auto w-full max-w-7xl px-6 pb-10 pt-8">
        <AnimatePresence mode="wait">
          <motion.div
            key={active.key}
            initial={{ opacity: 0, y: 14 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.3 }}
            className="grid gap-8 border-t border-white/10 pt-8 lg:grid-cols-[1fr_1fr]"
          >
            <div>
              <h2 className="text-4xl font-semibold leading-tight tracking-tight">{active.title}</h2>
              <p className={`${MONO} mt-4 max-w-md text-sm leading-relaxed text-white/55`}>{active.body}</p>
            </div>
            <div className="self-center">
              <div
                className="rounded-xl border p-5"
                style={{ borderColor: `hsla(${hue}, 90%, 68%, 0.3)`, background: `hsla(${hue}, 90%, 60%, 0.05)` }}
              >
                <div className={`${MONO} text-[10px] uppercase tracking-[0.2em]`} style={{ color: bandColor }}>
                  {active.artifact.label}
                </div>
                <p className={`${MONO} mt-2 text-sm leading-relaxed text-white/80`}>{active.artifact.text}</p>
              </div>
            </div>
          </motion.div>
        </AnimatePresence>
      </section>

      <footer className={`${MONO} mx-auto w-full max-w-7xl px-6 pb-6`}>
        <div className="flex items-center justify-between border-t border-white/10 pt-5 text-[10px] uppercase tracking-widest text-white/30">
          <span>brainiac · tuned to your organization</span>
          <span style={{ color: bandColor }}>γ binds · 0 leaks on every band</span>
        </div>
      </footer>
    </div>
  );
}
