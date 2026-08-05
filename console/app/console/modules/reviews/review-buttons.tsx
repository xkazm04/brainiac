"use client";

/*
 * The maintainer's affordances on the review queue. Pure presentation over the
 * server actions in ./actions (unchanged): promotions get approve (constructive
 * gamma) / reject (neutral), contradictions get the four resolutions.
 *
 * Every control here is the shared <Button variant="pill" size="sm">. That
 * bucket is byte-for-byte the local `PILL` constant this file used to carry
 * (rounded-full border px-3.5 py-1.5 text-sm, the same hover wash and the same
 * disabled treatment), so the migration was a swap and not a restyle — the one
 * thing it adds is the focus ring, which none of these buttons had. The accents
 * are equally unchanged: `accent` derives `borderColor: withAlpha(x, 0.4)` +
 * `color: x`, which is exactly what each call site spelled out by hand, and the
 * neutral pair (reject / dismiss) omits `accent` to land on the same
 * rgba(233,237,255,0.2) edge over INK_DIM text.
 *
 * The disputes DecisionBar answers through the same primitive, so the two
 * governance surfaces still match by construction rather than by copy-paste.
 */

import { useState, useTransition } from "react";

import type { ActionResult } from "./actions";
import { resolveContradictionAction, reviewPromotionAction } from "./actions";
import Button from "@/components/ui/Button";
import { band, FONT_MONO, GOLD, MAGENTA } from "@/design/theme";
import type { ContradictionResolution } from "@/lib/types";

function useAction() {
  const [pending, startTransition] = useTransition();
  const [result, setResult] = useState<ActionResult | null>(null);
  const run = (fn: () => Promise<ActionResult>) =>
    startTransition(async () => setResult(await fn()));
  return { pending, result, run };
}

function ResultNote({ result }: { result: ActionResult | null }) {
  if (!result) return null;
  return (
    <span
      role="status"
      className={`${FONT_MONO} text-xs`}
      style={{ color: result.ok ? GOLD : MAGENTA }}
    >
      {result.message}
    </span>
  );
}

export function PromotionButtons({ promotionId }: { promotionId: string }) {
  const { pending, result, run } = useAction();
  return (
    <div className={`${FONT_MONO} flex flex-wrap items-center gap-2`}>
      <Button
        disabled={pending}
        accent={GOLD}
        onClick={() => run(() => reviewPromotionAction(promotionId, "approve"))}
      >
        approve
      </Button>
      {/* No accent: reject is the neutral answer, and the neutral ink edge is
          what `controlStyle` already returns when none is given. */}
      <Button
        disabled={pending}
        onClick={() => run(() => reviewPromotionAction(promotionId, "reject"))}
      >
        reject
      </Button>
      <ResultNote result={result} />
    </div>
  );
}

export function ContradictionButtons({
  contradictionId,
  memoryAId,
  memoryBId,
}: {
  contradictionId: string;
  memoryAId: string;
  memoryBId: string;
}) {
  const { pending, result, run } = useAction();
  const ALPHA = band("alpha");
  const resolve = (resolution: ContradictionResolution, winner?: string) =>
    run(() => resolveContradictionAction(contradictionId, resolution, winner));
  return (
    <div className={`${FONT_MONO} flex flex-wrap items-center gap-2`}>
      <Button
        disabled={pending}
        accent={GOLD}
        title="A supersedes B"
        onClick={() => resolve("supersede", memoryAId)}
      >
        A wins
      </Button>
      <Button
        disabled={pending}
        accent={GOLD}
        title="B supersedes A"
        onClick={() => resolve("supersede", memoryBId)}
      >
        B wins
      </Button>
      <Button
        disabled={pending}
        accent={ALPHA}
        title="Both stand — no contradiction"
        onClick={() => resolve("coexist")}
      >
        coexist
      </Button>
      {/* Dismiss is the neutral answer here, so it drops the accent — same
          reason as reject above. */}
      <Button
        disabled={pending}
        title="Dismiss — not a real contradiction"
        onClick={() => resolve("dismiss")}
      >
        dismiss
      </Button>
      <ResultNote result={result} />
    </div>
  );
}
