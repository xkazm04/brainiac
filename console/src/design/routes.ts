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
  /** Route path, e.g. "/console/reviews". */
  path: string;
  /** The module key (the segment after /console/), e.g. "reviews". */
  segment: string;
  /** Nav label — may differ from the segment (e.g. …/memories → "archive"). */
  label: string;
  /** EEG band accent, or "ground" for surfaces outside the band spectrum. */
  band: RouteBand;
}

// In nav order. Bands mirror theme.ts MODULE_BAND; keys sits on ground (0 Hz),
// deliberately outside the spectrum (see GROUND in theme.ts). Every module
// lives under /console — one parent whose layout owns the operator chrome
// (app/console/(modules)/layout.tsx), so navigation between modules is a
// content-pane swap under a persistent header.
export const PRODUCT_ROUTES: ProductRoute[] = [
  { path: "/console/reviews", segment: "reviews", label: "reviews", band: "alpha" },
  { path: "/console/disputes", segment: "disputes", label: "disputes", band: "theta" },
  { path: "/console/graph", segment: "graph", label: "graph", band: "gamma" },
  { path: "/console/memories", segment: "memories", label: "archive", band: "delta" },
  { path: "/console/ingest", segment: "ingest", label: "ingest", band: "theta" },
  { path: "/console/analytics", segment: "analytics", label: "analytics", band: "beta" },
  // The leadership read: one composite the org can be held to (KB-PLAN KB0).
  { path: "/console/health", segment: "health", label: "health", band: "alpha" },
  // The document layer (KB-PLAN KB2): pages compiled from canonical memories.
  // Gamma — the binding band — because a composed page is exactly that: many
  // teams' governed memories bound into one percept.
  { path: "/console/docs", segment: "docs", label: "pages", band: "gamma" },
  // Standardization: where teams solved the same problem different ways (theta,
  // the divergence band — same family as disputes/contradiction work).
  { path: "/console/divergence", segment: "divergence", label: "standards", band: "theta" },
  { path: "/console/keys", segment: "keys", label: "keys", band: "ground" },
];

/** Resolve a route's accent color from its band. */
export const routeAccent = (b: RouteBand): string =>
  b === "ground" ? GROUND : band(b);

/** Human label for a route's band, for the chrome's module caption. */
export const routeBandLabel = (b: RouteBand): string =>
  b === "ground" ? "ground · 0 Hz" : `${b} band`;

/** The product route owning a URL path (exact or a subpage of it). */
export const routeForPath = (pathname: string): ProductRoute | undefined =>
  PRODUCT_ROUTES.find((r) => pathname === r.path || pathname.startsWith(`${r.path}/`));

// ── surface gating (the single source of truth) ─────────────────────────
//
// The middleware's allow-list lives here (the operator chrome no longer needs
// one: it is mounted by the console-module layout, so route structure decides
// where it renders). Kept in this pure module so anything else that needs the
// boundary imports the same predicate instead of growing its own copy — two
// copies drifted twice before (/kb unreachable, chrome stacked on /demo).

/** Exact-match public paths: the landing wave field and the gate itself. */
const PUBLIC_PATHS = new Set<string>(["/", "/login"]);

/** Public subtrees — each renders its own shell (pitch, demo tour, wiki). */
const PUBLIC_SUBTREES = ["/pitch", "/demo", "/kb"];

const inSubtree = (pathname: string, root: string): boolean =>
  pathname === root || pathname.startsWith(`${root}/`);

/** Every surface an anonymous visitor may reach. The middleware's allow-list. */
export const isPublicSurface = (pathname: string): boolean =>
  PUBLIC_PATHS.has(pathname) || PUBLIC_SUBTREES.some((p) => inSubtree(pathname, p));
