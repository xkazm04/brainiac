import type { Metadata } from "next";

import { FUNNEL } from "@/analytics/config";
import TrackView from "@/analytics/TrackView";
import Home from "@/home/Home";

// The public landing page: the console home (the wave field), rendered in its
// PUBLIC variant on example data.
//
// It passes `live={null}` deliberately and makes no API call at all — so it
// holds no token and cannot leak the org's real canonical counts, pending-review
// backlog or team names to an anonymous visitor. The identical home, on LIVE
// data, is /console behind the passcode gate (middleware.ts).
export const metadata: Metadata = {
  title: "Brainiac — governed memory for coding agents",
  description:
    "Three teams, one wave. Capture knowhow from real LLM sessions, govern it through review, serve it to agents under permission-aware retrieval — and read the org-level picture no single session can see: health, drift, standards, pages that cannot rot.",
};

export default function LandingPage() {
  return (
    <>
      {/*
        Funnel step 1 of 3 (landing → signup → activation). Fired as an explicit
        custom event rather than read off the pageview report, so the funnel's
        three steps are three goals of the same kind and stay comparable even if
        the landing route is ever split or renamed. No-op unless a Plausible
        domain is configured.
      */}
      <TrackView event={FUNNEL.landingView} />
      <Home live={null} variant="public" />
    </>
  );
}
