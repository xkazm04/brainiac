/*
 * The console's single navigation source of truth.
 *
 * Both the operator chrome (app/chrome.tsx) and the home wordmark nav
 * (src/home/Home.tsx) render their product links from PRODUCT_ROUTES, so the
 * two can never drift apart again. Home itself is the wordmark (links to "/",
 * not listed here); /demo is the public visitor surface — reachable from the
 * home footer rather than the operator nav, since it needs no token and shows
 * the fixture org.
 */

import { band, GROUND, type BandKey } from "./theme";

/** A route's accent band, or "ground" (0 Hz) for identity/access surfaces. */
export type RouteBand = BandKey | "ground";

export interface ProductRoute {
  /** Route path, e.g. "/reviews". */
  path: string;
  /** First URL segment (the module key), e.g. "reviews". */
  segment: string;
  /** Nav label — may differ from the segment (e.g. /memories → "archive"). */
  label: string;
  /** EEG band accent, or "ground" for surfaces outside the band spectrum. */
  band: RouteBand;
}

// In nav order. Bands mirror theme.ts MODULE_BAND; keys sits on ground (0 Hz),
// deliberately outside the spectrum (see GROUND in theme.ts).
export const PRODUCT_ROUTES: ProductRoute[] = [
  { path: "/reviews", segment: "reviews", label: "reviews", band: "alpha" },
  { path: "/disputes", segment: "disputes", label: "disputes", band: "theta" },
  { path: "/graph", segment: "graph", label: "graph", band: "gamma" },
  { path: "/memories", segment: "memories", label: "archive", band: "delta" },
  { path: "/ingest", segment: "ingest", label: "ingest", band: "theta" },
  { path: "/analytics", segment: "analytics", label: "analytics", band: "beta" },
  { path: "/keys", segment: "keys", label: "keys", band: "ground" },
];

/** Resolve a route's accent color from its band. */
export const routeAccent = (b: RouteBand): string =>
  b === "ground" ? GROUND : band(b);

/** Human label for a route's band, for the chrome's module caption. */
export const routeBandLabel = (b: RouteBand): string =>
  b === "ground" ? "ground · 0 Hz" : `${b} band`;

/** The product route owning a URL path (matched on its first segment). */
export const routeForPath = (pathname: string): ProductRoute | undefined => {
  const seg = pathname.split("/")[1] ?? "";
  return PRODUCT_ROUTES.find((r) => r.segment === seg);
};
