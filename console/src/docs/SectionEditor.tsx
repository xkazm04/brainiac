"use client";

/*
 * The section editor (KB-PLAN KB4).
 *
 * The design problem is not the textarea. It is that two visually identical
 * sections behave in completely opposite ways when you edit them, and the
 * maintainer must know which one they are in BEFORE they type — discovering
 * afterwards that your careful paragraph became a *proposal* would feel like
 * the product lost your work.
 *
 * So the mode is declared in the affordance itself ("edit this prose" vs
 * "propose a change"), restated as a rim + a warning the moment the editor
 * opens, and confirmed in the button ("save this prose" vs "propose this
 * change"). The word "save" appears nowhere on the composed path — before or
 * after (see edit-copy.ts).
 *
 * Like ApproveRevision, this island is only ever rendered when the console is
 * live: a mutating control wired to demo data would be a lie with consequences.
 */

import { useState, useTransition } from "react";

import Button from "@/components/ui/Button";
import { Input, Textarea } from "@/components/ui/Input";
import {
  band,
  BORDER,
  FONT_MONO,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  withAlpha,
} from "@/design/theme";
import type { DocSection } from "@/lib/types";

import { INTENT, OUTCOME, asMode, asOutcome, type EditOutcomeCopy } from "./edit-copy";

export interface EditResult {
  ok: boolean;
  /** The server's `outcome` — absent when the call failed. */
  outcome?: string;
  /** The server's own `message`, shown verbatim: it is the API's wording. */
  message: string;
}

export interface SectionEditorProps {
  section: DocSection;
  /** Server action: POST /v1/docs/{slug}/edit, then revalidate. */
  edit: (sectionId: string, content: string, note: string) => Promise<EditResult>;
}

export default function SectionEditor({ section, edit }: SectionEditorProps) {
  const [open, setOpen] = useState(false);
  const [content, setContent] = useState("");
  const [note, setNote] = useState("");
  const [result, setResult] = useState<EditResult | null>(null);
  const [pending, start] = useTransition();

  // `mode` is a bare string on the wire; narrow it once, here.
  const mode = asMode(section.mode);
  const composed = mode === "composed";
  const intent = INTENT[mode];
  // Composed = gamma (the colour this console already uses for "decided, not
  // yet in production"). Pinned = alpha, the calm human-owned colour.
  const accent = composed ? band("gamma") : band("alpha");

  if (!open) {
    return (
      // The affordance that declares the mode. It is a `pill` at its own
      // micro-scale — the shared xs bucket is py-1/10.5px, this one has always
      // been py-[3px]/10px — so the geometry rides in `style`, where a value
      // cannot lose a Tailwind stylesheet-order coin toss to the bucket's own
      // padding utility.
      <Button
        variant="pill"
        size="xs"
        accent={accent}
        onClick={() => setOpen(true)}
        className="mt-2 uppercase tracking-[0.14em]"
        style={{
          fontSize: "10px",
          paddingLeft: "0.75rem",
          paddingRight: "0.75rem",
          paddingTop: "3px",
          paddingBottom: "3px",
          borderColor: withAlpha(accent, 0.33),
        }}
      >
        {composed ? "propose a change" : "edit this prose"}
      </Button>
    );
  }

  const copy: EditOutcomeCopy | null =
    result?.ok && result.outcome ? OUTCOME[asOutcome(result.outcome)] : null;
  const doneAccent = copy?.tone === "queued" ? band("gamma") : band("beta");

  return (
    <div
      className="mt-4 rounded-lg border p-5"
      style={{ borderColor: withAlpha(accent, 0.33), background: withAlpha(accent, 0.05) }}
    >
      <span className={LABEL} style={{ color: accent }}>
        {intent.label}
      </span>
      <p className="mt-2 text-[13.5px] leading-relaxed" style={{ color: INK_DIM }}>
        {intent.warning}
      </p>

      {copy ? (
        // The server has answered. Say exactly what happened — and for a
        // captured edit, never the word "Saved".
        <div className="mt-4 border-t pt-4" style={{ borderColor: BORDER }}>
          <span className={LABEL} style={{ color: doneAccent }}>
            {copy.status}
          </span>
          <p className="mt-2 text-[14px]" style={{ color: INK }}>
            {result?.message}
          </p>
          <p className={`${FONT_MONO} mt-2 text-[12px]`} style={{ color: INK_DIM }}>
            {copy.next}
          </p>
          <Button
            variant="quiet"
            size="xs"
            accent={INK_FAINT}
            onClick={() => {
              setResult(null);
              setContent("");
              setNote("");
              setOpen(false);
            }}
            className="mt-4 underline underline-offset-4"
          >
            close
          </Button>
        </div>
      ) : (
        <>
          {/* Both fields carried `outline-none`, which strips the keyboard ring
              along with the pointer one — on the two controls in this module a
              maintainer actually types into. The primitives carry `.focus-ring`
              instead, and it lives outside Tailwind's cascade layers so no
              stray utility can suppress it again. The dark inset wash, the
              border and the type scale are this editor's own and stay in
              `style`, on top of the field chrome. */}
          <Textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            rows={8}
            aria-label={`${section.heading} — ${composed ? "proposed change" : "prose"}`}
            placeholder={
              composed
                ? "Write the section as it should read. What you write is read for the knowledge in it, not pasted onto the page."
                : "Write the section as it should read. It goes onto the page as typed."
            }
            className="mt-4"
            style={{
              padding: "0.75rem",
              fontSize: "13px",
              background: "rgba(0,0,0,0.25)",
              borderColor: BORDER,
              color: INK,
            }}
          />
          <Input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            aria-label="why this changed"
            placeholder={
              composed
                ? "why did this change? (optional — but 'why' is the one thing a diff cannot recover)"
                : "note (optional)"
            }
            className="mt-2"
            style={{
              paddingLeft: "0.75rem",
              paddingRight: "0.75rem",
              fontSize: "12px",
              background: "rgba(0,0,0,0.25)",
              borderColor: BORDER,
              color: INK,
            }}
          />
          <div className="mt-4 flex items-center gap-4">
            {/* px-5 py-2 text-sm IS the `pill` md bucket — nothing to override
                but the edge alpha this module chose. */}
            <Button
              variant="pill"
              size="md"
              accent={accent}
              disabled={pending || content.trim().length === 0}
              onClick={() =>
                start(async () => {
                  setResult(await edit(section.id, content.trim(), note.trim()));
                })
              }
              style={{ borderColor: withAlpha(accent, 0.47) }}
            >
              {pending ? intent.cta_pending : intent.cta}
            </Button>
            <Button
              variant="quiet"
              size="xs"
              accent={INK_FAINT}
              onClick={() => setOpen(false)}
              className="underline underline-offset-4"
            >
              cancel
            </Button>
          </div>
          {result && !result.ok && (
            <p className={`${FONT_MONO} mt-3 text-[12px]`} style={{ color: MAGENTA }}>
              {result.message}
            </p>
          )}
        </>
      )}
    </div>
  );
}
