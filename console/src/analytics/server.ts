import {
  FUNNEL,
  analyticsEnabled,
  plausibleDomain,
  plausibleEventUrl,
  type EventProps,
  type FunnelEvent,
} from "./config";

/*
 * Server-side funnel emission, via Plausible's Events API.
 *
 * WHY THE ACTIVATION EVENT CANNOT BE A BROWSER EVENT
 * --------------------------------------------------
 * Activation is "the org's knowledge actually moved" — a memory submitted, or
 * a promotion approved into canon. Both happen inside server actions that call
 * src/lib/api.ts with the privileged service token. There is no client
 * component in the analytics module's reach that observes the OUTCOME of
 * either (the components that invoke them live under app/console/**), and more
 * importantly a browser-fired activation would be a claim, not an observation:
 * it would count clicks, including the ones the backend rejected.
 *
 * WHAT THIS COSTS, STATED RATHER THAN HIDDEN
 * ------------------------------------------
 * A server-sent event has no visitor: no browser, no referrer, no real client
 * IP (we deliberately do NOT forward one — see below). Plausible will attribute
 * every activation to the server itself, so the UNIQUE count for this goal is
 * meaningless and only the TOTAL count is. That is the right trade: activation
 * volume is the number the funnel needs, and buying per-visitor attribution
 * would mean forwarding end-user IPs to a third party for no product gain.
 *
 * Fire-and-forget, bounded, and silent on every failure. Telemetry must never
 * slow, block or break a write the user is waiting on.
 */

/** Identifies the emitter to Plausible. The Events API rejects a missing UA. */
const SERVER_UA = "brainiac-console-server/1.0";

const EVENT_TIMEOUT_MS = 3000;

/**
 * Send one funnel event from the server.
 *
 * Returns a promise that NEVER rejects — it is returned only so a test can
 * await the beacon; production call sites do not await it (`void trackServer`)
 * and must not, because a user's write is not allowed to wait on telemetry.
 *
 * `url` is a synthetic path rather than a real request URL: the server has no
 * page context, and inventing one would pollute the pageview report.
 */
export async function trackServer(event: FunnelEvent, props?: EventProps): Promise<void> {
  if (!analyticsEnabled()) return;
  const domain = plausibleDomain();
  try {
    await fetch(plausibleEventUrl(), {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "user-agent": SERVER_UA,
        // NO x-forwarded-for. Plausible uses it to derive a (salted, daily,
        // non-stored) visitor hash; supplying the operator's real IP would send
        // personal data to a third party to buy attribution we just said is
        // meaningless for this event. Omitting it means Plausible sees the
        // server's own address, which identifies nobody.
      },
      body: JSON.stringify({
        name: event,
        domain,
        url: `https://${domain}/server/${event}`,
        ...(props ? { props } : {}),
      }),
      cache: "no-store",
      signal: AbortSignal.timeout(EVENT_TIMEOUT_MS),
    });
  } catch {
    // Swallowed on purpose. There is nothing a failed analytics beacon can
    // usefully tell the caller, and a rejected floating promise would surface
    // as an unhandled rejection in the server logs.
  }
}

/**
 * The activation event, named at its one meaning so call sites read as intent.
 *
 * `via` distinguishes the two shapes activation takes without identifying
 * anyone: `memory_add` (content arrived) or `promotion` (content was ratified).
 */
export function trackActivation(via: "memory_add" | "promotion"): void {
  void trackServer(FUNNEL.activation, { via });
}
