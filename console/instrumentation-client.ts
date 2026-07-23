// Sentry client-side (browser) init. Next.js 15.3+ loads this file natively on
// the client. Disabled unless NEXT_PUBLIC_SENTRY_DSN is set, so local dev and
// tests never phone home; the environment tag comes from NEXT_PUBLIC_BRAINIAC_ENV.
import * as Sentry from "@sentry/nextjs";

const dsn = process.env.NEXT_PUBLIC_SENTRY_DSN;

if (dsn) {
  Sentry.init({
    dsn,
    environment: process.env.NEXT_PUBLIC_BRAINIAC_ENV || "local",
    // Error tracking only — no performance/replay sampling by default.
    tracesSampleRate: 0,
  });
}

// Instrument client-side navigations (App Router). A no-op when Sentry is
// uninitialized.
export const onRouterTransitionStart = Sentry.captureRouterTransitionStart;
