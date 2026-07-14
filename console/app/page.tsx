import type { Metadata } from "next";

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
  return <Home live={null} variant="public" />;
}
