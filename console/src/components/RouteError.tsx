"use client";

import { useEffect } from "react";

import ApiOffline from "./ApiOffline";

/*
 * Shared error boundary for the console's data routes. Reuses the ApiOffline
 * look and wires the boundary's `reset` to its retry affordance, so a failed
 * fetch on any route degrades to the familiar "signal lost" screen with a way
 * back rather than a blank crash. Each app/<route>/error.tsx re-exports this.
 */
export default function RouteError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    // Surface the real error for operators watching the console log.
    console.error(error);
  }, [error]);

  return <ApiOffline error={error.message} onRetry={reset} />;
}
