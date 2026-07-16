/*
 * The library page owns its layout, exactly like /pitch and /kb: no operator
 * chrome (it is a public shell outside the console layout), the shared
 * `pitch-surface` background, and the shared sticky section rail with
 * scroll-spy + progress line.
 */

import SectionRail from "@/components/SectionRail";
import { LIBRARY_SECTIONS } from "@/library/library-data";

export default function LibraryLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="pitch-surface">
      <SectionRail
        railId="library"
        badge="the library"
        sections={LIBRARY_SECTIONS}
        cta={{ href: "/kb", label: "the knowledge base →" }}
      />
      {children}
    </div>
  );
}
