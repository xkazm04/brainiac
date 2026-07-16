"use client";

/*
 * The Library — the normative layer: coding standards per tech stack + skills
 * for coding agents, as governed artifacts with vital signs.
 *
 * Composition rule inherited from /pitch and /kb: THE FIGURE IS THE SECTION.
 * Every move leads with a drawing that carries the idea; prose is a caption
 * beside it, never the argument.
 *
 * This file is deliberately just the running order. Each move lives in
 * `sections/`, its drawings in `figures/`, the shared scaffolding (Stamp,
 * Section, tones) in `primitives.tsx`, and every user-visible string in
 * `library-data.ts` — where the honesty and audience rules are enforced by
 * `library-data.test.ts`. Chrome (nav rail, background, dividers) is shared
 * with /pitch and /kb via app/library/layout.tsx.
 */

import { MAGENTA } from "../design/theme";
import Divider from "../components/Divider";
import { ALPHA, INK, MINT, THETA } from "./primitives";
import { FONT_DISPLAY } from "../design/theme";
import Drift from "./sections/Drift";
import Layers from "./sections/Layers";
import Anatomy from "./sections/Anatomy";
import Loop from "./sections/Loop";
import Agents from "./sections/Agents";
import Never from "./sections/Never";
import StatusLadder from "./sections/StatusLadder";
import Finale from "./sections/Finale";

export default function Library() {
  return (
    <div className={FONT_DISPLAY} style={{ color: INK }}>
      <Drift />
      <Divider tone={THETA} />
      <Layers />
      <Divider tone={MINT} />
      <Anatomy />
      <Divider />
      <Loop />
      <Divider tone={ALPHA} />
      <Agents />
      <Divider tone={MAGENTA} />
      <Never />
      <Divider />
      <StatusLadder />
      <Finale />
    </div>
  );
}
