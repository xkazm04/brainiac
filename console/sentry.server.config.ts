// Sentry server-side init (Node runtime). Loaded by instrumentation.ts.
//
// Disabled unless NEXT_PUBLIC_SENTRY_DSN is set, so local dev and tests never
// phone home. The environment tag comes from NEXT_PUBLIC_BRAINIAC_ENV
// (default "local"), mirroring the Rust server's BRAINIAC_ENV.
import * as Sentry from "@sentry/nextjs";

const dsn = process.env.NEXT_PUBLIC_SENTRY_DSN;

if (dsn) {
  Sentry.init({
    dsn,
    environment: process.env.NEXT_PUBLIC_BRAINIAC_ENV || "local",
    // Error tracking only — no performance tracing by default.
    tracesSampleRate: 0,
  });
}
