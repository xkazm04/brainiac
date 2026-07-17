"use client";

/*
 * The archive's one search box.
 *
 * Full-text is the SERVER's now (`?q=` over content OR title), so this is a
 * plain controlled input: Archive owns the value, debounces it, and writes the
 * URL. The box just reports the live depth — how many rows match, of the corpus
 * under the current facets — so the count answers as you type.
 */

import { Search, X } from "lucide-react";

import { band, FONT_MONO, INK_DIM as DIM, INK_FAINT as FAINT, withAlpha } from "@/design/theme";

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
      style={{ borderColor: withAlpha("#e9edff", 0.1), background: "rgba(255,255,255,0.02)" }}
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
  );
}
