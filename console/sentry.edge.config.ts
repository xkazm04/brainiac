// Sentry init for the Edge runtime (middleware, edge routes). Loaded by
// instrumentation.ts. Same posture as the server config: disabled unless
// NEXT_PUBLIC_SENTRY_DSN is set.
import * as Sentry from "@sentry/nextjs";

const dsn = process.env.NEXT_PUBLIC_SENTRY_DSN;

if (dsn) {
  Sentry.init({
    dsn,
    environment: process.env.NEXT_PUBLIC_BRAINIAC_ENV || "local",
    tracesSampleRate: 0,
  });
}
