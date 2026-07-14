import type { NextConfig } from "next";

// The operator modules moved from top-level routes into the /console parent
// (app/console/(modules)/ — one persistent chrome, SPA-style module swaps).
// These keep every old bookmark and inbound link working. Temporary (307) on
// purpose: the auth gate sits behind them and a cached 308 would outlive any
// future reshuffle.
const MOVED_MODULES = [
  "reviews",
  "disputes",
  "graph",
  "memories",
  "ingest",
  "analytics",
  "health",
  "divergence",
  "docs",
  "keys",
];

const nextConfig: NextConfig = {
  output: "standalone",
  // Builds write to .next-build (see package.json) so `npm run build` can
  // never corrupt a running dev server's .next — the cause of the recurring
  // "Cannot find module './NNN.js'" / missing-CSS breakages.
  distDir: process.env.NEXT_DIST_DIR || ".next",
  async redirects() {
    return MOVED_MODULES.map((m) => ({
      source: `/${m}/:path*`,
      destination: `/console/${m}/:path*`,
      permanent: false,
    }));
  },
};

export default nextConfig;
