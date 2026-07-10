import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  // Builds write to .next-build (see package.json) so `npm run build` can
  // never corrupt a running dev server's .next — the cause of the recurring
  // "Cannot find module './NNN.js'" / missing-CSS breakages.
  distDir: process.env.NEXT_DIST_DIR || ".next",
};

export default nextConfig;
