/*
 * Is this org's console showing a front door, or a wall of empty charts?
 *
 * `/console` opens on analytics (design/routes.ts DEFAULT_MODULE). That is the
 * right front door for an org that HAS memories and the wrong one for a fresh
 * database: a stranger's first ever screen was five zeroed tiles and no next
 * step, while the one genuinely good on-ramp — the copyable brainiac-onboard
 * command — sat behind `?m=keys`, one of fourteen modules nothing pointed at.
 *
 * The predicate lives here, pure and tested, rather than inline in the module:
 * the branch decides what the product's first impression IS, and "did the
 * fetch fail" vs "the org is genuinely empty" are two different pages. Getting
 * that wrong the offline way would show an onboarding panel to an operator
 * whose server is merely down — telling them to onboard a repo when the thing
 * to fix is the server.
 */

/** The `?view=` value that forces the charts even on an empty org. */
export const FORCE_ANALYTICS_VIEW = "charts";

export interface FirstRunInput {
  /** withDemoFallback's verdict — false means we are looking at fixtures. */
  live: boolean;
  /** Observatory status totals, keyed by memory status. */
  totals: Record<string, number>;
  /** The `?view=` param, if the operator asked for something specific. */
  view?: string;
}

/**
 * True when the console should show the first-run on-ramp instead of the
 * Observatory.
 *
 * Three guards, each load-bearing:
 *  - `live` — fixtures mean the server is unreachable; that is ApiOffline's /
 *    DemoBanner's story, not onboarding's. The demo fixture is also non-empty,
 *    so without this guard the panel would simply never show.
 *  - `canonical === 0` — the product's claim is "memory an agent can retrieve
 *    from", and only canonical memories are that. An org mid-pipeline (raw or
 *    candidate rows, nothing promoted yet) still has nothing to retrieve, so it
 *    still gets the on-ramp — but the panel says so rather than pretending the
 *    repo was never connected.
 *  - `view` — an explicit `?view=charts` always wins. Never trap an operator on
 *    a page they did not ask for; an empty Observatory is still a legitimate
 *    thing to want to look at.
 */
export function isFirstRun({ live, totals, view }: FirstRunInput): boolean {
  if (!live) return false;
  if (view === FORCE_ANALYTICS_VIEW) return false;
  return (totals.canonical ?? 0) === 0;
}

/**
 * Has anything at all been ingested? Distinguishes "nothing has ever arrived"
 * from "content arrived and is still upstream of promotion" — the same panel,
 * a different sentence, because the operator's next action differs (connect a
 * repo vs. go clear the review queue).
 */
export function hasIngestedAnything(totals: Record<string, number>): boolean {
  return Object.values(totals).some((n) => n > 0);
}
