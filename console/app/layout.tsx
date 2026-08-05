import type { Metadata } from "next";
import { JetBrains_Mono, Space_Grotesk } from "next/font/google";
import type { ReactNode } from "react";

import { siteUrl } from "@/lib/site-url";

import Analytics from "@/analytics/Analytics";

import "./globals.css";

// The fused identity's type pair (see console/src/design/theme.ts):
// Space Grotesk for display, JetBrains Mono for instrument microcopy.
const spaceGrotesk = Space_Grotesk({
  subsets: ["latin"],
  variable: "--font-display",
});
const jetbrains = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-mono",
});

export const metadata: Metadata = {
  // Without a metadataBase every relative asset Next emits (og:image, twitter
  // card, canonical) stays relative — and a crawler that fetches the page from
  // its own origin cannot resolve it, so the share card is simply absent. There
  // is no production domain yet: see src/lib/site-url.ts for what to set.
  metadataBase: siteUrl(),
  // Deliberately NO title.template: every page that sets a title already spells
  // "· Brainiac" itself (see app/signup/page.tsx), so a template would render
  // "Get a project · Brainiac · Brainiac".
  title: "Brainiac Console",
  description: "Governance console for organizational AI knowledge",
  openGraph: {
    siteName: "Brainiac",
    type: "website",
    locale: "en_US",
  },
  twitter: { card: "summary_large_image" },
};

// No global chrome here: the operator header lives in the console-module
// layout (app/console/layout.tsx) and the public shells own theirs —
// route structure now draws the boundary that used to be path-matching.
export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body className={`${spaceGrotesk.variable} ${jetbrains.variable}`}>
        <main>{children}</main>
        {/*
          Product analytics: cookieless Plausible, and NOTHING at all when
          NEXT_PUBLIC_PLAUSIBLE_DOMAIN is unset (src/analytics/config.ts). It
          lives in the root layout because the funnel starts on the public
          shell, before any console chrome exists.
        */}
        <Analytics />
      </body>
    </html>
  );
}
