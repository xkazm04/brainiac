"use client";

import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { KeyRound, LogOut } from "lucide-react";

import NavDashboard from "@/components/NavDashboard";
import {
  NAV_GROUPS,
  PRODUCT_ROUTES,
  parseModule,
  routeAccent,
  routeBandLabel,
  type NavGroup,
  type ProductRoute,
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
// WHERE this renders is no longer this file's problem: it is mounted only by the
// console layout (app/console/layout.tsx), so it persists across module swaps —
// the mini-dashboard keeps its state while the pane below changes — and it can
// never stack on a surface that owns its own header (the public shells). Two
// generations of path-matching bugs retired by structure.

// Grouped, then A→Z by label within a group, so a link's position is predictable
// from its name rather than from the order someone added it to the registry.
// There is no separate "console" entry any more: /console IS a module
// (analytics), so listing it would be a second link to a tab already in this row.
const byLabel = (a: ProductRoute, b: ProductRoute) => a.label.localeCompare(b.label);

const GROUPED: Record<NavGroup, ProductRoute[]> = {
  memory: PRODUCT_ROUTES.filter((r) => r.group === "memory").sort(byLabel),
  knowledge: PRODUCT_ROUTES.filter((r) => r.group === "knowledge").sort(byLabel),
  library: PRODUCT_ROUTES.filter((r) => r.group === "library").sort(byLabel),
};

/** Access lives on the right, with the way out — never in a module group. */
const KEYS_ROUTE = PRODUCT_ROUTES.find((r) => r.segment === "keys");

export default function Chrome() {
  const pathname = usePathname();
  const params = useSearchParams();

  // The module is a query param now, not a path segment. The one exception is
  // the document sub-route (/console/docs/<slug>), which is a real page and
  // still reports itself as the pages module so the nav does not go blank.
  const active = pathname.startsWith("/console/docs")
    ? PRODUCT_ROUTES.find((r) => r.segment === "docs")
    : PRODUCT_ROUTES.find((r) => r.segment === parseModule(params.get("m") ?? undefined));
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
      <div className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-t border-white/[0.06] py-2.5">
        <nav
          aria-label="Primary"
          className="flex flex-wrap items-stretch gap-x-5 gap-y-3 text-xs uppercase tracking-widest text-[#e9edff]/45"
        >
          {NAV_GROUPS.map((g, i) => (
            <div key={g.id} className="flex items-stretch">
              {/* The divider belongs to the group that follows it, so it never
                  dangles at the end of the row when the nav wraps. */}
              {i > 0 && (
                <span
                  aria-hidden
                  className="mr-5 w-px self-stretch"
                  style={{ background: "rgba(233,237,255,0.12)" }}
                />
              )}
              <div className="flex flex-col gap-1">
                <span
                  className="text-[10px] tracking-[0.2em]"
                  style={{ color: "rgba(233,237,255,0.25)" }}
                >
                  {g.label}
                </span>
                <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
                  {GROUPED[g.id].map((r) => {
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
                </div>
              </div>
            </div>
          ))}
        </nav>

        {/* Access, not knowledge: the key and the way out live together, apart
            from the modules — so "sign out" is never one slip away from a tab. */}
        <div className="flex items-center gap-4 text-xs uppercase tracking-widest">
          {KEYS_ROUTE && (
            <Link
              href={KEYS_ROUTE.path}
              aria-current={active?.segment === "keys" ? "page" : undefined}
              className="flex items-center gap-1.5 transition hover:text-white"
              style={{
                color:
                  active?.segment === "keys"
                    ? routeAccent(KEYS_ROUTE.band)
                    : "rgba(233,237,255,0.45)",
              }}
            >
              <KeyRound size={13} strokeWidth={1.75} aria-hidden />
              {KEYS_ROUTE.label}
            </Link>
          )}

          {/* A shared-passcode session, so this clears the cookie for this
              browser — it does not sign out a person. */}
          <form action={logout}>
            <button
              type="submit"
              className="flex items-center gap-1.5 text-[#e9edff]/35 uppercase tracking-widest transition hover:text-white"
            >
              <LogOut size={13} strokeWidth={1.75} aria-hidden />
              sign out
            </button>
          </form>
        </div>
      </div>
    </header>
  );
}
