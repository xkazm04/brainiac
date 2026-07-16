"use client";

/*
 * Status and Kind as glyphs.
 *
 * Both are low-cardinality enums, and as text they were eating 176px of a table
 * whose Memory column is the only one carrying a claim. As icons they cost 24px
 * and read faster — but only if they are never icon-ONLY: an unlabelled glyph is
 * an enum you have to learn, and it is nothing at all to a screen reader. So
 * every one of these carries both `aria-label` (spoken) and `title` (hover), and
 * the table header spells the vocabulary out in a legend.
 *
 * Status keeps the colour semantics the rest of the module already teaches, via
 * statusTone: canonical = gamma/gold (the binding band — a claim the org stands
 * behind), deprecated = dim (it was true then), raw/candidate = delta/violet
 * (in flight), rejected = magenta (the contradiction accent).
 */

import {
  Ban,
  BadgeCheck,
  CircleDashed,
  CircleDot,
  Gavel,
  History,
  Info,
  Tag,
  TriangleAlert,
  Wrench,
  type LucideIcon,
} from "lucide-react";

import { band, INK_DIM } from "@/design/theme";

import { statusTone } from "./MemoryInspector";

const VIOLET = band("delta");

interface Glyph {
  Icon: LucideIcon;
  /** Spoken and hovered — the word the icon stands in for. */
  say: string;
}

const STATUS: Record<string, Glyph> = {
  canonical: { Icon: BadgeCheck, say: "canonical — the org stands behind this" },
  // The archive's own phrasing for a superseded claim, kept verbatim: it was
  // true then, and the as-of scrubber exists to go back and see it.
  deprecated: { Icon: History, say: "deprecated — was true then" },
  candidate: { Icon: CircleDot, say: "candidate — awaiting review" },
  raw: { Icon: CircleDashed, say: "raw — captured, not yet triaged" },
  rejected: { Icon: Ban, say: "rejected — reviewed and turned down" },
};

const KIND: Record<string, Glyph> = {
  fact: { Icon: Info, say: "fact" },
  decision: { Icon: Gavel, say: "decision" },
  pitfall: { Icon: TriangleAlert, say: "pitfall" },
  howto: { Icon: Wrench, say: "howto" },
};

/** The enums are open on the wire (plain strings), so an unknown value must
 *  still render as itself rather than vanish or crash. */
const fallback = (value: string): Glyph => ({ Icon: Tag, say: value });

export const statusGlyph = (v: string): Glyph => STATUS[v] ?? fallback(v);
export const kindGlyph = (v: string): Glyph => KIND[v] ?? fallback(v);

/**
 * The label rides on a wrapping span rather than on the svg: `title` on a span
 * is the tooltip a mouse gets, `role="img"` + `aria-label` is the word a screen
 * reader gets, and the glyph itself goes aria-hidden so the two never double up.
 */
function Glyphed({ Icon, say, color }: Glyph & { color: string }) {
  return (
    <span className="inline-flex" role="img" aria-label={say} title={say}>
      <Icon size={15} strokeWidth={1.75} color={color} aria-hidden />
    </span>
  );
}

export function StatusIcon({ status }: { status: string }) {
  const g = statusGlyph(status);
  return <Glyphed {...g} say={`status: ${g.say}`} color={statusTone(status)} />;
}

export function KindIcon({ kind }: { kind: string }) {
  const g = kindGlyph(kind);
  return <Glyphed {...g} say={`kind: ${g.say}`} color={VIOLET} />;
}

/** The same glyph inline with its word — for the filter menus and the legend,
 *  where the icon is being TAUGHT rather than relied on. */
export function GlyphLegend({ of, value }: { of: "status" | "kind"; value: string }) {
  const { Icon } = of === "status" ? statusGlyph(value) : kindGlyph(value);
  return (
    <Icon
      size={14}
      strokeWidth={1.75}
      aria-hidden
      color={of === "status" ? statusTone(value) : INK_DIM}
      className="shrink-0"
    />
  );
}
