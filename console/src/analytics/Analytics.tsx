import Script from "next/script";

import { analyticsEnabled, plausibleDomain, plausibleScriptUrl } from "./config";

/*
 * The tracker tag. Mounted once, in the root layout.
 *
 * Two scripts, and the order matters:
 *
 *  1. the queue shim, `beforeInteractive`-adjacent (inline, so it runs the
 *     moment it parses). A custom event fired during hydration — signup
 *     completion is exactly that — would otherwise hit an undefined
 *     `window.plausible` and be dropped silently. The shim buffers into
 *     `plausible.q`; the real script drains it on load. This is Plausible's own
 *     documented snippet, not an invention.
 *
 *  2. the tracker itself, `afterInteractive`, with `defer` — it must never
 *     compete with hydration for the main thread.
 *
 * Renders NOTHING when unconfigured, so a deployment without
 * NEXT_PUBLIC_PLAUSIBLE_DOMAIN ships no third-party script at all (which is
 * also what keeps the "no third-party requests without a domain" claim on the
 * privacy page true).
 */
export default function Analytics() {
  if (!analyticsEnabled()) return null;

  return (
    <>
      <Script id="plausible-queue" strategy="afterInteractive">
        {`window.plausible=window.plausible||function(){(window.plausible.q=window.plausible.q||[]).push(arguments)}`}
      </Script>
      <Script
        id="plausible-tracker"
        strategy="afterInteractive"
        defer
        data-domain={plausibleDomain()}
        src={plausibleScriptUrl()}
      />
    </>
  );
}
