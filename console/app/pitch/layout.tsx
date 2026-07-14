/*
 * The pitch owns its layout.
 *
 * It is a long-form argument for a reader who has no console login, so it must
 * not inherit the operator chrome (the two-row instrument header with its live
 * queue counts and its A–Z product nav). Chrome already suppresses itself here
 * (app/chrome.tsx FULL_BLEED), and this layout is where the pitch's own sticky
 * section rail lives instead.
 */

import PitchNav from "@/pitch/PitchNav";

export default function PitchLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="pitch-surface">
      <PitchNav />
      {children}
    </div>
  );
}
