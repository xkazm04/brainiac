import Link from "next/link";
import type { ReactNode } from "react";

import { BG, BORDER, FONT_DISPLAY, FONT_MONO, GOLD, INK_DIM, INK_FAINT, LABEL, PANEL, withAlpha } from "@/design/theme";

import { LEGAL_LAST_UPDATED } from "./entity";

/*
 * The shell both legal pages render into.
 *
 * Dark only, like the rest of the console (theme.ts: "Fixed art direction →
 * literal hexes/hsl on purpose (no light theme)"), and measure-limited: a
 * privacy policy at the console's full width is a policy nobody finishes.
 * Prose is set in the mono microcopy face at a comfortable reading size rather
 * than the instrument's 11px — this is the one surface in the product meant to
 * be READ, not scanned.
 */

export function LegalShell({
  kicker,
  title,
  intro,
  children,
}: {
  kicker: string;
  title: string;
  intro: ReactNode;
  children: ReactNode;
}) {
  return (
    <main className="min-h-screen px-6 py-16 sm:py-24" style={{ background: BG }}>
      <div className="mx-auto w-full max-w-3xl">
        <Link
          href="/"
          className={`${LABEL} inline-block transition-opacity hover:opacity-100`}
          style={{ color: INK_FAINT }}
        >
          ← brainiac
        </Link>

        <div className={`${LABEL} mt-10`} style={{ color: GOLD }}>
          {kicker}
        </div>
        <h1
          className={`${FONT_DISPLAY} mt-3 text-4xl font-semibold tracking-tight text-white sm:text-5xl`}
        >
          {title}
        </h1>
        <p className={`${FONT_MONO} mt-2 text-[11px] uppercase tracking-[0.2em]`} style={{ color: INK_FAINT }}>
          last updated {LEGAL_LAST_UPDATED}
        </p>

        <div
          className={`${FONT_MONO} mt-8 rounded-xl border p-5 text-sm leading-relaxed`}
          style={{ borderColor: BORDER, background: PANEL, color: INK_DIM }}
        >
          {intro}
        </div>

        <div className="mt-4">{children}</div>

        <div
          className={`${FONT_MONO} mt-16 border-t pt-6 text-xs leading-relaxed`}
          style={{ borderColor: BORDER, color: withAlpha(INK_FAINT, 0.7) }}
        >
          <Link href="/privacy" className="underline underline-offset-4 hover:text-white">
            Privacy
          </Link>
          <span className="px-2">·</span>
          <Link href="/terms" className="underline underline-offset-4 hover:text-white">
            Terms
          </Link>
        </div>
      </div>
    </main>
  );
}

/** One numbered clause. The id makes every clause linkable — policies get cited. */
export function Clause({
  id,
  n,
  heading,
  children,
}: {
  id: string;
  n: number;
  heading: string;
  children: ReactNode;
}) {
  return (
    <section id={id} className="mt-10 scroll-mt-8">
      <h2 className={`${FONT_DISPLAY} flex items-baseline gap-3 text-xl font-semibold text-white`}>
        <span className={`${FONT_MONO} text-xs`} style={{ color: GOLD }}>
          {String(n).padStart(2, "0")}
        </span>
        {heading}
      </h2>
      <div className={`${FONT_MONO} mt-3 flex flex-col gap-3 text-sm leading-relaxed`} style={{ color: INK_DIM }}>
        {children}
      </div>
    </section>
  );
}

/** A definition-style row: what is collected / what a term means. */
export function Row({ term, children }: { term: string; children: ReactNode }) {
  return (
    <div
      className="grid gap-1 border-l pl-4 sm:grid-cols-[minmax(0,11rem)_1fr] sm:gap-4"
      style={{ borderColor: withAlpha(GOLD, 0.35) }}
    >
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.72)" }}>
        {term}
      </div>
      <div>{children}</div>
    </div>
  );
}

/** Emphasis for the sentences a reader must not skim past. */
export function Strong({ children }: { children: ReactNode }) {
  return <strong className="font-normal text-white">{children}</strong>;
}
