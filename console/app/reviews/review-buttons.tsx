"use client";

import { useState, useTransition } from "react";

import type { ActionResult } from "./actions";
import { resolveContradictionAction, reviewPromotionAction } from "./actions";
import type { ContradictionResolution } from "@/lib/types";

function useAction() {
  const [pending, startTransition] = useTransition();
  const [result, setResult] = useState<ActionResult | null>(null);
  const run = (fn: () => Promise<ActionResult>) =>
    startTransition(async () => setResult(await fn()));
  return { pending, result, run };
}

export function PromotionButtons({ promotionId }: { promotionId: string }) {
  const { pending, result, run } = useAction();
  return (
    <span>
      <button
        disabled={pending}
        onClick={() => run(() => reviewPromotionAction(promotionId, "approve"))}
      >
        Approve
      </button>{" "}
      <button
        disabled={pending}
        onClick={() => run(() => reviewPromotionAction(promotionId, "reject"))}
      >
        Reject
      </button>
      {result && <em role="status"> {result.message}</em>}
    </span>
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
  const resolve = (resolution: ContradictionResolution, winner?: string) =>
    run(() => resolveContradictionAction(contradictionId, resolution, winner));
  return (
    <span>
      <button disabled={pending} onClick={() => resolve("supersede", memoryAId)}>
        A wins
      </button>{" "}
      <button disabled={pending} onClick={() => resolve("supersede", memoryBId)}>
        B wins
      </button>{" "}
      <button disabled={pending} onClick={() => resolve("coexist")}>
        Coexist
      </button>{" "}
      <button disabled={pending} onClick={() => resolve("dismiss")}>
        Dismiss
      </button>
      {result && <em role="status"> {result.message}</em>}
    </span>
  );
}
