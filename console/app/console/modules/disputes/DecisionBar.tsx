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

import Button from "@/components/ui/Button";
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

  // The bar's two densities map onto the shared pill's size buckets: `md` is
  // exactly `pill/sm` (px-3.5 py-1.5 text-sm) and `sm` is exactly `pill/xs`
  // (px-2.5 py-1) apart from the type scale — xs is 11px where this bar has
  // always been 12px. That one pixel goes through `style` rather than a
  // `text-xs` className: Tailwind resolves two competing font-size utilities by
  // stylesheet order, not className order (the same trap control-styles.ts
  // documents for width), so an inline value is the only deterministic way to
  // land on the size this bar had before.
  const btnSize = size === "sm" ? "xs" : "sm";
  const typeScale = size === "sm" ? { fontSize: "0.75rem", lineHeight: "1rem" } : undefined;

  return (
    <div className={`${FONT_MONO} flex flex-col gap-2.5`}>
      <div className="flex flex-wrap items-center gap-2">
        {DECISIONS.map((dcn) => (
          // `accent` derives the tinted edge + text this bar used to spell out
          // as `withAlpha(TONE, 0.4)` / `TONE` — same colour, one input.
          <Button
            key={dcn.id}
            size={btnSize}
            accent={TONE[dcn.id]}
            disabled={pending || !live}
            title={live ? dcn.gloss : "demo data — connect the API to answer claims"}
            onClick={() => decide(dcn.id)}
            style={typeScale}
          >
            {dcn.verb}
          </Button>
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
            // `selected` rather than a hand-rolled pressed look, so the state is
            // announced (aria-pressed) and not only drawn. The colours stay
            // explicit: the primitive's unselected treatment tints the edge with
            // the accent, and these budget chips are deliberately NEUTRAL until
            // chosen — gamma on all four would read as four live answers rather
            // than one budget. `style` merges over controlStyle, so the explicit
            // values win. Padding is inline for a different reason (px-2 py-0.5
            // vs the xs bucket's px-2.5 py-1): competing Tailwind utilities
            // resolve by stylesheet order, an inline value does not.
            <Button
              key={c.label}
              size="xs"
              disabled={pending || !live}
              selected={on}
              onClick={() => setDays(c.days)}
              style={{
                paddingInline: "0.5rem",
                paddingBlock: "0.125rem",
                borderColor: on ? withAlpha(band("gamma"), 0.55) : "rgba(233,237,255,0.12)",
                color: on ? band("gamma") : "rgba(233,237,255,0.45)",
                background: on ? withAlpha(band("gamma"), 0.1) : "transparent",
              }}
            >
              {c.label}
            </Button>
          );
        })}
      </div>
    </div>
  );
}
