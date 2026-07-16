import type { NextConfig } from "next";

// The operator modules stopped being routes: the console is one page and the
// module is a query param (/console?m=reviews). These keep every old inbound
// link working — both the original top-level /reviews and the /console/reviews
// it briefly became. Temporary (307) on purpose: the auth gate sits behind them
// and a cached 308 would outlive any future reshuffle.
//
// /console/docs/<slug> is deliberately NOT in this list. A document is a real
// page with its own URL worth sharing, so it stayed a route; only the bare
// /console/docs index folds into the tab.
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

// The public tour collapsed from seven routes into one page with a tab bar
// (app/demo/DemoConsole.tsx). The module now lives in the query string, so these
// keep every old /demo/<module> link — shared, bookmarked, or printed in a deck —
// landing on the module it named.
const DEMO_MODULES = ["reviews", "disputes", "graph", "memories", "health", "divergence"];

const nextConfig: NextConfig = {
  output: "standalone",
  // Builds write to .next-build (see package.json) so `npm run build` can
  // never corrupt a running dev server's .next — the cause of the recurring
  // "Cannot find module './NNN.js'" / missing-CSS breakages.
  distDir: process.env.NEXT_DIST_DIR || ".next",
  async redirects() {
    return [
      // /reviews and /console/reviews → /console?m=reviews
      ...MOVED_MODULES.flatMap((m) => [
        { source: `/${m}`, destination: `/console?m=${m}`, permanent: false },
        { source: `/console/${m}`, destination: `/console?m=${m}`, permanent: false },
      ]),
      // The document sub-route survives the move; its old top-level form does not.
      { source: "/docs/:slug", destination: "/console/docs/:slug", permanent: false },
      ...DEMO_MODULES.map((m) => ({
        source: `/demo/${m}`,
        destination: `/demo?m=${m}`,
        permanent: false,
      })),
    ];
  },
};

export default nextConfig;
