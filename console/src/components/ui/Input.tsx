/*
 * The console's text fields.
 *
 * Same story as Button: every field spelled out its own border, its own
 * placeholder tint, and — in every single case — `focus:outline-none`, which
 * removes the keyboard ring along with the pointer one. That utility is gone
 * from all of these; the ring comes from `.focus-ring`, which sits outside
 * Tailwind's cascade layers and therefore cannot be suppressed by a stray
 * `outline-none` further down a className.
 *
 * `label` is optional but strongly preferred — passing it wires htmlFor/id and
 * gives the field an accessible name without the caller thinking about it. A
 * field with neither a label nor an aria-label is the one thing this primitive
 * cannot fix for you.
 */

import { useId } from "react";
import type {
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";

import { FONT_MONO, LABEL, INK_FAINT } from "@/design/theme";

import { fieldClass, fieldStyle, type ControlSize } from "./control-styles";

type Common = {
  label?: ReactNode;
  size?: ControlSize;
  invalid?: boolean;
  /** Rendered under the field; announced via aria-describedby. */
  hint?: ReactNode;
  /** Off when the caller supplies its own width (`w-72`, `flex-1`, …). */
  fullWidth?: boolean;
};

function Field({
  label,
  hint,
  htmlFor,
  hintId,
  children,
}: {
  label?: ReactNode;
  hint?: ReactNode;
  htmlFor: string;
  hintId: string;
  children: ReactNode;
}) {
  if (!label && !hint) return <>{children}</>;
  return (
    <div className="w-full">
      {label && (
        <label className={LABEL} style={{ color: INK_FAINT }} htmlFor={htmlFor}>
          {label}
        </label>
      )}
      <div className={label ? "mt-2" : undefined}>{children}</div>
      {hint && (
        <p
          id={hintId}
          className={`${FONT_MONO} mt-1.5 text-[11px]`}
          style={{ color: INK_FAINT }}
        >
          {hint}
        </p>
      )}
    </div>
  );
}

export type InputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "size"> &
  Common;

export function Input({
  label,
  hint,
  size = "sm",
  invalid,
  fullWidth = true,
  className,
  style,
  id,
  ...rest
}: InputProps) {
  const auto = useId();
  const fieldId = id ?? auto;
  const hintId = `${fieldId}-hint`;
  return (
    <Field label={label} hint={hint} htmlFor={fieldId} hintId={hintId}>
      <input
        {...rest}
        id={fieldId}
        aria-invalid={invalid || undefined}
        aria-describedby={hint ? hintId : rest["aria-describedby"]}
        className={fieldClass(
          size,
          `${FONT_MONO} ${className ?? ""}`.trim(),
          fullWidth,
        )}
        style={{ ...fieldStyle(invalid), ...style }}
      />
    </Field>
  );
}

export type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & Common;

export function Textarea({
  label,
  hint,
  size = "sm",
  invalid,
  fullWidth = true,
  className,
  style,
  id,
  ...rest
}: TextareaProps) {
  const auto = useId();
  const fieldId = id ?? auto;
  const hintId = `${fieldId}-hint`;
  return (
    <Field label={label} hint={hint} htmlFor={fieldId} hintId={hintId}>
      <textarea
        {...rest}
        id={fieldId}
        aria-invalid={invalid || undefined}
        aria-describedby={hint ? hintId : rest["aria-describedby"]}
        className={fieldClass(
          size,
          `${FONT_MONO} leading-relaxed ${className ?? ""}`.trim(),
          fullWidth,
        )}
        style={{ ...fieldStyle(invalid), ...style }}
      />
    </Field>
  );
}

export type SelectProps = Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> &
  Common;

export function Select({
  label,
  hint,
  size = "sm",
  invalid,
  fullWidth = true,
  className,
  style,
  id,
  ...rest
}: SelectProps) {
  const auto = useId();
  const fieldId = id ?? auto;
  const hintId = `${fieldId}-hint`;
  return (
    <Field label={label} hint={hint} htmlFor={fieldId} hintId={hintId}>
      <select
        {...rest}
        id={fieldId}
        aria-invalid={invalid || undefined}
        aria-describedby={hint ? hintId : rest["aria-describedby"]}
        // The native popup renders on the OS surface, so it needs a real
        // opaque background rather than the translucent panel wash.
        className={fieldClass(
          size,
          `${FONT_MONO} bg-[#0a090f] ${className ?? ""}`.trim(),
          fullWidth,
        )}
        style={{ ...fieldStyle(invalid), ...style }}
      />
    </Field>
  );
}

export default Input;
