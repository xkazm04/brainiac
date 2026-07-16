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

/**
 * The three layers of the product, and the reason the nav is grouped at all.
 *
 * "memory" is the substrate: what the org captured, what it disagrees about,
 * what it resolved to, and what is arriving. "knowledge" is what the field
 * computes on top of it — the review gate, the health composite, the compiled
 * pages, the drift board. "library" is the normative layer computed above
 * both (LIBRARY-PLAN LB2): the org's ratified judgment — coding standards and
 * agent skills. Flat links made those read as one undifferentiated pile; they
 * are different jobs, often different people.
 *
 * `keys` is in none of them. It is access, not knowledge — it sits with
 * sign-out on the right, where the identity affordances live.
 */
export type NavGroup = "memory" | "knowledge" | "library";

export const NAV_GROUPS: { id: NavGroup; label: string }[] = [
  { id: "memory", label: "memory" },
  { id: "knowledge", label: "knowledge" },
  { id: "library", label: "library" },
];

export interface ProductRoute {
  /** Route path, e.g. "/console?m=reviews". */
  path: string;
  /** The module key (what ?m= carries), e.g. "reviews". */
  segment: string;
  /** Nav label — may differ from the segment (e.g. memories → "archive"). */
  label: string;
  /** EEG band accent, or "ground" for surfaces outside the band spectrum. */
  band: RouteBand;
  /** Which nav group it belongs to; omitted for the access surfaces. */
  group?: NavGroup;
}

// In nav order. Bands mirror theme.ts MODULE_BAND; keys sits on ground (0 Hz),
// deliberately outside the spectrum (see GROUND in theme.ts).
//
// The console is ONE route (app/console/page.tsx) and the module is a query
// param, so every path here is /console?m=<segment>. Analytics leads because it
// is what /console itself opens on: the console's front door is the wall, not a
// landing page — the landing lives at "/" and having it twice only made the
// operator click through a pitch to reach their own org.
export const PRODUCT_ROUTES: ProductRoute[] = [
  // ── memory: the substrate ──────────────────────────────────────────────
  { path: "/console?m=analytics", segment: "analytics", label: "analytics", band: "beta", group: "memory" },
  { path: "/console?m=memories", segment: "memories", label: "archive", band: "delta", group: "memory" },
  { path: "/console?m=disputes", segment: "disputes", label: "disputes", band: "theta", group: "memory" },
  { path: "/console?m=graph", segment: "graph", label: "graph", band: "gamma", group: "memory" },
  { path: "/console?m=ingest", segment: "ingest", label: "ingest", band: "theta", group: "memory" },

  // ── knowledge: what the field computes on top of it ────────────────────
  // The leadership read: one composite the org can be held to (KB-PLAN KB0).
  { path: "/console?m=health", segment: "health", label: "health", band: "alpha", group: "knowledge" },
  // The document layer (KB-PLAN KB2): pages compiled from canonical memories.
  // Gamma — the binding band — because a composed page is exactly that: many
  // teams' governed memories bound into one percept.
  { path: "/console?m=docs", segment: "docs", label: "pages", band: "gamma", group: "knowledge" },
  { path: "/console?m=reviews", segment: "reviews", label: "reviews", band: "alpha", group: "knowledge" },
  // The governance ledger: what the reviews rail already claims is "ledgered
  // and signed" (ReviewWorklist.tsx) but was, until now, viewable nowhere.
  // Alpha — same "calm governance" band as reviews/health — because this is
  // the record of THAT decision-making, not a new activity of its own.
  { path: "/console?m=audit", segment: "audit", label: "audit", band: "alpha", group: "knowledge" },
  // The drift detector: where teams solved the same problem different ways
  // (theta, the divergence band — same family as disputes/contradiction work).
  // Label was "standards" until the Library claimed that word for the
  // ARTIFACT (an adopted rule); this board is the DETECTOR that feeds it.
  { path: "/console?m=divergence", segment: "divergence", label: "drift", band: "theta", group: "knowledge" },

  // ── library: the normative layer — ratified judgment, distributed ───────
  // Standards stay on theta: a rule is a ratified drift, same family, one
  // band from detector to artifact. Skills are procedures agents actively
  // pull and run — beta, the active-recall band.
  { path: "/console?m=standards", segment: "standards", label: "standards", band: "theta", group: "library" },
  { path: "/console?m=skills", segment: "skills", label: "skills", band: "beta", group: "library" },

  // ── access: grouped with sign-out, not with the knowledge ──────────────
  { path: "/console?m=keys", segment: "keys", label: "keys", band: "ground" },
];

/** The console's modules, in nav order — the same list, named for what it is. */
export const CONSOLE_MODULES = PRODUCT_ROUTES;

export type ConsoleModuleId =
  | "analytics"
  | "reviews"
  | "disputes"
  | "graph"
  | "memories"
  | "ingest"
  | "health"
  | "docs"
  | "divergence"
  | "audit"
  | "standards"
  | "skills"
  | "keys";

/** The module /console opens on when ?m= is absent or junk. */
export const DEFAULT_MODULE: ConsoleModuleId = "analytics";

/** Read ?m= into a module id. Unknown values fall back rather than 404. */
export const parseModule = (raw: string | string[] | undefined): ConsoleModuleId => {
  const v = Array.isArray(raw) ? raw[0] : raw;
  return (PRODUCT_ROUTES.find((r) => r.segment === v)?.segment as ConsoleModuleId) ?? DEFAULT_MODULE;
};

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

/**
 * Exact-match public paths: the landing wave field, the gate itself, and the
 * free-tier sign-up.
 *
 * `/signup` MUST be public — it exists for someone who has no passcode, which is
 * the entire point of it. It reads no org data: the only privileged step is the
 * server-side provisioning call, made after the Google sign-in is verified.
 */
const PUBLIC_PATHS = new Set<string>(["/", "/login", "/signup"]);

/** Public subtrees — each renders its own shell (pitch, demo tour, wiki, library). */
const PUBLIC_SUBTREES = ["/pitch", "/demo", "/kb", "/library"];

const inSubtree = (pathname: string, root: string): boolean =>
  pathname === root || pathname.startsWith(`${root}/`);

/** Every surface an anonymous visitor may reach. The middleware's allow-list. */
export const isPublicSurface = (pathname: string): boolean =>
  PUBLIC_PATHS.has(pathname) || PUBLIC_SUBTREES.some((p) => inSubtree(pathname, p));
