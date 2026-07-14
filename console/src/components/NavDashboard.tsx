"use client";

/*
 * The operator chrome's top row: a compact mini-dashboard.
 *
 * DISPLAY ONLY — deliberately. This row carries no links and no controls, so
 * the eye can treat it as instrumentation and the row below as the only place
 * anything is clickable. (Its ancestor, NavStatus, mixed a link into the
 * status badges; splitting the two is the point of the two-row header.)
 *
 * The bearer token never reaches the browser: /api/status resolves it
 * server-side and returns only the counts.
 */

import { useEffect, useState } from "react";

import { band, FONT_MONO, LABEL } from "@/design/theme";

interface Status {
  live: boolean;
  pending: number;
  contradictions: number;
  flagged: number;
  queueDepth: number;
}

const POLL_MS = 30_000;

const MINT = band("beta");
const ALPHA = band("alpha");
const THETA = band("theta");
const GOLD = band("gamma");

function Metric({
  label,
  value,
  tone,
  hint,
}: {
  label: string;
  value: number | string;
  tone: string;
  hint: string;
}) {
  return (
    <span className="flex items-baseline gap-1.5" title={hint}>
      <span className={`${FONT_MONO} text-sm font-semibold tabular-nums`} style={{ color: tone }}>
        {value}
      </span>
      <span
        className={`${FONT_MONO} text-[10px] uppercase tracking-[0.14em]`}
        style={{ color: "rgba(233,237,255,0.35)" }}
      >
        {label}
      </span>
    </span>
  );
}

export default function NavDashboard() {
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
        if (!cancelled) {
          setStatus({ live: false, pending: 0, contradictions: 0, flagged: 0, queueDepth: 0 });
        }
      }
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  // Reserve the row's height before the first poll lands, so the header does
  // not jump under the operator on every navigation.
  if (!status) return <span className="h-5" aria-hidden />;

  const dot = status.live ? MINT : "#f0b429";
  const waiting = status.pending + status.contradictions + status.flagged;

  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
      {status.live ? (
        <>
          <Metric
            label="to review"
            value={waiting}
            tone={waiting > 0 ? GOLD : "rgba(233,237,255,0.5)"}
            hint="promotions + contradictions + disputed memories awaiting a maintainer"
          />
          <Metric
            label="pending"
            value={status.pending}
            tone={ALPHA}
            hint="promotions waiting for a human to sign"
          />
          <Metric
            label="contradictions"
            value={status.contradictions}
            tone={status.contradictions > 0 ? "#ff5da2" : "rgba(233,237,255,0.5)"}
            hint="two sources disagree and nobody has adjudicated"
          />
          <Metric
            label="disputed"
            value={status.flagged}
            tone={THETA}
            hint="memories readers flagged as wrong or stale"
          />
          <Metric
            label="ingest queue"
            value={status.queueDepth}
            tone="rgba(233,237,255,0.6)"
            hint="sources waiting for the pipeline worker"
          />
        </>
      ) : (
        <span className={`${FONT_MONO} text-[11px]`} style={{ color: "rgba(233,237,255,0.4)" }}>
          demo data · start `brainiac serve` for live numbers
        </span>
      )}

      <span
        className={`${LABEL} flex items-center gap-1.5`}
        style={{ color: "rgba(233,237,255,0.45)" }}
        title={status.live ? "connected to the brainiac API" : "API unreachable"}
      >
        <span
          aria-hidden
          className="inline-block h-1.5 w-1.5 rounded-full"
          style={{ background: dot, boxShadow: `0 0 6px ${dot}` }}
        />
        {status.live ? "live" : "offline"}
      </span>
    </div>
  );
}
