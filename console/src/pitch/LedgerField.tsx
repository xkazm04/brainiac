"use client";

/*
 * The hero field: the ledger.
 *
 * Won the hero prototype round against a wave-interference variant and the
 * original review-gate queue. Metaphor: attestation. It argues in structure
 * rather than physics, which is the thing an engineer actually gets: an
 * append-only record where every entry carries a signature and a source.
 *
 * Raw claims drift in the upper field, translucent and unattested. Signing one
 * drops it onto the chain, where it seals into a solid block: a provenance
 * stamp (who asserted it, from which session, with which model) and a link to
 * the block below it. Contested claims are struck through and can never dock —
 * they drift until they decay.
 *
 * The chain grows downward and scrolls, so the hero reads as a record being
 * WRITTEN, not a dashboard being displayed.
 *
 * Motion policy (theme.ts): ambient canvas motion on hero surfaces only, behind
 * a reduced-motion static frame.
 */

import { useCallback, useEffect, useRef } from "react";
import { useReducedMotion } from "framer-motion";

import type { HeroFieldProps } from "./hero-types";

type Phase = "floating" | "docking" | "sealed" | "struck";

interface Claim {
  /** Normalised float position (drifting) or dock target (sealed). */
  x: number;
  y: number;
  tx: number;
  ty: number;
  phase: Phase;
  contested: boolean;
  seed: number;
  /** 0→1 seal animation once docked. */
  seal: number;
  amp: number;
  /** Index in the chain once sealed. */
  slot: number;
}

const COUNT = 14;
const CHAIN_X = 0.63;
const ROW_H = 0.118;
const VISIBLE_ROWS = 6;

