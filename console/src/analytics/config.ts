/*
 * Product analytics configuration — ONE tool, cookieless, env-gated.
 *
 * WHY PLAUSIBLE, AND WHY IT MATTERS BEYOND TASTE
 * ----------------------------------------------
 * Plausible sets no cookies and stores no cross-request identifier for a
 * visitor. That is not a preference: it is the load-bearing assumption behind
 * the console's cookie position (see app/privacy/page.tsx). The only cookie
 * this origin sets is the httpOnly operator session (`bx_console`,
 * src/lib/auth.ts) — strictly necessary, so no consent banner is required.
 * Swapping in a tool that persists a visitor id in a cookie or in
 * localStorage invalidates that position and REQUIRES the privacy page and the
 * consent answer to change with it. Do not swap it casually.
 *
 * WHY IT NO-OPS BY DEFAULT
 * ------------------------
 * There is no domain yet. Every entry point here is a no-op unless
 * NEXT_PUBLIC_PLAUSIBLE_DOMAIN is set, and unconfigured must never mean
 * "throws" — the same posture instrumentation-client.ts takes with Sentry.
 * Local dev and CI therefore emit nothing and need no network.
 *
 * Env is read inside functions rather than into module-scope consts so a test
 * can stub it. Next still inlines NEXT_PUBLIC_* here, because the inlining is
 * static text substitution on the literal `process.env.NEXT_PUBLIC_…`
 * expression — what matters is that the key is written out, not where.
 */

/**
 * The site name registered in Plausible (usually the public domain, e.g.
 * `brainiac.dev`). Unset ⇒ analytics is entirely off.
 */
export const plausibleDomain = (): string =>
  process.env.NEXT_PUBLIC_PLAUSIBLE_DOMAIN?.trim() || "";

/** Plausible instance origin. Override to self-host; defaults to the cloud. */
export const plausibleHost = (): string =>
  process.env.NEXT_PUBLIC_PLAUSIBLE_HOST?.trim().replace(/\/+$/, "") || "https://plausible.io";

/** True when a domain is configured and events should actually be sent. */
export const analyticsEnabled = (): boolean => plausibleDomain().length > 0;

/** The tracker script URL. */
export const plausibleScriptUrl = (): string => `${plausibleHost()}/js/script.js`;

/** The server-side Events API endpoint. */
export const plausibleEventUrl = (): string => `${plausibleHost()}/api/event`;

/**
 * The three funnel events, named once so the tracker call sites and the
 * Plausible goal configuration cannot drift.
 *
 *   landing_view    someone reached the public landing page (app/page.tsx)
 *   signup_complete Google sign-in verified AND a project resolved
 *                   (app/signup/SignupClient.tsx)
 *   activation      the org's knowledge actually moved — a memory was
 *                   submitted, or a promotion was approved into canon
 *                   (src/lib/api.ts)
 *
 * Deliberately NO user, org, project or memory identifiers ride along on any
 * of them. The custom props are low-cardinality shape only, so the funnel can
 * never become a per-person record.
 */
export const FUNNEL = {
  landingView: "landing_view",
  signupComplete: "signup_complete",
  activation: "activation",
} as const;

export type FunnelEvent = (typeof FUNNEL)[keyof typeof FUNNEL];

/** Custom-property values are short, low-cardinality strings — never free text. */
export type EventProps = Record<string, string | number | boolean>;
