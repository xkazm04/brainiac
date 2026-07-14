"use client";

// Client detail hook: live proxy first, demo synthesis when offline.

import { useEffect, useState } from "react";

import type { MemoryDetail } from "@/lib/types";

import { demoDetail } from "./archive-data";

export function useMemoryDetail(id: string | null, live: boolean) {
  const [detail, setDetail] = useState<MemoryDetail | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    if (!id) return;
    setLoading(true);
    const finish = (d: MemoryDetail | null) => {
      if (!cancelled) {
        setDetail(d);
        setLoading(false);
      }
    };
    if (!live) {
      finish(demoDetail(id));
      return;
    }
    fetch(`/api/memories/${id}`)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((d: MemoryDetail) => finish(d))
      .catch(() => finish(demoDetail(id)));
    return () => {
      cancelled = true;
    };
  }, [id, live]);
  return { detail, loading };
}
