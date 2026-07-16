"use client";

/*
 * The gate's buttons. The state machine (triage.ts) decides what may be
 * offered; the backend remains the authority — a 409 on a plain adopt
 * re-offers the action as an explicit signed decree, with the consequence
 * spelled out before the second click.
 */

import { useState, useTransition } from "react";

import { FONT_MONO } from "@/design/theme";
import type { StandardDetail } from "@/lib/types";

import {
  adoptStandardAction,
  deprecateStandardAction,
  rejectStandardAction,
  type ActionResult,
} from "./actions";
import { adoptPlan, allowedActions } from "./triage";

const GOLD = "hsla(46, 90%, 68%, 1)";
const MINT = "hsla(158, 90%, 68%, 1)";
const MAGENTA = "#ff5da2";
const dim = (a: number) => `rgba(233,237,255,${a})`;

function GateButton({
  label,
  tone,
  busy,
  onClick,
}: {
  label: string;
  tone: string;
  busy: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      disabled={busy}
      className={`${FONT_MONO} rounded-full border px-4 py-2 text-xs uppercase tracking-[0.14em] transition hover:scale-[1.02] disabled:opacity-40`}
      style={{ borderColor: tone, color: tone }}
    >
      {label}
    </button>
  );
}

export default function TriageControls({ detail }: { detail: StandardDetail }) {
  const [pending, start] = useTransition();
  const [result, setResult] = useState<ActionResult | null>(null);
  const [confirmingDecree, setConfirmingDecree] = useState(false);

  const actions = allowedActions(detail.lifecycle);
  if (actions.length === 0 && !result) return null;

  const run = (fn: () => Promise<ActionResult>) =>
    start(async () => {
      const r = await fn();
      setResult(r);
      if (r.needsDecree) setConfirmingDecree(true);
    });

  const plan = adoptPlan(detail);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        {actions.includes("adopt") && !confirmingDecree && (
          <GateButton
            label={plan.kind === "needs_decree" ? "adopt — no evidence…" : "adopt"}
            tone={MINT}
            busy={pending}
            onClick={() =>
              plan.kind === "needs_decree"
                ? setConfirmingDecree(true)
                : run(() => adoptStandardAction(detail.id, false))
            }
          />
        )}
        {confirmingDecree && (
          <>
            <GateButton
              label="sign the decree — adopt without evidence"
              tone={GOLD}
              busy={pending}
              onClick={() =>
                run(async () => {
                  const r = await adoptStandardAction(detail.id, true);
                  setConfirmingDecree(false);
                  return r;
                })
              }
            />
            <GateButton
              label="cancel"
              tone={dim(0.5)}
              busy={pending}
              onClick={() => setConfirmingDecree(false)}
            />
          </>
        )}
        {actions.includes("reject") && !confirmingDecree && (
          <GateButton
            label="reject — and remember"
            tone={MAGENTA}
            busy={pending}
            onClick={() => run(() => rejectStandardAction(detail.id))}
          />
        )}
        {actions.includes("deprecate") && (
          <GateButton
            label="retire"
            tone={MAGENTA}
            busy={pending}
            onClick={() => run(() => deprecateStandardAction(detail.id))}
          />
        )}
      </div>
      {confirmingDecree && (
        <p className={`${FONT_MONO} max-w-md text-[11px] leading-relaxed`} style={{ color: GOLD }}>
          this rule carries no provenance. adopting it is a decree: your token&apos;s name goes on
          it, permanently — the only legal kind of evidence-free rule.
        </p>
      )}
      {result && !result.needsDecree && (
        <p
          className={`${FONT_MONO} text-[11px]`}
          style={{ color: result.ok ? MINT : MAGENTA }}
          role="status"
        >
          {result.message}
        </p>
      )}
    </div>
  );
}
