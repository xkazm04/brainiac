"use client";

import { useCallback, useEffect, useState } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import { Check, Link2 } from "lucide-react";

import { FONT_MONO } from "@/design/theme";

/*
 * Copy a link to exactly what you are looking at.
 *
 * The console's chrome could always tell you which MODULE was open; it could
 * never hand you the view. With the selection now in the URL (`?sel=`), the
 * address in the bar is a complete description of a view — so the useful
 * affordance is one button that puts it on the clipboard.
 *
 * Two deliberate details:
 *
 * - It copies `window.location.href`, not a reconstruction. Anything the page
 *   put in the URL (filters, as-of, selection, a param a module added last
 *   week) travels, and there is no second encoder to drift from the first.
 * - "copied" is confirmed only after the clipboard promise RESOLVES. An
 *   optimistic tick that lies when the write is blocked — the failure mode a
 *   previous wave already fixed on the keys page — teaches an operator that
 *   the control is decorative.
 */
export default function AddressBar() {
  const pathname = usePathname();
  const params = useSearchParams();
  const [copied, setCopied] = useState(false);
  const [failed, setFailed] = useState(false);

  // Reset the confirmation when the view changes: a tick left over from the
  // previous address would claim the wrong link is on the clipboard.
  useEffect(() => {
    setCopied(false);
    setFailed(false);
  }, [pathname, params]);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(t);
  }, [copied]);

  const copy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      setCopied(true);
      setFailed(false);
    } catch {
      // No clipboard permission (or an insecure origin). Say so rather than
      // showing a tick — the operator can still select the URL bar.
      setFailed(true);
    }
  }, []);

  return (
    <button
      type="button"
      onClick={copy}
      title="Copy a link to this exact view"
      aria-label="Copy a link to this exact view"
      className={`${FONT_MONO} inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[10px] uppercase tracking-[0.14em] transition-colors`}
      style={{
        borderColor: copied ? "rgba(120,255,190,0.4)" : "rgba(233,237,255,0.12)",
        color: copied ? "rgb(120,255,190)" : "rgba(233,237,255,0.45)",
      }}
    >
      {copied ? <Check size={11} aria-hidden /> : <Link2 size={11} aria-hidden />}
      {copied ? "copied" : failed ? "copy failed" : "copy link"}
    </button>
  );
}
