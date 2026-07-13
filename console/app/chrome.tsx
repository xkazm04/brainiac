"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import NavStatus from "@/components/NavStatus";
import { band, FONT_MONO, GROUND, MODULE_BAND, type BandKey } from "@/design/theme";

// Transitional shell for the feature pages awaiting their /prototype pass.
// Home is full-bleed and owns its own chrome. Each module's accent follows
// its EEG band (theme.ts MODULE_BAND).
export default function Chrome() {
  const pathname = usePathname();
  if (pathname === "/") return null;
  const moduleKey = pathname.split("/")[1] ?? "";
  // Keys sits on ground (0 Hz) — outside the band spectrum.
  const grounded = moduleKey === "keys";
  const bandKey: BandKey = MODULE_BAND[moduleKey] ?? "gamma";
  const accent = grounded ? GROUND : band(bandKey);
  const bandLabel = grounded ? "ground · 0 Hz" : `${bandKey} band`;
  return (
    <header
      className={`${FONT_MONO} flex items-center justify-between border-b px-6 py-4 text-xs uppercase tracking-widest`}
      style={{ borderColor: "rgba(233,237,255,0.1)" }}
    >
      <div className="flex items-center gap-3">
        <Link href="/" className="text-sm font-semibold normal-case tracking-tight text-white">
          Brainiac
        </Link>
        <span style={{ color: accent }}>
          {moduleKey ? `${moduleKey} · ${bandLabel}` : ""}
        </span>
      </div>
      <nav aria-label="Primary" className="flex items-center gap-5 text-[#e9edff]/45">
        <NavStatus />
        <Link href="/reviews" className="transition hover:text-white">
          reviews
        </Link>
        <Link href="/graph" className="transition hover:text-white">
          graph
        </Link>
        <Link href="/analytics" className="transition hover:text-white">
          analytics
        </Link>
      </nav>
    </header>
  );
}
