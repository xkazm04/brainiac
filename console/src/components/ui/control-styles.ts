/*
 * The shape of every control in the console, as data.
 *
 * Kept separate from the React components for one reason: it is pure, so it is
 * the part a test can actually assert on. The vitest suite here runs in a node
 * environment with no DOM and no testing-library, so a test can prove that
 * every variant carries the focus ring and that an accent reaches border and
 * text — which is the whole contract these primitives exist to guarantee — but
 * could not render a component to find that out.
 *
 * Variants are named after what the console already does, not after a generic
 * button taxonomy. They were derived by reading the 107 hand-rolled controls:
 *
 *   solid  the one loud affordance on a page — gold fill, dark ink (login
 *          unlock, signup continue). At most one per view.
 *   pill   the console's workhorse: bordered capsule, accent-tinted. Approve /
 *          reject / coexist / dismiss, filters, pagination, mode switches.
 *   quiet  text-only. Inline "clear", "expand all", "reset" — actions that must
 *          not compete with the data they sit next to.
 *   icon   a square hit area around a glyph. Always needs an aria-label.
 *   row    a full-width, left-aligned selectable list row. Not decorative
 *          chrome — a real control, and the family the old code most often
 *          left invisible to the keyboard.
 */

import type { CSSProperties } from "react";

import { BG, FOCUS, INK, INK_DIM, withAlpha } from "@/design/theme";

export type ControlVariant = "solid" | "pill" | "quiet" | "icon" | "row";
export type ControlSize = "xs" | "sm" | "md";

/** Shared by every variant. `transition` is safe: globals.css flattens it under
 *  prefers-reduced-motion. */
const BASE = `${FOCUS} cursor-pointer transition disabled:cursor-not-allowed disabled:opacity-40`;

const SHAPE: Record<ControlVariant, string> = {
  solid: "rounded-full font-semibold hover:brightness-110",
  pill: "rounded-full border hover:bg-white/5",
  quiet: "hover:text-white",
  icon: "grid place-items-center rounded hover:bg-white/10",
  row: "flex w-full items-center gap-2 rounded-md text-left hover:bg-white/[0.03]",
};

/** Padding + type scale. `row` and `quiet` opt out of the capsule padding. */
const SIZING: Record<ControlVariant, Record<ControlSize, string>> = {
  solid: {
    xs: "px-4 py-1.5 text-xs",
    sm: "px-5 py-2 text-sm",
    md: "px-6 py-3 text-sm",
  },
  pill: {
    xs: "px-2.5 py-1 text-[11px]",
    sm: "px-3.5 py-1.5 text-sm",
    md: "px-5 py-2 text-sm",
  },
  quiet: { xs: "text-[11px]", sm: "text-xs", md: "text-sm" },
  icon: { xs: "p-0.5", sm: "p-1", md: "p-1.5" },
  row: {
    xs: "px-2 py-1 text-[12px]",
    sm: "px-2.5 py-1.5 text-sm",
    md: "px-3 py-2.5 text-sm",
  },
};

export function controlClass(
  variant: ControlVariant,
  size: ControlSize,
  extra?: string,
): string {
  return [BASE, SHAPE[variant], SIZING[variant][size], extra]
    .filter(Boolean)
    .join(" ");
}

/**
 * Colour, from one accent.
 *
 * Every call site in the old code spelled this out by hand as
 * `{ borderColor: withAlpha(X, 0.4), color: X }`, and the ones that forgot
 * `withAlpha` on an hsl accent emitted invalid CSS (see the withAlpha docblock
 * in theme.ts). Deriving it once removes both the repetition and that trap.
 *
 * `selected` is the pressed state of a toggle pill: the tint comes forward
 * rather than the border changing shape, so the row does not reflow.
 */
export function controlStyle(
  variant: ControlVariant,
  accent: string | undefined,
  selected = false,
): CSSProperties {
  if (variant === "solid") {
    return {
      background: accent ?? INK,
      color: BG,
      boxShadow: accent ? `0 0 32px ${withAlpha(accent, 0.35)}` : undefined,
    };
  }
  const tone = accent ?? INK_DIM;
  if (variant === "quiet" || variant === "icon") {
    return { color: selected ? (accent ?? INK) : tone };
  }
  // pill + row: a tinted edge, tinted text, and a wash only when selected.
  return {
    borderColor: withAlpha(accent ?? INK, selected ? 0.65 : accent ? 0.4 : 0.2),
    color: accent ?? (selected ? INK : INK_DIM),
    background: selected ? withAlpha(accent ?? INK, 0.1) : undefined,
  };
}

/** Field chrome, shared by Input / Textarea / Select.
 *
 *  Width is NOT in here. Tailwind resolves conflicting utilities by stylesheet
 *  order, not className order, so a baked-in `w-full` would beat a caller's
 *  `w-72` about half the time depending on how the classes happen to be
 *  emitted — a bug that only shows up on one field. `fullWidth` is an explicit
 *  flag instead. */
export const FIELD_BASE = `${FOCUS} bg-white/[0.03] text-white transition placeholder:text-[#e9edff]/30 disabled:opacity-50`;

export const FIELD_SIZING: Record<ControlSize, string> = {
  xs: "rounded-md px-3 py-1.5 text-xs",
  sm: "rounded-lg px-3.5 py-2 text-sm",
  md: "rounded-lg px-4 py-3 text-sm",
};

export function fieldClass(
  size: ControlSize,
  extra?: string,
  fullWidth = true,
): string {
  return [FIELD_BASE, "border", fullWidth ? "w-full" : "", FIELD_SIZING[size], extra]
    .filter(Boolean)
    .join(" ");
}

export function fieldStyle(invalid = false): CSSProperties {
  return {
    borderColor: invalid ? "rgba(255,93,162,0.55)" : "rgba(233,237,255,0.14)",
  };
}
