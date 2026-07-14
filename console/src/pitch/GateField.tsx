"use client";

/*
 * The hero field: the review gate, made literal.
 *
 * Raw extractions stream in from the left. They pile up against the gate —
 * because in Brainiac nothing becomes org truth on its own. When a maintainer
 * signs (the button, or the ambient approvals that stand in for maintainers
 * working the queue), a batch passes and crystallises into canonical gold on
 * the right. Poisoned claims (magenta) are refused at the gate and bounce back,
 * however hard they push.
 *
 * This is the same physical-metaphor idiom as the console home's interference
 * field (src/home/Home.tsx) — the brand is "knowledge as a wave" — but it
 * carries the pitch's argument rather than the product's: the gate IS the
 * differentiator, so the gate is the art.
 *
 * Two properties the first draft got wrong and that matter:
 *   - The sim is PRE-WARMED before first paint, so the hero is never an empty
 *     void while particles fly in from off-screen.
 *   - Canonical nodes are RECYCLED past a cap, so the stream reaches a steady
 *     state instead of silting up and stopping.
 *
 * Reduced motion: renders one representative static frame — full queue, lit
 * gate, populated lattice — and never starts the loop.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useReducedMotion } from "framer-motion";

type Phase = "approach" | "queued" | "passing" | "canonical" | "refused";

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  phase: Phase;
  poison: boolean;
  hx: number;
  hy: number;
  seed: number;
  /** Monotonic stamp of when this became canonical — drives recycling. */
  settled: number;
  /** Reserved lattice slot, or -1. Reserved at the gate, held until recycled. */
  slotIdx: number;
}

const COUNT = 165;
const POISON_RATE = 0.07;
const GATE_X = 0.5;
const CANONICAL_CAP = 40;
/** Ambient approvals: maintainers working the queue, so the field breathes. */
const AMBIENT_APPROVE_EVERY = 4.2;

/**
 * The pending pool: queued memories don't stack into a single column, they
 * crowd in a shallow band in front of the gate. Visually it reads as a review
 * backlog; numerically it keeps the inbound stream from starving, which the
 * first version did (everything ended up queued and the left half went empty).
 */
const POOL_X0 = 0.375;
const POOL_X1 = 0.468;

