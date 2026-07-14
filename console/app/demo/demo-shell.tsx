"use client";

/*
 * The public showcase shell.
 *
 * Every surface under /demo renders the SAME components an operator sees, fed
 * the DEMO_* fixtures. Those fixtures all carry `live: false`, which is what
 * makes this safe rather than merely unauthenticated: each component already
 * degrades on that flag — it synthesizes drill-in detail client-side instead of
 * calling the gated /api routes, and it disables its write controls. No API
 * token is ever used on this subtree.
 *
 * The ribbon is unconditional and not dismissible: a visitor must never mistake
 * the Meridian fixture org for their own data.
 */

import Link from "next/link";
import { usePathname } from "next/navigation";

import { band, FONT_DISPLAY, FONT_MONO, GOLD, LABEL } from "@/design/theme";

const BETA = band("beta");

const TOUR = [
  { path: "/demo", label: "overview", blurb: "governance health" },
  { path: "/demo/reviews", label: "the gate", blurb: "promotions awaiting a human" },
  { path: "/demo/disputes", label: "disputes", blurb: "contradictions, adjudicated" },
  { path: "/demo/graph", label: "graph", blurb: "canonical entities" },
  { path: "/demo/memories", label: "archive", blurb: "the corpus, as-of any date" },
  { path: "/demo/health", label: "health", blurb: "is the knowledge rotting?" },
  { path: "/demo/divergence", label: "standards", blurb: "same problem, solved two ways" },
];

export default function DemoShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();

  return (
    <div className={FONT_DISPLAY}>
      {/* the ribbon — always on, never dismissible */}
      <div
        className="border-b"
        style={{ borderColor: band("beta", 68, 0.25), background: band("beta", 60, 0.06) }}
      >
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-6 py-2.5">
          <span className={LABEL} style={{ color: BETA }}>
            example data · org “meridian” — a synthetic fintech, invented for this demo
          </span>
          <span className={`${FONT_MONO} text-[11px]`} style={{ color: "rgba(233,237,255,0.4)" }}>
            nothing here is real · no sign-in required
          </span>
        </div>
      </div>

      {/* the showcase nav */}
      <header className="mx-auto max-w-7xl px-6 pt-5">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <Link href="/" className="flex items-center gap-3">
            <span className="text-lg font-semibold tracking-tight text-white">Brainiac</span>
            <span className={LABEL} style={{ color: GOLD }}>
              γ · the demo org
            </span>
          </Link>
          <nav
            className={`${FONT_MONO} flex flex-wrap items-center gap-x-5 gap-y-2 text-xs uppercase tracking-widest`}
            style={{ color: "rgba(233,237,255,0.45)" }}
          >
            <Link href="/pitch" className="transition hover:text-[#f3c74f]">
              the pitch
            </Link>
            <Link href="/kb" className="transition hover:text-[#f3c74f]">
              knowledge base
            </Link>
            <Link href="/console" className="transition hover:text-[#f3c74f]">
              console →
            </Link>
          </nav>
        </div>

        {/* the tour strip */}
        <div className="mt-5 flex flex-wrap gap-2 border-b border-white/[0.08] pb-4">
          {TOUR.map((t) => {
            const active = pathname === t.path;
            return (
              <Link
                key={t.path}
                href={t.path}
                title={t.blurb}
                aria-current={active ? "page" : undefined}
                className={`${FONT_MONO} group rounded-lg border px-3 py-2 text-xs transition`}
                style={{
                  borderColor: active ? "hsla(46,90%,68%,0.45)" : "rgba(233,237,255,0.10)",
                  background: active ? "hsla(46,90%,60%,0.07)" : "transparent",
                  color: active ? GOLD : "rgba(233,237,255,0.6)",
                }}
              >
                <span className="font-medium">{t.label}</span>
                {/* The blurb is a nicety; it must not push the tour onto a
                    second line and orphan a tab. Below xl it goes to the title. */}
                <span
                  className="ml-2 hidden text-[10px] xl:inline"
                  style={{ color: "rgba(233,237,255,0.3)" }}
                >
                  {t.blurb}
                </span>
              </Link>
            );
          })}
        </div>
      </header>

      {children}

      <footer
        className={`${LABEL} mx-auto mt-16 flex max-w-7xl flex-wrap items-center justify-between gap-3 border-t border-white/10 px-6 py-8`}
        style={{ color: "rgba(233,237,255,0.32)" }}
      >
        <span>brainiac · constructive by design</span>
        <span>
          every surface here is the real console component, running on the fixture org
        </span>
        <Link href="/pitch" className="transition hover:text-[#f3c74f]">
          → why this exists
        </Link>
      </footer>
    </div>
  );
}
