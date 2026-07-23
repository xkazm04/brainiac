// Next.js instrumentation hook. Loads the Sentry server/edge configs on the
// matching runtime and forwards server request errors to Sentry. All of it is
// inert when NEXT_PUBLIC_SENTRY_DSN is unset (the configs guard their own init).
import * as Sentry from "@sentry/nextjs";

export async function register() {
  if (process.env.NEXT_RUNTIME === "nodejs") {
    await import("./sentry.server.config");
  }
  if (process.env.NEXT_RUNTIME === "edge") {
    await import("./sentry.edge.config");
  }
}

export const onRequestError = Sentry.captureRequestError;
