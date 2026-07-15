"use client";

// Live feed hook: polls the combined proxy while the tab is visible; keeps the
// server-rendered snapshot when offline/demo.
//
// Self-scheduling rather than a fixed setInterval, because an interval cannot
// (a) guarantee the previous request finished — a feed slower than the period
// stacks overlapping requests forever — or (b) back off when the feed is failing,
// which turned a down/erroring endpoint into an un-throttled request storm for as
// long as the tab stayed open.

import { useCallback, useEffect, useRef, useState } from "react";

import type { IngestData } from "./ingest-data";

/** Healthy poll period. */
const BASE_MS = 6000;
/** Ceiling for the failure backoff. */
const MAX_MS = 60_000;

export function useIngestFeed(initial: IngestData) {
  const [data, setData] = useState(initial);
  const [refreshing, setRefreshing] = useState(false);
  // Synchronous guard. React state is async, so checking `refreshing` could not
  // stop a second call entering before the first resolved.
  const inFlight = useRef(false);
  const failures = useRef(0);
  const abortRef = useRef<AbortController | null>(null);

  const refresh = useCallback(async () => {
    if (!initial.live || inFlight.current) return;
    inFlight.current = true;
    const ac = new AbortController();
    abortRef.current = ac;
    setRefreshing(true);
    try {
      const r = await fetch("/api/ingest/feed", { signal: ac.signal });
      if (r.ok) {
        const next = await r.json();
        // A response that lost the race (or arrived after unmount) must not
        // clobber fresher state.
        if (!ac.signal.aborted) setData({ live: true, ...next });
        failures.current = 0;
      } else {
        failures.current += 1;
      }
    } catch {
      // Keep the last good snapshot. An abort is our own doing, not a failure —
      // counting it would back the poll off every time the component remounts.
      if (!ac.signal.aborted) failures.current += 1;
    } finally {
      inFlight.current = false;
      if (!ac.signal.aborted) setRefreshing(false);
    }
  }, [initial.live]);

  useEffect(() => {
    if (!initial.live) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      // Exponential backoff on consecutive failures, reset to BASE_MS by the
      // first success.
      const delay = Math.min(BASE_MS * 2 ** failures.current, MAX_MS);
      timer = setTimeout(async () => {
        if (cancelled) return;
        // Awaiting here is what serialises the loop: the next tick is only
        // scheduled once this request has settled, so polls can never overlap.
        if (document.visibilityState === "visible") await refresh();
        if (!cancelled) schedule();
      }, delay);
    };
    schedule();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      abortRef.current?.abort();
    };
  }, [initial.live, refresh]);

  return { data, refresh, refreshing };
}
