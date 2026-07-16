"use client";

/*
 * Standards — the one module whose prototype round is still OPEN, parked here.
 *
 * Reviews and the archive were decided in the console against live queues. This
 * board could not be: the dev org has no divergences at all, and a sweep finds
 * few by design, so there was nothing to judge a layout against. The tour is
 * where the fixture org lives, so the round moved here — /demo?m=divergence —
 * where there is a corpus to argue with.
 *
 * TWO THINGS THIS OWES A VISITOR, since it sits on a public page:
 *  - The switcher is a design tool, not a product feature. It is labelled as an
 *    open round rather than dressed up as a view toggle.
 *  - 60 divergences is NOT the healthy steady state (see divergence-scale.ts).
 *    A sparse board is the good outcome; this is the first-sweep pile, shown
 *    because it is the state a layout has to survive, not the state to expect.
 *
 * Delete this file and the losing variants when the round closes.
 */

import { useMemo } from "react";

import PrototypeSwitcher, { usePrototypeState, type VariantDef } from "@/design/PrototypeSwitcher";
import type { PracticeDivergences } from "@/lib/types";

import PracticeDivergenceReport from "@/divergence/PracticeDivergence";
import { makeLargeDivergences, SCALE_DIVERGENCES } from "@/divergence/divergence-scale";
import StandardsVariantBoard from "@/divergence/variants/StandardsVariantBoard";
import StandardsVariantMatrix from "@/divergence/variants/StandardsVariantMatrix";

const VARIANTS: VariantDef[] = [
  { id: "baseline", name: "baseline", blurb: "the shipped card column" },
  { id: "board", name: "board", blurb: "impact lanes, compact rows, ratify in place" },
  { id: "matrix", name: "matrix", blurb: "practice × team interference grid" },
];

export default function DemoStandards({ data }: { data: PracticeDivergences }) {
  const { variant, pickVariant, large, pickScale } = usePrototypeState(
    "brainiac-proto-standards",
    VARIANTS,
  );
  const big = useMemo(() => makeLargeDivergences(), []);
  const shown = large ? big : data;

  return (
    <>
      {variant === "baseline" && <PracticeDivergenceReport data={shown} />}
      {variant === "board" && <StandardsVariantBoard data={shown} />}
      {variant === "matrix" && <StandardsVariantMatrix data={shown} />}
      <PrototypeSwitcher
        variants={VARIANTS}
        variant={variant}
        onVariant={pickVariant}
        large={large}
        onScale={pickScale}
        smallLabel={`live · ${data.divergences?.length ?? 0}`}
        largeLabel={`org scale · ${SCALE_DIVERGENCES}`}
      />
    </>
  );
}
