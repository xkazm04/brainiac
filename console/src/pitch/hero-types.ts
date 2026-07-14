/** Shared contract for the hero-field variants, so the hero shell (headline,
 *  copy, the sign button, the telemetry strip) is identical across them and
 *  only the illustration's mental model differs. */
export interface HeroStats {
  /** Claims waiting for a human. */
  queued: number;
  /** Claims a human has signed. */
  canonical: number;
}

export interface HeroFieldProps {
  onStats?: (s: HeroStats) => void;
  /** The field hands its "a maintainer signs" action back to the shell. */
  onApproveRef?: (fn: () => void) => void;
}
