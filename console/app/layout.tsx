import type { Metadata } from "next";
import { JetBrains_Mono, Space_Grotesk } from "next/font/google";
import type { ReactNode } from "react";

import Chrome from "./chrome";

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

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body className={`${spaceGrotesk.variable} ${jetbrains.variable}`}>
        <Chrome />
        <main>{children}</main>
      </body>
    </html>
  );
}
