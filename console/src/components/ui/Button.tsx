/*
 * The console's button.
 *
 * There was no shared one: 107 `<button>` elements each re-derived their own
 * capsule, their own disabled opacity, their own accent arithmetic — and nine
 * focus declarations between all of them, which is the part that mattered. A
 * keyboard operator moving through the review queue could not see where they
 * were. Every variant here carries the ring (FOCUS → `.focus-ring`), so it is
 * no longer something a call site can forget.
 *
 * It stays a thin primitive on purpose. It does not own loading state, does not
 * wrap useTransition, does not render icons for you — the modules already have
 * their own opinions about all three, and a primitive that took them over would
 * be rewritten by the second call site. It owns shape, size, accent and focus.
 *
 * `accent` is a theme colour (GOLD, band("alpha"), MAGENTA …), not a semantic
 * name. The band system already assigns meaning per module, so a fixed
 * `intent="danger"` palette would fight it: on the disputes bench the
 * constructive answer is gamma and on governance it is alpha, and both are
 * "primary" in their own surface.
 */

import type { ButtonHTMLAttributes } from "react";

import { FONT_MONO } from "@/design/theme";

import {
  controlClass,
  controlStyle,
  type ControlSize,
  type ControlVariant,
} from "./control-styles";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ControlVariant;
  size?: ControlSize;
  /** A theme colour. Omit for the neutral ink treatment. */
  accent?: string;
  /** Toggle/pressed state. Also emits aria-pressed, so it is announced. */
  selected?: boolean;
  /** Set false when the caller supplies its own type scale/family. */
  mono?: boolean;
  /** Drop the size bucket's padding and supply your own in `className`.
   *  Required when your padding is SMALLER than the bucket's — see
   *  controlClass for why a plain className override loses that fight. */
  unpadded?: boolean;
};

export default function Button({
  variant = "pill",
  size = "sm",
  accent,
  selected,
  mono = true,
  unpadded = false,
  className,
  style,
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      {...rest}
      // Never let a call site forget this. A <button> inside a <form> defaults
      // to submit, and several of these live in forms that own a server action.
      type={type}
      aria-pressed={selected === undefined ? undefined : selected}
      className={controlClass(
        variant,
        size,
        [mono ? FONT_MONO : "", className ?? ""].filter(Boolean).join(" "),
        unpadded,
      )}
      style={{ ...controlStyle(variant, accent, selected ?? false), ...style }}
    />
  );
}
