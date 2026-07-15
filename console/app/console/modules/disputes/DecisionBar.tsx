"use client";

/*
 * The three answers a maintainer can give a disputed memory. Constructive
 * (gamma) / destructive (magenta) / neutral follows the theme's light
 * metaphor: re-verifying restores the signal, deprecating nulls it,
 * dismissing cancels the noise.
 */

import { useState, useTransition } from "react";

import { band, FONT_MONO, MAGENTA } from "@/design/theme";

import { resolveDisputeAction, type DecisionResult } from "./actions";
import { DECISIONS, type Resolution } from "./disputes-data";

const TONE: Record<Resolution, string> = {
  reverified: band("gamma"),
  deprecated: MAGENTA,
  dismissed: "rgba(233,237,255,0.55)",
};

function useDecision(onDone?: () => void) {
  const [pending, startTransition] = useTransition();
  const [result, setResult] = useState<DecisionResult | null>(null);
  const decide = (memoryId: string, resolution: Resolution) =>
    startTransition(async () => {
      const r = await resolveDisputeAction(memoryId, resolution);
      setResult(r);
      if (r.ok) onDone?.();
    });
  return { pending, result, decide };
}

export default function DecisionBar({
  memoryId,
  live,
  size = "md",
  onDone,
}: {
  memoryId: string;
  live: boolean;
  size?: "sm" | "md";
  onDone?: () => void;
}) {
  const { pending, result, decide } = useDecision(onDone);
  const pad = size === "sm" ? "px-2.5 py-1 text-xs" : "px-3.5 py-1.5 text-sm";

  return (
    <div className={`${FONT_MONO} flex flex-wrap items-center gap-2`}>
      {DECISIONS.map((dcn) => (
        <button
          key={dcn.id}
          type="button"
          disabled={pending || !live}
          title={live ? dcn.gloss : "demo data — connect the API to answer claims"}
          onClick={() => decide(memoryId, dcn.id)}
          className={`rounded-full border ${pad} transition disabled:cursor-not-allowed disabled:opacity-40 hover:bg-white/5`}
          style={{ borderColor: `${TONE[dcn.id]}66`, color: TONE[dcn.id] }}
        >
          {dcn.verb}
        </button>
      ))}
      {result && (
        <span
          role="status"
          className="text-xs"
          style={{ color: result.ok ? band("gamma") : MAGENTA }}
        >
          {result.message}
        </span>
      )}
    </div>
  );
}
