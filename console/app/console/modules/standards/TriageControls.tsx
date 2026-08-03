"use client";

/*
 * The gate's buttons. The state machine (triage.ts) decides what may be
 * offered; the backend remains the authority — a 409 on a plain adopt
 * re-offers the action as an explicit signed decree, with the consequence
 * spelled out before the second click.
 *
 * Alongside judging a candidate, the gate can now NARROW one. Mining and the
 * ratification bridge file everything under `general`, so before this the only
 * honest answer to "this is really a rust rule" was to reject it and hope
 * someone re-proposed it correctly. Editing sits next to adopt/reject
 * deliberately: it is the same decision, taken with the same authority.
 */

import { useState, useTransition } from "react";

import { FONT_MONO, INK, INK_FAINT, PANEL } from "@/design/theme";
import type { StandardDetail } from "@/lib/types";

import {
  adoptStandardAction,
  deprecateStandardAction,
  rejectStandardAction,
  updateStandardAction,
  type ActionResult,
} from "./actions";
import { adoptPlan, allowedActions, editablePatch, isEditable, type RuleDraft } from "./triage";

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

function Field({
  label,
  value,
  onChange,
  hint,
  multiline,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  hint?: string;
  multiline?: boolean;
}) {
  const shared = `${FONT_MONO} w-full rounded-md border px-2 py-1.5 text-[12px]`;
  const style = { borderColor: dim(0.18), background: PANEL, color: INK };
  return (
    <label className="flex flex-col gap-1">
      <span
        className={`${FONT_MONO} text-[10px] uppercase tracking-[0.16em]`}
        style={{ color: INK_FAINT }}
      >
        {label}
        {hint ? <span className="normal-case tracking-normal"> · {hint}</span> : null}
      </span>
      {multiline ? (
        <textarea
          value={value}
          rows={3}
          onChange={(e) => onChange(e.target.value)}
          className={shared}
          style={style}
        />
      ) : (
        <input value={value} onChange={(e) => onChange(e.target.value)} className={shared} style={style} />
      )}
    </label>
  );
}

export default function TriageControls({ detail }: { detail: StandardDetail }) {
  const [pending, start] = useTransition();
  const [result, setResult] = useState<ActionResult | null>(null);
  const [confirmingDecree, setConfirmingDecree] = useState(false);
  const [draft, setDraft] = useState<RuleDraft | null>(null);

  const actions = allowedActions(detail.lifecycle);
  const editable = isEditable(detail.lifecycle);
  if (actions.length === 0 && !editable && !result) return null;

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
        {actions.includes("adopt") && !confirmingDecree && !draft && (
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
        {actions.includes("reject") && !confirmingDecree && !draft && (
          <GateButton
            label="reject — and remember"
            tone={MAGENTA}
            busy={pending}
            onClick={() => run(() => rejectStandardAction(detail.id))}
          />
        )}
        {actions.includes("deprecate") && !draft && (
          <GateButton
            label="retire"
            tone={MAGENTA}
            busy={pending}
            onClick={() => run(() => deprecateStandardAction(detail.id))}
          />
        )}
        {editable && !confirmingDecree && !draft && (
          <GateButton
            label={
              detail.stack === "general"
                ? "narrow — this isn't a general rule…"
                : "narrow / re-word…"
            }
            tone={dim(0.55)}
            busy={pending}
            onClick={() =>
              setDraft({
                stack: detail.stack,
                category: detail.category,
                statement: detail.statement,
              })
            }
          />
        )}
      </div>

      {draft && (
        <div
          className="flex max-w-lg flex-col gap-3 rounded-lg border p-4"
          style={{ borderColor: dim(0.16) }}
        >
          <div className="grid grid-cols-2 gap-3">
            <Field
              label="stack"
              hint="rust, typescript, general"
              value={draft.stack}
              onChange={(stack) => setDraft({ ...draft, stack })}
            />
            <Field
              label="category"
              hint="errors, testing…"
              value={draft.category}
              onChange={(category) => setDraft({ ...draft, category })}
            />
          </div>
          <Field
            label="statement"
            hint="one sentence — a change here mints a revision"
            multiline
            value={draft.statement}
            onChange={(statement) => setDraft({ ...draft, statement })}
          />
          <div className="flex flex-wrap items-center gap-3">
            <GateButton
              label="save"
              tone={MINT}
              busy={pending || editablePatch(detail, draft) === null}
              onClick={() => {
                const patch = editablePatch(detail, draft);
                if (!patch) return;
                run(async () => {
                  const r = await updateStandardAction(detail.id, patch);
                  if (r.ok) setDraft(null);
                  return r;
                });
              }}
            />
            <GateButton
              label="cancel"
              tone={dim(0.5)}
              busy={pending}
              onClick={() => setDraft(null)}
            />
          </div>
          {detail.lifecycle === "adopted" && (
            <p className={`${FONT_MONO} text-[11px] leading-relaxed`} style={{ color: GOLD }}>
              this rule is already adopted. re-wording it is a new revision — the wording the
              org ratified stays in the history, and the {detail.stack} standards page
              recomposes from what you write here.
            </p>
          )}
        </div>
      )}
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
