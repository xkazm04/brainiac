"use client";

// Client drill-down hook: live proxy first, demo synthesis when offline.

import { useEffect, useState } from "react";

import type { CanonicalDetail } from "@/lib/types";

import { demoDetail, type CortexData } from "./cortex-data";

export function useCanonicalDetail(id: string | null, data: CortexData) {
  const [detail, setDetail] = useState<CanonicalDetail | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    if (!id) return;
    setLoading(true);
    const finish = (d: CanonicalDetail | null) => {
      if (!cancelled) {
        setDetail(d);
        setLoading(false);
      }
    };
    if (!data.live) {
      finish(demoDetail(id, data.overview));
      return;
    }
    fetch(`/api/graph/canonical/${id}`)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((d: CanonicalDetail) => finish(d))
      .catch(() => finish(demoDetail(id, data.overview)));
    return () => {
      cancelled = true;
    };
  }, [id, data]);
  return { detail, loading };
}
