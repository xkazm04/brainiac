"use client";

import { analyticsEnabled, type EventProps, type FunnelEvent } from "./config";

/*
 * Browser-side event emission. The whole surface is one function.
 *
 * Every failure mode is a no-op, never a throw: an ad-blocker removing the
 * tracker, a deployment with no domain, a server render reaching this by
 * accident. Telemetry that can break a sign-up is worse than no telemetry.
 */

declare global {
  interface Window {
    plausible?: ((event: string, opts?: { props?: EventProps }) => void) & {
      q?: unknown[];
    };
  }
}

/** Fire a funnel event. Safe to call anywhere, at any time, always. */
export function track(event: FunnelEvent, props?: EventProps): void {
  if (!analyticsEnabled()) return;
  if (typeof window === "undefined") return;
  try {
    window.plausible?.(event, props ? { props } : undefined);
  } catch {
    // An analytics call has no business surfacing to the user, and there is
    // nothing to retry — the event is simply lost.
  }
}
