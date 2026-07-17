"use client";

/*
 * A column header that is also its own filter.
 *
 * Its options and their counts come straight from the SERVER's cross-filtered
 * facet menu — what you would get by picking this value, measured against every
 * other active filter but never against this column's own. Single-select per
 * dimension, mirroring the API (`?kind=`, `?status=`, `?team=`), and it writes
 * the URL, so opening "status" here and clicking a shelf is one navigation.
 *
 * A `team` option's `value` is a UUID and its `label` is the team name — the
 * label is shown, the value is sent.
 */

import { useEffect, useRef, useState } from "react";
import { motion, useReducedMotion } from "framer-motion";
import { Check, ChevronDown } from "lucide-react";

import { band, FONT_MONO, INK_DIM as DIM, INK_FAINT as FAINT, LABEL, withAlpha } from "@/design/theme";
import type { ArchiveFacet } from "./archive-data";

import { GlyphLegend } from "./row-icons";

const VIOLET = band("delta");
const SEL_EDGE = band("delta", 60, 0.55);
const SEL_FILL = band("delta", 60, 0.09);

export interface ColumnFilterProps {
  label: string;
  /** Draws the value's glyph beside its name — the icon columns' legend. */
  glyphs?: "status" | "kind";
  options: ArchiveFacet[];
  /** The single chosen value for this dimension, or undefined. */
  active: string | undefined;
  onToggle: (value: string) => void;
  onClear: () => void;
  /** Pins the panel's right edge to the header cell — for the last columns. */
  align?: "left" | "right";
}

export default function ColumnFilter({
  label,
  glyphs,
  options,
  active,
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

  const on = active !== undefined;

  const take = (value: string) => {
    onToggle(value);
    setOpen(false);
  };

  return (
    <div className="relative" ref={wrap}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-haspopup="true"
        aria-label={`Filter by ${label}${on ? ` — ${active} on` : ""}`}
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
            {options.length === 0 && (
              <li className={`${FONT_MONO} px-2 py-2 text-sm`} style={{ color: FAINT }}>
                no values
              </li>
            )}
            {options.map((o) => {
              const picked = active === o.value;
              return (
                <li key={o.value}>
                  <button
                    type="button"
                    role="checkbox"
                    aria-checked={picked}
                    onClick={() => take(o.value)}
                    className={`flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left transition ${
                      picked ? "" : "border-transparent hover:border-white/15 hover:bg-white/[0.04]"
                    }`}
                    style={picked ? { borderColor: SEL_EDGE, background: SEL_FILL } : undefined}
                  >
                    <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
                      {picked && <Check size={12} strokeWidth={2.5} color={VIOLET} aria-hidden />}
                    </span>
                    {glyphs && <GlyphLegend of={glyphs} value={o.value} />}
                    <span
                      className={`${FONT_MONO} min-w-0 flex-1 truncate text-sm`}
                      style={{ color: picked ? "#fff" : DIM }}
                    >
                      {o.label}
                    </span>
                    <span
                      className={`${FONT_MONO} shrink-0 text-sm tabular-nums`}
                      style={{ color: FAINT }}
                    >
                      {o.count}
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
                onClick={() => {
                  onClear();
                  setOpen(false);
                }}
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
