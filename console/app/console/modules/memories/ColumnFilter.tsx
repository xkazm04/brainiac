"use client";

/*
 * A column header that is also its own filter.
 *
 * Same contract as the facet rail's channels, and deliberately the same STATE:
 * opening "status" here and clicking "canonical" in the rail are one selection,
 * not two competing ones. Two filter UIs over two stores is how a table starts
 * lying about what it is showing.
 *
 * The counts are cross-filtered — what you would get by picking this value,
 * measured against every other active filter but never against this column's
 * own. A count that ignores the rest of the query promises 88 rows and hands
 * you three.
 */

import { useEffect, useRef, useState } from "react";
import { motion, useReducedMotion } from "framer-motion";
import { Check, ChevronDown } from "lucide-react";

import { band, FONT_MONO, INK_DIM as DIM, INK_FAINT as FAINT, LABEL, withAlpha } from "@/design/theme";

import { GlyphLegend } from "./row-icons";

const VIOLET = band("delta");
const SEL_EDGE = band("delta", 60, 0.55);
const SEL_FILL = band("delta", 60, 0.09);

export interface ColumnFilterProps {
  label: string;
  /** Draws the value's glyph beside its name — the icon columns' legend. */
  glyphs?: "status" | "kind";
  values: string[];
  counts: Map<string, number>;
  chosen: string[];
  onToggle: (v: string) => void;
  onClear: () => void;
  /** Pins the panel's right edge to the header cell — for the last columns. */
  align?: "left" | "right";
}

export default function ColumnFilter({
  label,
  glyphs,
  values,
  counts,
  chosen,
  onToggle,
  onClear,
  align = "left",
}: ColumnFilterProps) {
  const reduce = !!useReducedMotion();
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);

  // Escape closes from anywhere inside; a pointerdown outside closes too. Both
  // only while open — no listener is attached for the 99% of the session when
  // this menu is shut.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onDown = (e: PointerEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onDown);
    };
  }, [open]);

  const on = chosen.length > 0;

  return (
    <div className="relative" ref={wrap}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-haspopup="true"
        aria-label={`Filter by ${label}${on ? ` — ${chosen.length} on: ${chosen.join(", ")}` : ""}`}
        className={`${LABEL} flex w-full items-center gap-1 rounded px-1 py-0.5 transition hover:text-white`}
        style={{ color: on ? VIOLET : FAINT }}
      >
        <span className="truncate">{label}</span>
        {on && (
          <span
            aria-hidden
            className="h-1 w-1 shrink-0 rounded-full"
            style={{ background: VIOLET }}
          />
        )}
        <ChevronDown size={11} strokeWidth={2} aria-hidden className="shrink-0 opacity-60" />
      </button>

      {open && (
        <motion.div
          initial={reduce ? false : { opacity: 0, y: -4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.12 }}
          role="group"
          aria-label={`${label} filter`}
          className={`absolute top-full z-30 mt-1 w-56 overflow-hidden rounded-lg border shadow-xl ${
            align === "right" ? "right-0" : "left-0"
          }`}
          style={{ borderColor: withAlpha(VIOLET, 0.28), background: "#0d0a16" }}
        >
          <ul className="max-h-64 overflow-y-auto p-1">
            {values.length === 0 && (
              <li className={`${FONT_MONO} px-2 py-2 text-sm`} style={{ color: FAINT }}>
                no values
              </li>
            )}
            {values.map((v) => {
              const n = counts.get(v) ?? 0;
              const picked = chosen.includes(v);
              // A value that is off and would return nothing is shown, dimmed,
              // and inert — a shelf that vanishes mid-narrowing is a broken
              // catalog; one that reads zero is an answer.
              const mute = n === 0 && !picked;
              return (
                <li key={v}>
                  <button
                    type="button"
                    role="checkbox"
                    aria-checked={picked}
                    disabled={mute}
                    onClick={() => onToggle(v)}
                    className={`flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left transition ${
                      mute
                        ? "cursor-default border-transparent opacity-40"
                        : picked
                          ? ""
                          : "border-transparent hover:border-white/15 hover:bg-white/[0.04]"
                    }`}
                    style={picked ? { borderColor: SEL_EDGE, background: SEL_FILL } : undefined}
                  >
                    <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
                      {picked && <Check size={12} strokeWidth={2.5} color={VIOLET} aria-hidden />}
                    </span>
                    {glyphs && <GlyphLegend of={glyphs} value={v} />}
                    <span
                      className={`${FONT_MONO} min-w-0 flex-1 truncate text-sm`}
                      style={{ color: picked ? "#fff" : DIM }}
                    >
                      {v}
                    </span>
                    <span
                      className={`${FONT_MONO} shrink-0 text-sm tabular-nums`}
                      style={{ color: FAINT }}
                    >
                      {n}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          {on && (
            <div className="border-t border-white/10 p-1">
              <button
                type="button"
                onClick={onClear}
                className={`${FONT_MONO} w-full rounded-md px-2 py-1.5 text-left text-sm transition hover:bg-white/[0.04]`}
                style={{ color: DIM }}
              >
                clear {label} filter
              </button>
            </div>
          )}
        </motion.div>
      )}
    </div>
  );
}