export default function LedgerField({ onStats, onApproveRef }: HeroFieldProps) {
  const reduce = !!useReducedMotion();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const claimsRef = useRef<Claim[]>([]);
  const chainRef = useRef({ length: 0, scroll: 0 });
  const flashRef = useRef(0);

  const approve = useCallback(() => {
    const claims = claimsRef.current;
    const next = claims.find((c) => c.phase === "floating" && !c.contested);
    if (next) {
      const chain = chainRef.current;
      next.phase = "docking";
      next.slot = chain.length;
      chain.length += 1;
      flashRef.current = 1;
      return;
    }
    const bad = claims.find((c) => c.phase === "floating" && c.contested);
    if (bad) bad.phase = "struck";
  }, []);

  useEffect(() => {
    onApproveRef?.(approve);
  }, [approve, onApproveRef]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const rand = (n: number) => {
      const s = Math.sin(n * 57.31) * 43758.5453;
      return s - Math.floor(s);
    };

    claimsRef.current = Array.from({ length: COUNT }, (_, i) => ({
      x: 0.06 + rand(i + 3) * 0.42,
      y: 0.10 + rand(i + 11) * 0.78,
      tx: 0,
      ty: 0,
      phase: "floating" as Phase,
      contested: rand(i + 29) < 0.16,
      seed: i,
      seal: 0,
      amp: 1,
      slot: -1,
    }));
    chainRef.current = { length: 0, scroll: 0 };

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

    /** Rounded rect — the block shape. */
    const rrect = (x: number, y: number, w: number, h: number, r: number) => {
      ctx.beginPath();
      ctx.moveTo(x + r, y);
      ctx.arcTo(x + w, y, x + w, y + h, r);
      ctx.arcTo(x + w, y + h, x, y + h, r);
      ctx.arcTo(x, y + h, x, y, r);
      ctx.arcTo(x, y, x + w, y, r);
      ctx.closePath();
    };

    const draw = (now: number) => {
      const rect = canvas.getBoundingClientRect();
      const W = rect.width;
      const H = rect.height;
      const t = now / 1000;
      ctx.clearRect(0, 0, W, H);

      const claims = claimsRef.current;
      const chain = chainRef.current;

      // The chain scrolls so the newest block always sits in view.
      const targetScroll = Math.max(0, chain.length - VISIBLE_ROWS) * ROW_H;
      chain.scroll += (targetScroll - chain.scroll) * 0.06;

      const rowY = (slot: number) => 0.16 + slot * ROW_H - chain.scroll;

      let queued = 0;
      let sealed = 0;

      // ── the spine: the chain's backbone ──────────────────────────────────
      const sealedClaims = claims.filter((c) => c.phase === "sealed" || c.phase === "docking");
      if (sealedClaims.length > 0) {
        ctx.strokeStyle = "hsla(46,90%,66%,0.30)";
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(CHAIN_X * W, rowY(0) * H);
        ctx.lineTo(CHAIN_X * W, rowY(Math.max(0, chain.length - 1)) * H);
        ctx.stroke();
      }

      for (const c of claims) {
        switch (c.phase) {
          case "floating": {
            queued++;
            // Unattested: it drifts. Nothing is holding it anywhere.
            c.x += Math.sin(t * 0.4 + c.seed * 1.7) * 0.00035;
            c.y += Math.cos(t * 0.31 + c.seed) * 0.0004;
            break;
          }
          case "docking": {
            c.tx = CHAIN_X;
            c.ty = rowY(c.slot);
            c.x += (c.tx - c.x) * 0.09;
            c.y += (c.ty - c.y) * 0.09;
            if (Math.abs(c.tx - c.x) < 0.004 && Math.abs(c.ty - c.y) < 0.004) {
              c.phase = "sealed";
            }
            break;
          }
          case "sealed": {
            sealed++;
            c.seal = Math.min(1, c.seal + 0.05);
            c.x += (CHAIN_X - c.x) * 0.2;
            c.y += (rowY(c.slot) - c.y) * 0.12;
            break;
          }
          case "struck": {
            c.amp += (0 - c.amp) * 0.03;
            c.x -= 0.0012;
            c.y += 0.0006;
            break;
          }
        }

        if (c.amp < 0.02) continue;

        const x = c.x * W;
        const y = c.y * H;
        const isBlock = c.phase === "sealed" || c.phase === "docking";
        const w = isBlock ? 200 : 150;
        const h = isBlock ? 42 : 34;

        ctx.globalAlpha = c.amp;

        if (isBlock) {
          // A sealed block: solid, gold-bordered, glowing on the seam.
          const s = c.seal;
          rrect(x - w / 2, y - h / 2, w, h, 6);
          ctx.fillStyle = `hsla(46,90%,60%,${0.05 + s * 0.06})`;
          ctx.fill();
          ctx.strokeStyle = `hsla(46,90%,68%,${0.35 + s * 0.45})`;
          ctx.lineWidth = 1.2;
          ctx.shadowColor = "hsla(46,90%,60%,0.45)";
          ctx.shadowBlur = 10 * s;
          ctx.stroke();
          ctx.shadowBlur = 0;

          // The content bars: a claim, redacted to its shape.
          ctx.fillStyle = `hsla(46,60%,80%,${0.30 + s * 0.35})`;
          ctx.fillRect(x - w / 2 + 12, y - 11, (w - 46) * (0.55 + 0.35 * rand(c.seed)), 3);
          ctx.fillRect(x - w / 2 + 12, y - 3, (w - 46) * (0.35 + 0.4 * rand(c.seed + 5)), 3);

          // The provenance stamp: the whole point of the metaphor.
          ctx.fillStyle = `hsla(46,92%,72%,${0.55 + s * 0.4})`;
          for (let i = 0; i < 4; i++) {
            ctx.fillRect(x - w / 2 + 12 + i * 7, y + 8, 4, 4);
          }
          ctx.fillStyle = `rgba(233,237,255,${0.22 + s * 0.18})`;
          ctx.fillRect(x - w / 2 + 46, y + 9, w - 90, 2);

          // The seal: a filled dot on the chain seam.
          ctx.fillStyle = "hsl(46,92%,74%)";
          ctx.shadowColor = "hsla(46,90%,60%,0.8)";
          ctx.shadowBlur = 12 * s;
          ctx.beginPath();
          ctx.arc(x + w / 2, y, 3.2 * (0.5 + s * 0.5), 0, Math.PI * 2);
          ctx.fill();
          ctx.shadowBlur = 0;
        } else {
          // Unattested: dashed, translucent, weightless.
          rrect(x - w / 2, y - h / 2, w, h, 6);
          ctx.setLineDash(c.contested ? [2, 3] : [4, 5]);
          ctx.strokeStyle = c.contested
            ? "rgba(255,93,162,0.6)"
            : "rgba(200,210,245,0.35)";
          ctx.lineWidth = 1;
          ctx.stroke();
          ctx.setLineDash([]);

          ctx.fillStyle = c.contested
            ? "rgba(255,93,162,0.30)"
            : "rgba(200,210,245,0.22)";
          ctx.fillRect(x - w / 2 + 10, y - 6, (w - 32) * (0.5 + 0.4 * rand(c.seed + 2)), 2.5);
          ctx.fillRect(x - w / 2 + 10, y + 1, (w - 32) * (0.3 + 0.4 * rand(c.seed + 8)), 2.5);

          if (c.contested || c.phase === "struck") {
            // Struck through — this one is contested and cannot be sealed.
            ctx.strokeStyle = "rgba(255,93,162,0.75)";
            ctx.lineWidth = 1.2;
            ctx.beginPath();
            ctx.moveTo(x - w / 2 + 8, y + h / 2 - 6);
            ctx.lineTo(x + w / 2 - 8, y - h / 2 + 6);
            ctx.stroke();
          }
        }

        ctx.globalAlpha = 1;
      }

      if (flashRef.current > 0) {
        flashRef.current = Math.max(0, flashRef.current - 0.025);
        ctx.fillStyle = `hsla(46,90%,70%,${flashRef.current * 0.05})`;
        ctx.fillRect(0, 0, W, H);
      }

      onStats?.({ queued, canonical: sealed });
      if (!reduce && !disposed) raf = requestAnimationFrame(draw);
    };

    const ro = new ResizeObserver(() => {
      resize();
      // Repaint here ONLY when no RAF chain is running. `draw` reschedules itself
      // at its tail, so calling it while the chain is live spawns a SECOND chain:
      // it overwrites the shared `raf` handle (leaking the previous id, which
      // cleanup can then never cancel) and the shared physics — drift, docking
      // lerp, chain scroll — advances once per chain per frame. ResizeObserver
      // fires one callback immediately on observe(), so this doubled the hero's
      // animation speed from mount, before any user interaction, and added
      // another chain per resize.
      //
      // Under reduced motion there IS no chain (draw ran once and did not
      // reschedule), so a resize would otherwise clear the canvas and never
      // repaint it — the hero would just go blank. Draw exactly once; the tail's
      // `!reduce` guard means it still won't reschedule.
      if (reduce && !disposed) draw(performance.now());
    });
    ro.observe(canvas);

    // Pre-seed the chain. An empty ledger is a void, and the hero would open on
    // nothing; the record should already be visibly under way when you arrive.
    for (let i = 0; i < 3; i++) approve();
    for (const c of claimsRef.current) {
      if (c.phase === "docking") {
        c.phase = "sealed";
        c.seal = 1;
        c.x = CHAIN_X;
        c.y = 0.16 + c.slot * ROW_H;
      }
    }

    if (reduce) {
      draw(0);
    } else {
      raf = requestAnimationFrame(draw);
    }

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [reduce, onStats, approve]);

  return (
    <canvas
      ref={canvasRef}
      className="h-full w-full"
      role="img"
      aria-label="Raw claims drift unattested on the left. Signing one drops it onto an append-only chain where it seals into a block carrying a provenance stamp. Contested claims are struck through and never dock."
    />
  );
}
