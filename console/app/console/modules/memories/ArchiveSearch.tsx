"use client";

/*
 * The archive's one search box.
 *
 * It replaces the facet rail's — two search inputs over one corpus is a coin
 * flip about which one is filtering the table.
 *
 * Typing already filters (debounced upstream), so nothing in the suggestion
 * list duplicates plain search: every option does something typing cannot —
 * switch a facet on, snap the query to a name the corpus actually uses, or open
 * a record directly. Each one states its verb and its count BEFORE you take it,
 * because a suggestion that silently reinterprets your question is worse than
 * no suggestion.
 */

import { useEffect, useRef, useState } from "react";
import { CornerDownLeft, Search, X } from "lucide-react";

import {
  band,
  FONT_MONO,
  INK_DIM as DIM,
  INK_FAINT as FAINT,
  LABEL,
  withAlpha,
} from "@/design/theme";

import type { Suggestion } from "./archive-index";

const VIOLET = band("delta");
const SEL_FILL = band("delta", 60, 0.12);

const LISTBOX = "archive-suggestions";

export default function ArchiveSearch({
  value,
  onChange,
  suggestions,
  onPick,
  matched,
  scope,
}: {
  value: string;
  onChange: (v: string) => void;
  suggestions: Suggestion[];
  onPick: (s: Suggestion) => void;
  /** Live result count for the text as typed — the box answers as you go. */
  matched: number;
  /** What that count is measured against (the as-of + facet scope). */
  scope: number;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const wrap = useRef<HTMLDivElement>(null);

  // A stale highlight is a mis-click waiting to happen: the list re-ranks on
  // every keystroke, so the cursor goes back to "nothing chosen" with it.
  useEffect(() => setActive(-1), [suggestions]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  const show = open && suggestions.length > 0;

  const take = (s: Suggestion) => {
    onPick(s);
    setOpen(false);
    setActive(-1);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      // First escape dismisses the list, second clears the query — the usual
      // two-stage escape, so it never destroys a search you meant to keep.
      if (show) setOpen(false);
      else if (value) onChange("");
      return;
    }
    if (!show) {
      if (e.key === "ArrowDown" && suggestions.length > 0) setOpen(true);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => (i + 1) % suggestions.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => (i <= 0 ? suggestions.length - 1 : i - 1));
    } else if (e.key === "Enter" && active >= 0) {
      e.preventDefault();
      take(suggestions[active]);
    } else if (e.key === "Enter") {
      setOpen(false);
    }
  };

  return (
    <div className="relative" ref={wrap}>
      <div
        className="flex items-center gap-2.5 rounded-xl border px-3 py-2 transition focus-within:border-[var(--vio)]"
        style={{ borderColor: withAlpha("#e9edff", 0.1), background: "rgba(255,255,255,0.02)" }}
      >
        <Search size={15} strokeWidth={1.75} color={FAINT} aria-hidden className="shrink-0" />
        <input
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={onKeyDown}
          placeholder="search titles, claims, services, teams…"
          aria-label="Search the archive"
          role="combobox"
          aria-expanded={show}
          aria-controls={LISTBOX}
          aria-autocomplete="list"
          aria-activedescendant={active >= 0 ? `${LISTBOX}-${active}` : undefined}
          className={`${FONT_MONO} min-w-0 flex-1 bg-transparent text-sm text-white outline-none placeholder:text-[#e9edff]/25`}
        />
        {value && (
          <>
            <span
              className={`${FONT_MONO} shrink-0 text-sm tabular-nums`}
              style={{ color: matched === 0 ? FAINT : VIOLET }}
            >
              {matched}/{scope}
            </span>
            <button
              type="button"
              onClick={() => onChange("")}
              aria-label="Clear search"
              className="shrink-0 rounded p-0.5 transition hover:bg-white/10"
            >
              <X size={14} strokeWidth={2} color={DIM} aria-hidden />
            </button>
          </>
        )}
      </div>

      {show && (
        <ul
          id={LISTBOX}
          role="listbox"
          aria-label="Search suggestions"
          className="absolute left-0 right-0 top-full z-40 mt-1 max-h-80 overflow-y-auto rounded-xl border p-1 shadow-2xl"
          style={{ borderColor: withAlpha(VIOLET, 0.28), background: "#0d0a16" }}
        >
          {suggestions.map((s, i) => (
            <li key={s.id}>
              <button
                type="button"
                id={`${LISTBOX}-${i}`}
                role="option"
                aria-selected={i === active}
                onClick={() => take(s)}
                onMouseEnter={() => setActive(i)}
                className="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left transition"
                style={i === active ? { background: SEL_FILL } : undefined}
              >
                {/* The verb, before the fact — and wide enough to READ it.
                    Clipped to "FILTER T…" it stopped being a promise about
                    what the option does, which is the option's whole point. */}
                <span
                  className={`${LABEL} w-[118px] shrink-0`}
                  style={{ color: s.kind === "memory" ? VIOLET : FAINT }}
                >
                  {s.action}
                </span>
                <span className="min-w-0 flex-1">
                  <span
                    className={`${FONT_MONO} block truncate text-sm`}
                    style={{ color: i === active ? "#fff" : DIM }}
                  >
                    {s.label}
                  </span>
                  {s.detail && (
                    <span className={`${LABEL} block truncate`} style={{ color: FAINT }}>
                      {s.detail}
                    </span>
                  )}
                </span>
                {s.count > 0 && (
                  <span
                    className={`${FONT_MONO} shrink-0 text-sm tabular-nums`}
                    style={{ color: FAINT }}
                  >
                    {s.count}
                  </span>
                )}
                {i === active && (
                  <CornerDownLeft size={12} strokeWidth={2} color={VIOLET} aria-hidden className="shrink-0" />
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
