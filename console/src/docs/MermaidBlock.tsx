"use client";

/*
 * The mermaid client island (KB-PLAN's deferred "mermaid rendering in the
 * console reader", closing D9's read side).
 *
 * The composed page's diagrams are deterministic — every arrow is a row in
 * `edges`, compiled by code (crates/brainiac-pipeline/src/compose.rs) — but the
 * SOURCE still travels inside a page a model assembled, so it is untrusted text
 * everywhere except right here. This component is the single sanctioned place
 * where that text becomes markup, and only through mermaid itself under
 * `securityLevel: "strict"` (labels sanitized, scripts and click handlers
 * refused). The library is lazily imported on the client, so the sanitized
 * server-rendered markdown path (src/docs/markdown.ts) stays dependency-free
 * and nothing is fetched from a CDN — the bundle ships with the console.
 *
 * Failure is a downgrade, never a crash: until the diagram renders — and
 * forever, if it will not parse — the reader shows the same verbatim code
 * fence it always showed.
 */

import { useEffect, useState, type ReactNode } from "react";

import { BG, BORDER, INK, withAlpha } from "@/design/theme";

/** initialize() is global to the library; run it once per session. */
let initialized = false;
/** mermaid.render wants a DOM-unique id; a module counter is enough. */
let seq = 0;

export interface MermaidBlockProps {
  /** Untrusted diagram source — the body of a ```mermaid fence, verbatim. */
  source: string;
  /** What to show while loading and whenever rendering fails: the reader
   *  passes its own code-fence presentation, so degradation is invisible. */
  fallback: ReactNode;
}

export default function MermaidBlock({ source, fallback }: MermaidBlockProps) {
  const [svg, setSvg] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const id = `docs-mermaid-${seq++}`;
    (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        if (!initialized) {
          mermaid.initialize({
            startOnLoad: false,
            securityLevel: "strict",
            // The console is deliberately dark-only (src/design/theme.ts), so
            // the diagram is tuned to that one palette rather than toggled.
            theme: "dark",
            themeVariables: {
              background: BG,
              primaryColor: "#14121d", // node fill — panel-on-dark
              primaryTextColor: INK,
              primaryBorderColor: withAlpha(INK, 0.28),
              lineColor: withAlpha(INK, 0.55),
              edgeLabelBackground: BG,
              fontFamily: "var(--font-mono), ui-monospace, monospace",
            },
            // SVG text labels only — no HTML label pathway at all.
            flowchart: { htmlLabels: false },
          });
          initialized = true;
        }
        const out = await mermaid.render(id, source);
        if (!cancelled) setSvg(out.svg);
      } catch {
        // A failed parse can leave mermaid's scratch nodes behind; clear them,
        // then stay on the fallback fence (svg remains null).
        document.getElementById(id)?.remove();
        document.getElementById(`d${id}`)?.remove();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [source]);

  if (svg === null) return <>{fallback}</>;
  return (
    <div
      role="img"
      aria-label="relationship diagram"
      className="my-5 overflow-x-auto rounded-lg border p-4"
      style={{ background: "rgba(255,255,255,0.02)", borderColor: BORDER }}
      // The one sanctioned innerHTML on the docs surface: this string is not
      // page content but mermaid's own output, produced from the untrusted
      // source under securityLevel "strict".
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
