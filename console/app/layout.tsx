import type { Metadata } from "next";
import Link from "next/link";
import type { ReactNode } from "react";

export const metadata: Metadata = {
  title: "Brainiac Console",
  description: "Governance console for organizational AI knowledge",
};

// Deliberately unstyled semantic shell — the visual identity pass replaces
// this chrome wholesale; keep structure, not looks.
export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>
        <header>
          <strong>Brainiac</strong>
          <nav aria-label="Primary">
            <Link href="/reviews">Reviews</Link> · <Link href="/graph">Graph</Link> ·{" "}
            <Link href="/analytics">Analytics</Link>
          </nav>
        </header>
        <main>{children}</main>
      </body>
    </html>
  );
}
