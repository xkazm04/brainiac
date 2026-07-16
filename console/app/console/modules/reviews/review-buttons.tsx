"use client";

/*
 * The maintainer's affordances on the review queue. Pure presentation over the
 * server actions in ./actions (unchanged): promotions get approve (constructive
 * gamma) / reject (neutral), contradictions get the four resolutions. Pill
 * styling matches the disputes DecisionBar so every governance surface answers
 * the same way.
 */

import { useState, useTransition } from "react";

import type { ActionResult } from "./actions";
import { resolveContradictionAction, reviewPromotionAction } from "./actions";
import {
  band,
  FONT_MONO,
  GOLD,
  INK_DIM,
  MAGENTA,
  withAlpha,
} from "@/design/theme";
import type { ContradictionResolution } from "@/lib/types";

const PILL =
  "rounded-full border px-3.5 py-1.5 text-sm transition disabled:cursor-not-allowed disabled:opacity-40 hover:bg-white/5";

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
      <button
        type="button"
        disabled={pending}
        onClick={() => run(() => reviewPromotionAction(promotionId, "approve"))}
        className={PILL}
        style={{ borderColor: withAlpha(GOLD, 0.4), color: GOLD }}
      >
        approve
      </button>
      <button
        type="button"
        disabled={pending}
        onClick={() => run(() => reviewPromotionAction(promotionId, "reject"))}
        className={PILL}
        style={{ borderColor: "rgba(233,237,255,0.2)", color: INK_DIM }}
      >
        reject
      </button>
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
      <button
        type="button"
        disabled={pending}
        title="A supersedes B"
        onClick={() => resolve("supersede", memoryAId)}
        className={PILL}
        style={{ borderColor: withAlpha(GOLD, 0.4), color: GOLD }}
      >
        A wins
      </button>
      <button
        type="button"
        disabled={pending}
        title="B supersedes A"
        onClick={() => resolve("supersede", memoryBId)}
        className={PILL}
        style={{ borderColor: withAlpha(GOLD, 0.4), color: GOLD }}
      >
        B wins
      </button>
      <button
        type="button"
        disabled={pending}
        title="Both stand — no contradiction"
        onClick={() => resolve("coexist")}
        className={PILL}
        style={{ borderColor: withAlpha(ALPHA, 0.4), color: ALPHA }}
      >
        coexist
      </button>
      <button
        type="button"
        disabled={pending}
        title="Dismiss — not a real contradiction"
        onClick={() => resolve("dismiss")}
        className={PILL}
        style={{ borderColor: "rgba(233,237,255,0.2)", color: INK_DIM }}
      >
        dismiss
      </button>
      <ResultNote result={result} />
    </div>
  );
}
