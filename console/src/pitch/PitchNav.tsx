"use client";

/*
 * The pitch's nav — a configured instance of the shared SectionRail (the sticky
 * scroll-spy rail whose lower edge is the progress line). The behaviour lives
 * in src/components/SectionRail.tsx, shared with /kb.
 */

import SectionRail from "../components/SectionRail";
import { SECTIONS } from "./pitch-data";

export default function PitchNav() {
  return (
    <SectionRail
      railId="pitch"
      badge="the case"
      sections={SECTIONS}
      cta={{ href: "/demo", label: "see it running →" }}
    />
  );
}
