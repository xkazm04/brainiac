"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import NavDashboard from "@/components/NavDashboard";
import {
  PRODUCT_ROUTES,
  routeAccent,
  routeBandLabel,
  routeForPath,
} from "@/design/routes";
import { FONT_MONO } from "@/design/theme";

import { logout } from "./login/actions";

// The operator chrome, in two rows.
//
//   TOP    identity + the active module + a compact mini-dashboard. Display
//          only: nothing in this row is clickable, so the eye can read it as
//          instrumentation rather than scanning it for targets.
//   BOTTOM navigation, and only navigation. Sorted A→Z by label so the position
//          of a link is predictable from its name rather than from the order
//          someone happened to add it to the registry.
//
// Splitting them is what gives each enough room: the counts get to be numbers
// instead of one cramped badge, and the nav gets a full row as the surface list
// grows.
//
// WHERE this renders is no longer this file's problem: it is mounted only by
// the console-module layout (app/console/(modules)/layout.tsx), so it persists
// across module navigation — the mini-dashboard keeps its state while the
// content pane swaps — and it can never stack on a surface that owns its own
// header (the wave home, the public shells). Two generations of path-matching
// bugs retired by structure.

/** The console home is a destination too — nav is the only clickable row now. */
const NAV_ROUTES = [
  { path: "/console", segment: "console", label: "console", band: "gamma" as const },
  ...PRODUCT_ROUTES,
].sort((a, b) => a.label.localeCompare(b.label));

export default function Chrome() {
  const pathname = usePathname();

  const active = routeForPath(pathname);
  const accent = active ? routeAccent(active.band) : undefined;

  return (
    <header
      className={`${FONT_MONO} border-b px-6`}
      style={{ borderColor: "rgba(233,237,255,0.1)" }}
    >
      {/* ── top row: display only ─────────────────────────────────────────── */}
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 py-3">
        <div className="flex items-center gap-3">
          <span className="text-sm font-semibold tracking-tight text-white">Brainiac</span>
          {active && (
            <span
              className="text-xs uppercase tracking-widest"
              style={{ color: accent }}
            >
              {active.segment} · {routeBandLabel(active.band)}
            </span>
          )}
        </div>
        <NavDashboard />
      </div>

      {/* ── bottom row: navigation only ───────────────────────────────────── */}
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-white/[0.06] py-2.5">
        <nav
          aria-label="Primary"
          className="flex flex-wrap items-center gap-x-5 gap-y-2 text-xs uppercase tracking-widest text-[#e9edff]/45"
        >
          {NAV_ROUTES.map((r) => {
            const isActive = r.segment === active?.segment;
            return (
              <Link
                key={r.path}
                href={r.path}
                aria-current={isActive ? "page" : undefined}
                className="transition hover:text-white"
                style={isActive ? { color: routeAccent(r.band) } : undefined}
              >
                {r.label}
              </Link>
            );
          })}
        </nav>

        {/* A shared-passcode session, so this clears the cookie for this
            browser — it does not sign out a person. */}
        <form action={logout}>
          <button
            type="submit"
            className="text-xs uppercase tracking-widest text-[#e9edff]/35 transition hover:text-white"
          >
            sign out
          </button>
        </form>
      </div>
    </header>
  );
}
