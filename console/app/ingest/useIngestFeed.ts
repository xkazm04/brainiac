"use client";

// Live feed hook: polls the combined proxy every 6s while the tab is
// visible; keeps the server-rendered snapshot when offline/demo.

import { useCallback, useEffect, useState } from "react";

import type { IngestData } from "./ingest-data";

export function useIngestFeed(initial: IngestData) {
  const [data, setData] = useState(initial);
  const [refreshing, setRefreshing] = useState(false);

  const refresh = useCallback(async () => {
    if (!initial.live) return;
    setRefreshing(true);
    try {
      const r = await fetch("/api/ingest/feed");
      if (r.ok) {
        const next = await r.json();
        setData({ live: true, ...next });
      }
    } catch {
      // keep the last good snapshot
    } finally {
      setRefreshing(false);
    }
  }, [initial.live]);

  useEffect(() => {
    if (!initial.live) return;
    const tick = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    const t = setInterval(tick, 6000);
    return () => clearInterval(t);
  }, [initial.live, refresh]);

  return { data, refresh, refreshing };
}
