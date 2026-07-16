"use client";

/*
 * Sweep control — the "manageable in UI" half of the scheduled sweeps.
 *
 * One compact instrument strip per sweep, sat above the report it produces:
 * when it last ran and what it found, an enable/disable toggle, a cadence
 * selector, and a one-shot "run now". A sweep is a multi-minute LLM scan the
 * worker runs off-request, so "run now" queues it (status flips running → ok
 * on a later paint) rather than blocking — the microcopy says so.
 */

import { useTransition } from "react";

import {
  band,
  BORDER,
  FONT_DISPLAY,
  FONT_MONO,
  INK,
  INK_DIM,
  INK_FAINT,
  LABEL,
  MAGENTA,
  PANEL,
  withAlpha,
} from "@/design/theme";
import type { SweepSchedule } from "@/lib/types";
import { runSweepAction, updateSweepAction } from "@/ops/sweep-actions";

const CADENCES: { label: string; secs: number }[] = [
  { label: "hourly", secs: 3600 },
  { label: "daily", secs: 86_400 },
  { label: "weekly", secs: 604_800 },
];

const statusAccent = (status: string | null | undefined): string => {
  switch (status) {
    case "ok":
      return band("beta");
    case "running":
      return band("gamma");
    case "error":
      return MAGENTA;
    default:
      return INK_FAINT;
  }
};

/** "2h ago", "3d ago", "just now" — leaders read elapsed, not timestamps. */
const ago = (iso: string | null | undefined): string => {
  if (!iso) return "never";
  const secs = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (secs < 90) return "just now";
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
};

/** Time until a future instant — "imminent", "in 3h", "in 2d". */
const until = (iso: string): string => {
  const secs = (new Date(iso).getTime() - Date.now()) / 1000;
  if (secs <= 60) return "imminent";
  const m = Math.floor(secs / 60);
  if (m < 60) return `in ${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `in ${h}h`;
  return `in ${Math.floor(h / 24)}d`;
};

export default function SweepControl({
  schedule,
  title,
  revalidate,
}: {
  schedule: SweepSchedule;
  /** Human name for this sweep, e.g. "divergence scan". */
  title: string;
  /** Path to revalidate after an action (the page this control sits on). */
  revalidate: string;
}) {
  const [pending, start] = useTransition();
  const accent = statusAccent(schedule.last_status);

  const toggle = () =>
    start(() => updateSweepAction(schedule.kind, { enabled: !schedule.enabled }, revalidate));
  const setCadence = (secs: number) =>
    start(() => updateSweepAction(schedule.kind, { cadence_secs: secs }, revalidate));
  const runNow = () => start(() => runSweepAction(schedule.kind, revalidate));

  const currentCadence =
    CADENCES.find((c) => c.secs === schedule.cadence_secs)?.label ??
    `${Math.round(schedule.cadence_secs / 3600)}h`;

  return (
    <div
      className="mx-auto flex max-w-5xl flex-col gap-4 rounded-xl px-6 py-4"
      style={{ background: PANEL, border: `1px solid ${BORDER}`, opacity: pending ? 0.7 : 1 }}
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        {/* status */}
        <div className="flex items-center gap-3">
          <span className="h-2.5 w-2.5 rounded-full" style={{ background: accent }} />
          <div className="flex flex-col">
            <span className={`${FONT_DISPLAY} text-[15px]`} style={{ color: INK }}>
              {title}
            </span>
            <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
              {schedule.last_status
                ? `${schedule.last_status} · ${ago(schedule.last_run_at)}`
                : "never run"}
              {schedule.last_detail ? ` · ${schedule.last_detail}` : ""}
            </span>
          </div>
        </div>

        {/* controls */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={runNow}
            disabled={pending}
            className={`${FONT_MONO} rounded-md px-3 py-1.5 text-[12px]`}
            style={{ color: INK, border: `1px solid ${BORDER}`, background: "rgba(255,255,255,0.04)" }}
          >
            run now
          </button>
          <button
            type="button"
            onClick={toggle}
            disabled={pending}
            className={`${FONT_MONO} rounded-md px-3 py-1.5 text-[12px]`}
            style={{
              color: schedule.enabled ? band("beta") : INK_DIM,
              border: `1px solid ${schedule.enabled ? band("beta") : BORDER}`,
              background: schedule.enabled ? withAlpha(band("beta"), 0.08) : "transparent",
            }}
          >
            {schedule.enabled ? "scheduled ✓" : "schedule off"}
          </button>
        </div>
      </div>

      {/* cadence row — only meaningful when scheduled */}
      <div className="flex flex-wrap items-center gap-3">
        <span className={LABEL} style={{ color: INK_FAINT }}>
          cadence
        </span>
        <div className="flex gap-1.5">
          {CADENCES.map((c) => {
            const active = c.label === currentCadence;
            return (
              <button
                key={c.label}
                type="button"
                onClick={() => setCadence(c.secs)}
                disabled={pending || active}
                className={`${FONT_MONO} rounded px-2.5 py-1 text-[11px]`}
                style={{
                  color: active ? band("gamma") : INK_DIM,
                  border: `1px solid ${active ? band("gamma") : BORDER}`,
                  background: active ? withAlpha(band("gamma"), 0.08) : "transparent",
                }}
              >
                {c.label}
              </button>
            );
          })}
        </div>
        {schedule.enabled && schedule.next_run_at && (
          <span className={`${FONT_MONO} text-[11px]`} style={{ color: INK_FAINT }}>
            · next run {until(schedule.next_run_at)}
          </span>
        )}
      </div>
    </div>
  );
}
