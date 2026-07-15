"use client";

// Client drill-down hook: live proxy first, demo synthesis when offline.

import { useEffect, useState } from "react";

import type { CanonicalDetail } from "@/lib/types";

import { demoDetail, type CortexData } from "./cortex-data";

export function useCanonicalDetail(id: string | null, data: CortexData) {
  const [detail, setDetail] = useState<CanonicalDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setError(null);
    if (!id) return;
    setLoading(true);
    const finish = (d: CanonicalDetail | null, err: string | null = null) => {
      if (!cancelled) {
        setDetail(d);
        setError(err);
        setLoading(false);
      }
    };
    if (!data.live) {
      // Whole board is in demo mode (page shows a DemoBanner) — synthesis is
      // expected and clearly labeled.
      finish(demoDetail(id, data.overview));
      return;
    }
    fetch(`/api/graph/canonical/${id}`)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((d: CanonicalDetail) => finish(d))
      // On a LIVE board there is no demo banner, so synthesizing demoDetail here
      // would render fabricated surface forms / edges / memories AS REAL. Surface
      // the failure honestly instead.
      .catch(() => finish(null, "Couldn't load this entity — the server may be unavailable."));
    return () => {
      cancelled = true;
    };
  }, [id, data]);
  return { detail, loading, error };
}