export default function GateField({
  onQueueChange,
  onApproveRef,
}: {
  onQueueChange?: (queued: number, canonical: number) => void;
  onApproveRef?: (fn: () => void) => void;
}) {
  const reduce = !!useReducedMotion();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const gateRef = useRef({ open: 0, flash: 0, sinceApprove: 0 });
  const approve = useCallback(() => {
    gateRef.current.open = 1;
    gateRef.current.flash = 1;
    gateRef.current.sinceApprove = 0;
  }, []);

  useEffect(() => {
    onApproveRef?.(approve);
  }, [approve, onApproveRef]);

  const [, forceFrame] = useState(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    let disposed = false;
    let last = 0;
    let clock = 0;

    // Deterministic PRNG: identical field on server-adjacent renders and across
    // reloads, so the hero has a stable composition rather than a random one.
    const rand = (seed: number) => {
      const s = Math.sin(seed * 12.9898) * 43758.5453;
      return s - Math.floor(s);
    };

    const spawn = (p: Particle, i: number, salt: number) => {
      p.x = -0.02 - rand(i + salt) * 0.5;
      p.y = 0.1 + rand(i + salt + 2.7) * 0.8;
      p.vx = 0.05 + rand(i + salt + 9.1) * 0.07;
      p.vy = 0;
      p.phase = "approach";
      p.poison = rand(i + salt + 41.7) < POISON_RATE;
      p.settled = 0;
      p.slotIdx = -1;
    };

    const parts: Particle[] = Array.from({ length: COUNT }, (_, i) => {
      const p: Particle = {
        x: 0, y: 0, vx: 0, vy: 0,
        phase: "approach", poison: false, hx: 0, hy: 0, seed: i, settled: 0,
        slotIdx: -1,
      };
      spawn(p, i, 1);
      return p;
    });

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();

    // Assigning canvas.width CLEARS the bitmap, and the observer's first
    // callback lands after this effect's initial draw. Without redrawing here,
    // the reduced-motion path (which paints one frame and never loops) is wiped
    // to blank by that first callback — an empty hero for exactly the users who
    // asked for less motion, not fewer pixels.
    const ro = new ResizeObserver(() => {
      resize();
      render();
    });
    ro.observe(canvas);

    /**
     * Lattice slots are RESERVED at the gate and held until the particle is
     * recycled. Deriving the slot from a live count instead (the first version
     * of this) hands the same slot to every particle currently in flight, and
     * the canonical side clumps into a few overlapping blobs.
     */
    const occupied: (Particle | null)[] = Array.from({ length: CANONICAL_CAP }, () => null);

    // Jittered, not gridded: a perfect lattice reads as a spreadsheet. The
    // offsets are deterministic per slot, so the graph has a stable shape.
    const slotXY = (slot: number) => {
      const col = slot % 8;
      const row = Math.floor(slot / 8);
      const jx = (rand(slot * 3.7) - 0.5) * 0.028;
      const jy = (rand(slot * 9.4 + 5) - 0.5) * 0.07;
      return { hx: 0.56 + col * 0.052 + jx, hy: 0.17 + row * 0.16 + jy };
    };

    const reserveSlot = (p: Particle): boolean => {
      for (let i = 0; i < CANONICAL_CAP; i++) {
        if (!occupied[i]) {
          occupied[i] = p;
          p.slotIdx = i;
          const { hx, hy } = slotXY(i);
          p.hx = hx;
          p.hy = hy;
          return true;
        }
      }
      return false;
    };

    const release = (p: Particle) => {
      if (p.slotIdx >= 0 && occupied[p.slotIdx] === p) occupied[p.slotIdx] = null;
      p.slotIdx = -1;
    };

    const recycle = (p: Particle) => {
      release(p);
      spawn(p, p.seed, clock);
    };

    const step = (dt: number) => {
      clock += dt;
      const gate = gateRef.current;
      gate.open = Math.max(0, gate.open - dt * 0.85);
      gate.flash = Math.max(0, gate.flash - dt * 1.8);
      gate.sinceApprove += dt;

      // Maintainers work the queue even when nobody is clicking.
      if (gate.sinceApprove > AMBIENT_APPROVE_EVERY) {
        gate.open = 1;
        gate.sinceApprove = 0;
      }

      // Retire the oldest canonical node when the lattice is full, so the
      // stream reaches a steady state instead of silting up and stopping.
      let settledCount = 0;
      for (const p of parts) if (p.phase === "canonical") settledCount++;
      if (settledCount >= CANONICAL_CAP) {
        let oldest: Particle | null = null;
        for (const p of parts) {
          if (p.phase === "canonical" && (!oldest || p.settled < oldest.settled)) oldest = p;
        }
        if (oldest) recycle(oldest);
      }

      for (const p of parts) {
        switch (p.phase) {
          case "approach":
            p.x += p.vx * dt;
            if (p.x >= POOL_X0 + rand(p.seed + 13) * (POOL_X1 - POOL_X0)) {
              p.phase = "queued";
              // Remember where in the pool this one settled.
              p.hx = p.x;
              p.hy = p.y;
            }
            break;

          case "queued":
            // Restless in the pool: this is a review backlog, visibly waiting.
            p.x = p.hx + Math.sin(clock * 0.9 + p.seed * 2) * 0.004;
            p.y = p.hy + Math.sin(clock * 1.4 + p.seed) * 0.006;
            if (gate.open > 0.4) {
              if (p.poison) {
                // Refused at the gate, however good its provenance looks.
                p.phase = "refused";
                p.vx = -0.13 - rand(p.seed) * 0.07;
                p.vy = (rand(p.seed + 3) - 0.5) * 0.12;
              } else if (reserveSlot(p)) {
                // Signed. It has a home in the canonical lattice.
                p.phase = "passing";
                p.vx = 0.17 + rand(p.seed + 5) * 0.09;
              }
              // No free slot → it stays queued. The gate does not overflow.
            }
            break;

          case "passing": {
            p.x += p.vx * dt;
            p.y += (p.hy - p.y) * dt * 2.6;
            if (p.x >= p.hx) {
              p.phase = "canonical";
              p.x = p.hx;
              p.settled = clock;
            }
            break;
          }

          case "canonical":
            p.x += (p.hx - p.x) * dt * 4;
            p.y += (p.hy - p.y) * dt * 4;
            break;

          case "refused":
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += 0.03 * dt;
            if (p.x < -0.08 || p.y < 0 || p.y > 1) recycle(p);
            break;
        }
      }
    };

    const render = () => {
      const rect = canvas.getBoundingClientRect();
      const W = rect.width;
      const H = rect.height;
      ctx.clearRect(0, 0, W, H);
      const gate = gateRef.current;
      const gx = GATE_X * W;
      const open = gate.open;

      // The gate: a vertical seam. Dashed and dim when shut; solid and lit when
      // a maintainer signs.
      const grad = ctx.createLinearGradient(gx - 40, 0, gx + 40, 0);
      grad.addColorStop(0, "rgba(233,237,255,0)");
      grad.addColorStop(0.5, `hsla(46, 90%, 62%, ${0.06 + open * 0.3})`);
      grad.addColorStop(1, "rgba(233,237,255,0)");
      ctx.fillStyle = grad;
      ctx.fillRect(gx - 40, 0, 80, H);

      ctx.strokeStyle = `hsla(46, 90%, 68%, ${0.3 + open * 0.6})`;
      ctx.lineWidth = 1 + open * 1.5;
      ctx.setLineDash(open > 0.4 ? [] : [4, 8]);
      ctx.beginPath();
      ctx.moveTo(gx, 10);
      ctx.lineTo(gx, H - 10);
      ctx.stroke();
      ctx.setLineDash([]);

      // Bind the canonical lattice: gold edges between settled neighbours.
      // Gamma is the binding band — canonical knowledge is not a pile of facts,
      // it is a graph, and the art should say so.
      ctx.lineWidth = 1;
      for (let i = 0; i < CANONICAL_CAP; i++) {
        const a = occupied[i];
        if (!a || a.phase !== "canonical") continue;
        const col = i % 8;
        // Not every neighbour pair is bound — a knowledge graph is sparse and
        // irregular, and drawing every edge just reproduces the grid.
        for (const j of [col < 7 ? i + 1 : -1, i + 8, col < 7 ? i + 9 : -1]) {
          if (j < 0 || j >= CANONICAL_CAP) continue;
          const b = occupied[j];
          if (!b || b.phase !== "canonical") continue;
          if (rand(i * 17.3 + j * 2.9) > 0.62) continue;
          ctx.strokeStyle = "hsla(46, 90%, 66%, 0.20)";
          ctx.beginPath();
          ctx.moveTo(a.x * W, a.y * H);
          ctx.lineTo(b.x * W, b.y * H);
          ctx.stroke();
        }
      }

      for (const p of parts) {
        const x = p.x * W;
        const y = p.y * H;
        if (x < -30 || x > W + 30) continue;

        switch (p.phase) {
          case "canonical":
            ctx.fillStyle = "hsla(46, 92%, 76%, 1)";
            ctx.shadowColor = "hsla(46, 90%, 60%, 0.75)";
            ctx.shadowBlur = 14;
            ctx.fillRect(x - 3, y - 3, 6, 6);
            ctx.shadowBlur = 0;
            break;
          case "passing":
            ctx.fillStyle = "hsla(46, 90%, 70%, 0.9)";
            ctx.shadowColor = "hsla(46, 90%, 60%, 0.5)";
            ctx.shadowBlur = 6;
            ctx.fillRect(x - 1.8, y - 1.8, 3.6, 3.6);
            ctx.shadowBlur = 0;
            break;
          case "refused":
            ctx.fillStyle = "rgba(255, 93, 162, 0.9)";
            ctx.shadowColor = "rgba(255,93,162,0.5)";
            ctx.shadowBlur = 6;
            ctx.fillRect(x - 1.8, y - 1.8, 3.6, 3.6);
            ctx.shadowBlur = 0;
            break;
          case "queued":
            // Waiting for a human. Restless, dim, unmistakably not yet truth.
            ctx.fillStyle = p.poison ? "rgba(255, 93, 162, 0.55)" : "rgba(233,237,255,0.5)";
            ctx.fillRect(x - 1.7, y - 1.7, 3.4, 3.4);
            break;
          default:
            ctx.fillStyle = "rgba(190, 200, 240, 0.42)";
            ctx.fillRect(x - 1.3, y - 1.3, 2.6, 2.6);
            ctx.strokeStyle = "rgba(190, 200, 240, 0.13)";
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(x - 12, y);
            ctx.lineTo(x - 3, y);
            ctx.stroke();
            break;
        }
      }

      if (gate.flash > 0) {
        ctx.fillStyle = `hsla(46, 90%, 70%, ${gate.flash * 0.08})`;
        ctx.fillRect(0, 0, W, H);
      }
    };

    const report = () => {
      if (!onQueueChange) return;
      let q = 0;
      let c = 0;
      for (const p of parts) {
        if (p.phase === "queued") q++;
        else if (p.phase === "canonical") c++;
      }
      onQueueChange(q, c);
    };

    // Pre-warm: reach a populated steady state BEFORE the first paint, so the
    // hero never opens on an empty field.
    for (let i = 0; i < 900; i++) step(1 / 60);

    render();
    report();
    forceFrame((n) => n + 1);

    if (reduce) {
      gateRef.current.open = 0.55;
      render();
      return () => {
        disposed = true;
        ro.disconnect();
      };
    }

    const loop = (now: number) => {
      if (disposed) return;
      const dt = Math.min(0.05, last ? (now - last) / 1000 : 0.016);
      last = now;
      step(dt);
      render();
      report();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [reduce, onQueueChange, onApproveRef]);

  return (
    <canvas
      ref={canvasRef}
      className="h-full w-full"
      role="img"
      aria-label="Raw extracted memories stream toward a review gate and queue against it. When a maintainer approves, safe memories pass and crystallise into canonical gold; poisoned claims are refused at the gate and bounce back."
    />
  );
}
