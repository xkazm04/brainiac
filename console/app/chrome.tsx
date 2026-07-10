"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import { band, FONT_MONO, MODULE_BAND, type BandKey } from "@/design/theme";

// Transitional shell for the feature pages awaiting their /prototype pass.
// Home is full-bleed and owns its own chrome. Each module's accent follows
// its EEG band (theme.ts MODULE_BAND).
export default function Chrome() {
  const pathname = usePathname();
  if (pathname === "/") return null;
  const moduleKey = pathname.split("/")[1] ?? "";
  const bandKey: BandKey = MODULE_BAND[moduleKey] ?? "gamma";
  const accent = band(bandKey);
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
          {moduleKey ? `${moduleKey} · ${bandKey} band` : ""}
        </span>
      </div>
      <nav aria-label="Primary" className="flex items-center gap-5 text-[#e9edff]/45">
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
