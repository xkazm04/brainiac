"use client";

/*
 * The three answers a maintainer can give a disputed memory. Constructive
 * (gamma) / destructive (magenta) / neutral follows the theme's light
 * metaphor: re-verifying restores the signal, deprecating nulls it,
 * dismissing cancels the noise.
 *
 * The outcome is NOT rendered here. `claims_closed` is the one number proving
 * the write landed, and this bar is unmounted by the refresh its own success
 * triggers — so the result is handed up (`onResult`) to something that outlives
 * the row, and shown there. See DisputeBench's receipt.
 */

import { useState, useTransition } from "react";

import { band, FONT_MONO, MAGENTA, withAlpha } from "@/design/theme";

import { resolveDisputeAction, type DecisionResult } from "./actions";
import { DECISIONS, EXTEND_CHOICES, type Resolution } from "./disputes-data";

const TONE: Record<Resolution, string> = {
  reverified: band("gamma"),
  deprecated: MAGENTA,
  dismissed: "rgba(233,237,255,0.55)",
};

export default function DecisionBar({
  memoryId,
  live,
  size = "md",
  onResult,
}: {
  memoryId: string;
  live: boolean;
  size?: "sm" | "md";
  /** Called with every outcome — success or failure. */
  onResult?: (r: DecisionResult, memoryId: string) => void;
}) {
  const [pending, startTransition] = useTransition();
  // The re-verification budget. `null` = whatever TTL the memory's kind
  // carries, which is the only honest default: the console does not know the
  // kind TTLs, the server does.
  const [days, setDays] = useState<number | null>(null);

  const decide = (resolution: Resolution) =>
    startTransition(async () => {
      const r = await resolveDisputeAction(memoryId, resolution, days ?? undefined);
      onResult?.(r, memoryId);
    });

  const pad = size === "sm" ? "px-2.5 py-1 text-xs" : "px-3.5 py-1.5 text-sm";

  return (
    <div className={`${FONT_MONO} flex flex-col gap-2.5`}>
      <div className="flex flex-wrap items-center gap-2">
        {DECISIONS.map((dcn) => (
          <button
            key={dcn.id}
            type="button"
            disabled={pending || !live}
            title={live ? dcn.gloss : "demo data — connect the API to answer claims"}
            onClick={() => decide(dcn.id)}
            className={`rounded-full border ${pad} transition disabled:cursor-not-allowed disabled:opacity-40 hover:bg-white/5`}
            style={{ borderColor: withAlpha(TONE[dcn.id], 0.4), color: TONE[dcn.id] }}
          >
            {dcn.verb}
          </button>
        ))}
      </div>

      {/* The budget rides WITH "still true" rather than behind a second step:
          a maintainer who checked a fact usually knows how long it is good
          for, and hiding that behind a disclosure is how it stayed unreachable. */}
      <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
        <span style={{ color: "rgba(233,237,255,0.35)" }}>still true → good for</span>
        {EXTEND_CHOICES.map((c) => {
          const on = c.days === days;
          return (
            <button
              key={c.label}
              type="button"
              disabled={pending || !live}
              aria-pressed={on}
              onClick={() => setDays(c.days)}
              className="rounded-full border px-2 py-0.5 transition disabled:cursor-not-allowed disabled:opacity-40 hover:bg-white/5"
              style={{
                borderColor: on ? withAlpha(band("gamma"), 0.55) : "rgba(233,237,255,0.12)",
                color: on ? band("gamma") : "rgba(233,237,255,0.45)",
                background: on ? withAlpha(band("gamma"), 0.1) : "transparent",
              }}
            >
              {c.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
