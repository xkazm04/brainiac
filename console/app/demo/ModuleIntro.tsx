/*
 * The tour's framing, and the reason the console stays clean.
 *
 * Every module below this element is the operator's own component, rendered on
 * fixtures — so it must not carry a word of pitch. The sales voice ("this is
 * what your organization's memory looks like once someone is accountable for
 * it") belongs to the demo and nowhere else, so it lives here, above the module,
 * in a demo-only element. Nothing in this file is imported by /console.
 *
 * The audience is a public visitor, not an operator: they have no console to
 * compare against and no session to check it with, so the intro names no routes.
 * "/console/reviews" is our word for this surface, not theirs — to a visitor it
 * is implementation trivia at best, and a link they cannot open at worst.
 */

import { band, FONT_MONO, LABEL } from "@/design/theme";

const BETA = band("beta");

/**
 * The console's modules were each designed to their own measure — the archive
 * and the bench are wide, the review queue and the reports are narrow. The intro
 * adopts the width of the module it introduces so the two headlines share a left
 * edge; a fixed width here would leave the pitch hanging off the side of every
 * surface it does not happen to match. Spelled out rather than interpolated
 * because Tailwind only emits classes it can see as literals.
 */
export type ModuleWidth = "max-w-5xl" | "max-w-6xl" | "max-w-7xl";

export interface ModuleIntroCopy {
  /** The demo's framing headline for this module. */
  title: string;
  /** One paragraph of context, in the demo's voice. */
  description: string;
  /** The measure of the module below — the intro matches it. */
  width: ModuleWidth;
}

export default function ModuleIntro({ title, description, width }: ModuleIntroCopy) {
  return (
    <section className={`mx-auto ${width} px-6 pt-8`}>
      <span className={LABEL} style={{ color: BETA }}>
        the demo org
      </span>

      <h1 className="mt-2 max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white md:text-4xl">
        {title}
      </h1>
      <p
        className={`${FONT_MONO} mt-4 max-w-2xl text-sm leading-relaxed`}
        style={{ color: "rgba(233,237,255,0.55)" }}
      >
        {description}
      </p>

      {/* The seam: pitch above, product below. Everything past this rule is the
          operator's surface, unedited. */}
      <div
        className="mt-8 border-t"
        style={{ borderColor: band("beta", 68, 0.18) }}
        aria-hidden
      />
    </section>
  );
}
