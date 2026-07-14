/*
 * The knowledge-base page owns its layout, exactly like the pitch: no operator
 * chrome (app/chrome.tsx FULL_BLEED already suppresses it), the shared
 * `pitch-surface` background (band-hue glows over 48px engineering paper), and
 * the shared sticky section rail with scroll-spy + progress line.
 */

import SectionRail from "@/components/SectionRail";
import { KB_SECTIONS } from "@/kb/kb-data";

export default function KbLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="pitch-surface">
      <SectionRail
        railId="kb"
        badge="the knowledge base"
        sections={KB_SECTIONS}
        cta={{ href: "/pitch", label: "the full case →" }}
      />
      {children}
    </div>
  );
}
