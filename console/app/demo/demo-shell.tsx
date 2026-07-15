/*
 * The public showcase chrome: the ribbon, the wordmark nav, the footer.
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
 *
 * The tour strip that used to live here is now the tab bar in DemoConsole. It
 * had to move: once the modules stopped being routes, the active module became
 * page state, and chrome cannot read page state. This file went back to being a
 * server component the moment it stopped needing usePathname.
 */

import Link from "next/link";

import { band, FONT_DISPLAY, FONT_MONO, GOLD, LABEL } from "@/design/theme";

const BETA = band("beta");

export default function DemoShell({ children }: { children: React.ReactNode }) {
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

      <header className="mx-auto max-w-7xl px-6 pb-5 pt-5">
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
