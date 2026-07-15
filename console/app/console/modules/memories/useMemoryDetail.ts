"use client";

// Client detail hook: live proxy first, demo synthesis when offline.

import { useEffect, useState } from "react";

import type { MemoryDetail } from "@/lib/types";

import { demoDetail } from "./archive-data";

export function useMemoryDetail(id: string | null, live: boolean) {
  const [detail, setDetail] = useState<MemoryDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setError(null);
    if (!id) return;
    setLoading(true);
    const finish = (d: MemoryDetail | null, err: string | null = null) => {
      if (!cancelled) {
        setDetail(d);
        setError(err);
        setLoading(false);
      }
    };
    if (!live) {
      // Whole archive is in demo mode (page shows a DemoBanner) — expected.
      finish(demoDetail(id));
      return;
    }
    fetch(`/api/memories/${id}`)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((d: MemoryDetail) => finish(d))
      // On a LIVE archive there is no banner, so demoDetail(id) would show an
      // unrelated fabricated memory (DEMO_ROWS[0]) as if it were this one. Fail
      // honestly instead of substituting.
      .catch(() => finish(null, "Couldn't load this memory — the server may be unavailable."));
    return () => {
      cancelled = true;
    };
  }, [id, live]);
  return { detail, loading, error };
}
