"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

import { band, FONT_MONO } from "@/design/theme";

interface Status {
  live: boolean;
  pending: number;
  contradictions: number;
  queueDepth: number;
}

const POLL_MS = 30_000;

// Live queue badges + connection dot for the shared nav: every page tells
// the operator whether numbers are live and whether work is waiting.
export default function NavStatus() {
  const [status, setStatus] = useState<Status | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const res = await fetch("/api/status", { cache: "no-store" });
        if (!res.ok) throw new Error(String(res.status));
        const s = (await res.json()) as Status;
        if (!cancelled) setStatus(s);
      } catch {
        if (!cancelled) setStatus({ live: false, pending: 0, contradictions: 0, queueDepth: 0 });
      }
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  if (!status) return null;
  const waiting = status.pending + status.contradictions;
  const dotColor = status.live ? band("beta") : "#f0b429";
  const label = status.live ? "live" : "demo data";
  return (
    <span className={`${FONT_MONO} flex items-center gap-3`}>
      {status.live && waiting > 0 && (
        <Link
          href="/reviews"
          className="rounded-full border px-2.5 py-0.5 transition hover:text-white"
          style={{ borderColor: band("beta", 68, 0.4), color: band("beta") }}
          title={`${status.pending} pending promotions, ${status.contradictions} open contradictions`}
        >
          {waiting} to review
        </Link>
      )}
      <span className="flex items-center gap-1.5 text-[#e9edff]/45" title={`connection: ${label}`}>
        <span
          aria-hidden
          className="inline-block h-1.5 w-1.5 rounded-full"
          style={{ background: dotColor, boxShadow: `0 0 6px ${dotColor}` }}
        />
        {label}
      </span>
    </span>
  );
}
