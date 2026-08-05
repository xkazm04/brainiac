"use client";

/*
 * The archive's one search box.
 *
 * Full-text is the SERVER's now (`?q=` over content OR title), so this is a
 * plain controlled input: Archive owns the value, debounces it, and writes the
 * URL. The box just reports the live depth — how many rows match, of the corpus
 * under the current facets — so the count answers as you type.
 *
 * The text field stays a bare <input> rather than the shared Input primitive:
 * the CHROME here belongs to the wrapper (its border, its focus-within tint, the
 * glyph and the counter sharing the same row), and Input would bring a second
 * border, background and radius inside it. Its `focus:outline-none` is gone
 * though — that utility suppressed the keyboard ring, and the unlayered
 * `:where(input):focus-visible` baseline in globals.css now draws it.
 */

import { Search, X } from "lucide-react";

import Button from "@/components/ui/Button";
import { band, FONT_MONO, INK, INK_DIM as DIM, INK_FAINT as FAINT, withAlpha } from "@/design/theme";

const VIOLET = band("delta");

export default function ArchiveSearch({
  value,
  onChange,
  matched,
  scope,
}: {
  value: string;
  onChange: (v: string) => void;
  /** Rows matching the text, as the server counts them (the filtered total). */
  matched: number;
  /** What that count is measured against — the corpus under the same facets. */
  scope: number;
}) {
  return (
    <div
      className="flex items-center gap-2.5 rounded-xl border px-3 py-2 transition focus-within:border-[var(--vio)]"
      style={{ borderColor: withAlpha(INK, 0.1), background: "rgba(255,255,255,0.02)" }}
    >
      <Search size={15} strokeWidth={1.75} color={FAINT} aria-hidden className="shrink-0" />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape" && value) onChange("");
        }}
        placeholder="search titles, claims, services, teams…"
        aria-label="Search the archive"
        className={`${FONT_MONO} min-w-0 flex-1 bg-transparent text-sm text-white placeholder:text-[#e9edff]/25`}
      />
      {value && (
        <>
          <span
            className={`${FONT_MONO} shrink-0 text-sm tabular-nums`}
            style={{ color: matched === 0 ? FAINT : VIOLET }}
          >
            {matched}/{scope}
          </span>
          {/* `icon` is the same square hit area this was hand-rolling
              (rounded + p-0.5 + the white/10 hover wash), plus the focus ring
              and the aria-label a glyph-only control cannot go without. */}
          <Button
            variant="icon"
            size="xs"
            onClick={() => onChange("")}
            aria-label="Clear search"
            className="shrink-0"
          >
            <X size={14} strokeWidth={2} color={DIM} aria-hidden />
          </Button>
        </>
      )}
    </div>
  );
}
