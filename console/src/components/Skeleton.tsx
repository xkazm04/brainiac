import type { CSSProperties, ReactNode } from "react";

import { LABEL } from "@/design/theme";

/*
 * Band-themed loading skeletons shared by the route loading.tsx files. Pure
 * CSS pulse (no client JS) — every block is a faint ink fill so the frame
 * reads as "instrument warming up" in the module's accent. Dimensions
 * approximate each page's real layout so content resolves without a jump.
 */

const FILL = "rgba(233,237,255,0.05)";

/** A single pulsing block. Width/height accept any CSS length. */
export function Pulse({
  w = "100%",
  h = 14,
  rounded = "rounded",
  style,
}: {
  w?: string | number;
  h?: string | number;
  rounded?: string;
  style?: CSSProperties;
}) {
  return (
    <div
      className={`animate-pulse ${rounded}`}
      style={{ width: w, height: h, background: FILL, ...style }}
    />
  );
}

/** A bordered card placeholder (matches the console's panel look). */
export function PulseCard({
  h = 160,
  accent,
  children,
}: {
  h?: string | number;
  accent?: string;
  children?: ReactNode;
}) {
  return (
    <div
      className="animate-pulse rounded-lg border"
      style={{
        height: children ? undefined : h,
        borderColor: accent ? `${accent}` : "rgba(233,237,255,0.08)",
        background: "rgba(233,237,255,0.02)",
      }}
    >
      {children}
    </div>
  );
}

/**
 * Standard page frame for a loading state: the module caption in its accent
 * (matching chrome's "segment · band"), then the skeleton body.
 */
export function SkeletonFrame({
  segment,
  accent,
  children,
}: {
  segment: string;
  accent: string;
  children: ReactNode;
}) {
  return (
    <div className="mx-auto max-w-7xl px-6 py-10" aria-busy="true" aria-live="polite">
      <div className={LABEL} style={{ color: accent }}>
        {segment} · loading
      </div>
      <span className="sr-only">Loading {segment}…</span>
      <div className="mt-6">{children}</div>
    </div>
  );
}
