"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import NavStatus from "@/components/NavStatus";
import {
  PRODUCT_ROUTES,
  routeAccent,
  routeBandLabel,
  routeForPath,
} from "@/design/routes";
import { FONT_MONO } from "@/design/theme";

// Transitional shell for the feature pages awaiting their /prototype pass.
// Home is full-bleed and owns its own chrome. Nav links and the active-module
// accent both come from the shared registry (src/design/routes.ts) so the
// chrome and home nav can never disagree about which routes exist.
export default function Chrome() {
  const pathname = usePathname();
  if (pathname === "/") return null;
  const active = routeForPath(pathname);
  const accent = active ? routeAccent(active.band) : undefined;
  return (
    <header
      className={`${FONT_MONO} flex items-center justify-between border-b px-6 py-4 text-xs uppercase tracking-widest`}
      style={{ borderColor: "rgba(233,237,255,0.1)" }}
    >
      <div className="flex items-center gap-3">
        <Link href="/" className="text-sm font-semibold normal-case tracking-tight text-white">
          Brainiac
        </Link>
        {active && (
          <span style={{ color: accent }}>
            {active.segment} · {routeBandLabel(active.band)}
          </span>
        )}
      </div>
      <nav
        aria-label="Primary"
        className="flex flex-wrap items-center justify-end gap-x-5 gap-y-2 text-[#e9edff]/45"
      >
        <NavStatus />
        {PRODUCT_ROUTES.map((r) => {
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
    </header>
  );
}
