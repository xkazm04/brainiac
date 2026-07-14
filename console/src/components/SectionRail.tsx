"use client";

/*
 * The sticky section rail — a nav that IS the progress line rather than sitting
 * above one. Extracted from the pitch so the public long-form pages (/pitch,
 * /kb) share one navigation behaviour instead of two drifting copies.
 *
 * Three things it has to get right (unchanged from the original):
 *
 * 1. ALWAYS ACCESSIBLE. It sticks to the top, so the reader can leave and
 *    re-enter the argument at any point without scrolling back to a menu.
 *
 * 2. YOU ARE HERE. A scroll-spy marks the section in view — the section
 *    occupying the most viewport, not merely the first one intersecting, which
 *    flickers between neighbours on a scroll.
 *
 * 3. NO TELEPORT. Clicking scrolls — it does not jump. A hash jump drops you
 *    somewhere with no sense of direction or distance. Honouring
 *    prefers-reduced-motion, we jump for the people who asked for it.
 */

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { motion, useReducedMotion, useScroll, useSpring } from "framer-motion";

import { FONT_MONO, GOLD, GOLD_GLOW } from "../design/theme";

const dim = (a: number) => `rgba(233,237,255,${a})`;

export interface RailSection {
  id: string;
  /** The display name in the nav rail. Title-case, no article. */
  nav: string;
}

export default function SectionRail({
  sections,
  badge,
  cta,
  railId,
}: {
  sections: readonly RailSection[];
  /** The page's identity, next to the wordmark — e.g. "the case". */
  badge: string;
  cta: { href: string; label: string };
  /** Unique per page: keys the active-marker's shared-layout animation. */
  railId: string;
}) {
  const reduce = !!useReducedMotion();
  const [active, setActive] = useState<string>(sections[0]?.id ?? "");

  const { scrollYProgress } = useScroll();
  const progress = useSpring(scrollYProgress, { stiffness: 90, damping: 30, restDelta: 0.001 });

  useEffect(() => {
    const seen = new Map<string, number>();
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) seen.set(e.target.id, e.intersectionRatio);
        let best: string | null = null;
        let bestRatio = 0;
        for (const [id, ratio] of seen) {
          if (ratio > bestRatio) {
            bestRatio = ratio;
            best = id;
          }
        }
        if (best && bestRatio > 0) setActive(best);
      },
      // A band across the middle of the viewport: the section the reader is
      // actually looking at, not the one grazing the bottom edge.
      { rootMargin: "-25% 0px -45% 0px", threshold: [0, 0.15, 0.4, 0.75, 1] },
    );

    for (const s of sections) {
      const el = document.getElementById(s.id);
      if (el) io.observe(el);
    }
    return () => io.disconnect();
  }, [sections]);

  const go = useCallback(
    (id: string) => (e: React.MouseEvent) => {
      e.preventDefault();
      const el = document.getElementById(id);
      if (!el) return;
      el.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" });
      window.history.replaceState(null, "", `#${id}`);
    },
    [reduce],
  );

  return (
    <header
      className="sticky top-0 z-40 backdrop-blur"
      style={{
        background: "rgba(8,7,12,0.82)",
        borderBottom: "1px solid rgba(233,237,255,0.08)",
      }}
    >
      <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-x-6 gap-y-2 px-6 py-3">
        <Link href="/" className="flex shrink-0 items-center gap-2.5">
          <span className="text-base font-semibold tracking-tight text-white">Brainiac</span>
          <span
            className={`${FONT_MONO} text-[11px] uppercase tracking-[0.2em]`}
            style={{ color: GOLD }}
          >
            {badge}
          </span>
        </Link>

        <nav
          aria-label="Sections"
          className={`${FONT_MONO} flex flex-wrap items-center gap-x-4 gap-y-1 text-xs`}
        >
          {sections.map((s) => {
            const on = s.id === active;
            return (
              <a
                key={s.id}
                href={`#${s.id}`}
                onClick={go(s.id)}
                aria-current={on ? "true" : undefined}
                className="relative py-1 transition"
                style={{ color: on ? GOLD : dim(0.45) }}
              >
                {s.nav}
                {on && (
                  <motion.span
                    layoutId={`${railId}-nav-active`}
                    className="absolute -bottom-[13px] left-0 right-0 h-[2px]"
                    style={{ background: GOLD, boxShadow: `0 0 10px ${GOLD_GLOW}` }}
                    transition={{ type: "spring", stiffness: 380, damping: 32 }}
                  />
                )}
              </a>
            );
          })}
        </nav>

        <Link
          href={cta.href}
          className={`${FONT_MONO} hidden shrink-0 rounded-full border px-4 py-1.5 text-xs transition hover:text-[#f3c74f] lg:block`}
          style={{ borderColor: "rgba(233,237,255,0.18)", color: dim(0.7) }}
        >
          {cta.label}
        </Link>
      </div>

      {/* The progress line IS the nav's lower edge — the active-section marker
          above rides on it, so position and progress are one object. */}
      <motion.div
        aria-hidden
        className="h-[2px] origin-left"
        style={{ scaleX: progress, background: GOLD, boxShadow: `0 0 12px ${GOLD_GLOW}` }}
      />
    </header>
  );
}
