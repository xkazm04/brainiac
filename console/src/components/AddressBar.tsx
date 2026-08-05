"use client";

import { useCallback, useEffect, useState } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import { Check, Link2 } from "lucide-react";

import Button from "@/components/ui/Button";
import { band } from "@/design/theme";

/** Confirmation tone: beta, the active-recall band, same as the nav's live dot.
 *  It was a bare rgb(120,255,190) here — the same colour, spelled by hand. */
const MINT = band("beta");

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
    <Button
      variant="pill"
      size="xs"
      unpadded
      accent={copied ? MINT : undefined}
      onClick={copy}
      title="Copy a link to this exact view"
      aria-label="Copy a link to this exact view"
      className="inline-flex items-center gap-1.5 px-2 py-1 uppercase tracking-[0.14em]"
      // A tab, not a capsule: this sits in the instrumentation row, where a
      // pill would read as one more status chip. Radius and type size come
      // through style rather than `rounded-md` / `text-[10px]` classes because
      // Tailwind resolves competing utilities by stylesheet order, not
      // className order — the override would win only by luck.
      style={{ borderRadius: "0.375rem", fontSize: "10px" }}
    >
      {copied ? <Check size={11} aria-hidden /> : <Link2 size={11} aria-hidden />}
      {copied ? "copied" : failed ? "copy failed" : "copy link"}
    </Button>
  );
}
