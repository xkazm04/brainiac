"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

// Transitional shell for the not-yet-styled feature pages. The design lab at
// `/` is full-bleed and owns its own chrome; once an identity is picked this
// component is replaced wholesale.
export default function Chrome() {
  const pathname = usePathname();
  if (pathname === "/") return null;
  return (
    <header style={{ padding: "12px 16px", borderBottom: "1px solid #ddd" }}>
      <strong>Brainiac</strong>{" "}
      <nav aria-label="Primary" style={{ display: "inline" }}>
        <Link href="/">Design lab</Link> · <Link href="/reviews">Reviews</Link> ·{" "}
        <Link href="/graph">Graph</Link> · <Link href="/analytics">Analytics</Link>
      </nav>
    </header>
  );
}
