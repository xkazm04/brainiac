"use client";

import { useEffect, useRef } from "react";

import { track } from "./track";
import type { EventProps, FunnelEvent } from "./config";

/**
 * Fire one funnel event when a server-rendered page mounts.
 *
 * It exists so a server component (the landing page) can contribute to the
 * funnel without becoming a client component itself, and without the page's
 * own components having to know analytics exists.
 *
 * The ref guard is not ceremony: React StrictMode double-invokes effects in
 * development, and a funnel whose first step counts twice is a funnel that
 * lies about its own conversion rate.
 */
export default function TrackView({
  event,
  props,
}: {
  event: FunnelEvent;
  props?: EventProps;
}) {
  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(event, props);
    // Intentionally mount-only: this marks "the page was viewed", which happens
    // once per mount regardless of how the props object identity churns.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return null;
}
