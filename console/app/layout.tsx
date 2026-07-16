import type { Metadata } from "next";
import { JetBrains_Mono, Space_Grotesk } from "next/font/google";
import type { ReactNode } from "react";

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
  title: "Brainiac Console",
  description: "Governance console for organizational AI knowledge",
};

// No global chrome here: the operator header lives in the console-module
// layout (app/console/layout.tsx) and the public shells own theirs —
// route structure now draws the boundary that used to be path-matching.
export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body className={`${spaceGrotesk.variable} ${jetbrains.variable}`}>
        <main>{children}</main>
      </body>
    </html>
  );
}
