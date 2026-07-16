"use client";

/*
 * The prototype round's tab switcher — scaffolding, not product.
 *
 * Follows the pattern already proven by the cortex map's view toggle: the
 * choice lives in the URL (?variant=) so a round can be shared and argued over
 * by link, and in localStorage so a reload does not throw away the comparison
 * you were mid-way through. Baseline is always the default, so the page a
 * maintainer already knows is what loads unless someone asks for otherwise.
 *
 * The scale toggle is the whole point of THIS round. Every one of these
 * surfaces was designed against a fixture org of a dozen items and quietly
 * falls apart at organizational scale — so each variant must be inspectable at
 * both sizes, on demand, without a database. `?scale=large` swaps in a
 * deterministic several-hundred-item corpus (src/lib/seeded.ts).
 *
 * DELETE THIS FILE when the round consolidates: the winner ships without a
 * switcher, per the prototype skill's exit checklist.
 */

import { useEffect, useState } from "react";

import { FONT_MONO, LABEL } from "./theme";

export interface VariantDef {
  id: string;
  name: string;
  blurb: string;
}

/** Reads ?variant= / ?scale= once on mount, then keeps both in sync. */
export function usePrototypeState(storageKey: string, variants: VariantDef[]) {
  const [variant, setVariant] = useState<string>(variants[0].id);
  const [large, setLarge] = useState(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const fromUrl = params.get("variant");
    const stored = window.localStorage.getItem(storageKey);
    const initial = fromUrl ?? stored;
    if (initial && variants.some((v) => v.id === initial)) setVariant(initial);
    if (params.get("scale") === "large") setLarge(true);
    // variants is a module-level constant at every call site; re-running this on
    // its identity would clobber a user's choice on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storageKey]);

  const pickVariant = (id: string) => {
    setVariant(id);
    window.localStorage.setItem(storageKey, id);
    const url = new URL(window.location.href);
    url.searchParams.set("variant", id);
    window.history.replaceState(null, "", url.toString());
  };

  const pickScale = (next: boolean) => {
    setLarge(next);
    const url = new URL(window.location.href);
    if (next) url.searchParams.set("scale", "large");
    else url.searchParams.delete("scale");
    window.history.replaceState(null, "", url.toString());
  };

  return { variant, pickVariant, large, pickScale };
}

export default function PrototypeSwitcher({
  variants,
  variant,
  onVariant,
  large,
  onScale,
  smallLabel,
  largeLabel,
}: {
  variants: VariantDef[];
  variant: string;
  onVariant: (id: string) => void;
  large: boolean;
  onScale: (v: boolean) => void;
  /** e.g. "fixture · 16" */
  smallLabel: string;
  /** e.g. "org scale · 480" */
  largeLabel: string;
}) {
  const active = variants.find((v) => v.id === variant) ?? variants[0];
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
      <div
        className="pointer-events-auto flex max-w-[95vw] flex-wrap items-center gap-1 rounded-full border p-1 backdrop-blur"
        style={{
          borderColor: "rgba(233,237,255,0.14)",
          background: "rgba(12,11,18,0.88)",
        }}
      >
        <span className={`${LABEL} px-3`} style={{ color: "rgba(233,237,255,0.3)" }}>
          prototype
        </span>
        {variants.map((v) => {
          const on = v.id === variant;
          return (
            <button
              key={v.id}
              type="button"
              onClick={() => onVariant(v.id)}
              title={v.blurb}
              className={`${FONT_MONO} cursor-pointer rounded-full px-3.5 py-1.5 text-xs transition`}
              style={{
                background: on ? "rgba(233,237,255,0.10)" : "transparent",
                color: on ? "#fff" : "rgba(233,237,255,0.5)",
              }}
            >
              {v.name}
            </button>
          );
        })}

        <span className="mx-1 h-4 w-px" style={{ background: "rgba(233,237,255,0.14)" }} />

        {/* The density switch: the reason this round exists. */}
        {[
          { on: false, label: smallLabel },
          { on: true, label: largeLabel },
        ].map((s) => (
          <button
            key={String(s.on)}
            type="button"
            onClick={() => onScale(s.on)}
            className={`${FONT_MONO} cursor-pointer rounded-full px-3 py-1.5 text-[11px] transition`}
            style={{
              background: large === s.on ? "rgba(233,237,255,0.10)" : "transparent",
              color: large === s.on ? "#fff" : "rgba(233,237,255,0.45)",
            }}
          >
            {s.label}
          </button>
        ))}
      </div>

      <span className="sr-only" aria-live="polite">
        {active.name}: {active.blurb}
      </span>
    </div>
  );
}
