import type { Metadata } from "next";
import {
  Fraunces,
  IBM_Plex_Mono,
  Inter,
  JetBrains_Mono,
  Space_Grotesk,
} from "next/font/google";
import type { ReactNode } from "react";

import Chrome from "./chrome";

import "./globals.css";

// One font pair per design philosophy — exposed as CSS variables so each
// variant binds its own vocabulary without loading fonts client-side.
const spaceGrotesk = Space_Grotesk({
  subsets: ["latin"],
  variable: "--font-synapse-display",
});
const jetbrains = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-synapse-mono",
});
const fraunces = Fraunces({
  subsets: ["latin"],
  variable: "--font-cortex-display",
});
const inter = Inter({ subsets: ["latin"], variable: "--font-cortex-text" });
const plexMono = IBM_Plex_Mono({
  weight: ["400", "500", "600"],
  subsets: ["latin"],
  variable: "--font-vault-mono",
});

export const metadata: Metadata = {
  title: "Brainiac Console",
  description: "Governance console for organizational AI knowledge",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body
        className={`${spaceGrotesk.variable} ${jetbrains.variable} ${fraunces.variable} ${inter.variable} ${plexMono.variable}`}
      >
        <Chrome />
        <main>{children}</main>
      </body>
    </html>
  );
}
