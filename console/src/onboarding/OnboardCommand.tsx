"use client";

/*
 * The developer-facing bootstrap: one copyable command that installs the
 * brainiac-onboard skill into a checkout.
 *
 * No secret is in it. The command fetches a skill file over plain GET; the
 * authority arrives later, through the device-flow pairing the skill drives
 * and an operator approves in the Projects module. That is why this can sit on
 * the console's front door without a scope check in front of it.
 *
 * Extracted from the Keys module so the first-run panel could render THE SAME
 * on-ramp rather than a second one that drifts. Keys still carries its own
 * copy of this markup at the time of writing (a concurrent Button-primitive
 * migration owns that file); folding `modules/keys/Keys.tsx`'s `OnboardPanel`
 * onto this component is a one-import follow-up, and this is the component it
 * should import.
 */

import { useState } from "react";

import Button from "@/components/ui/Button";
import { FONT_MONO, GROUND, LABEL } from "@/design/theme";

/** The command itself — exported so a test (or a doc generator) can assert the
 *  console and docs/QUICKSTART.md are telling a developer the same thing. */
export const onboardCommand = (apiUrl: string): string =>
  `mkdir -p .claude/skills/brainiac-onboard && curl -fsSL ${apiUrl}/v1/onboard/skill -o .claude/skills/brainiac-onboard/SKILL.md`;

export default function OnboardCommand({
  apiUrl,
  title = "onboard a repo — no key handling required",
}: {
  apiUrl: string;
  /** Overridden by the first-run panel, which numbers it as a step. */
  title?: string;
}) {
  const [copied, setCopied] = useState(false);
  const cmd = onboardCommand(apiUrl);
  return (
    <div className="rounded-xl border border-white/10 bg-white/[0.015] p-5">
      <div className={LABEL} style={{ color: "rgba(233,237,255,0.4)" }}>
        {title}
      </div>
      <p className={`${FONT_MONO} mt-2 text-xs leading-relaxed text-[var(--ink)]/55`}>
        A developer runs this in their checkout, then{" "}
        <span className="text-[var(--ink)]/80">/brainiac-onboard</span> in Claude Code. The skill pairs
        the repo here, writes a project-scoped key straight into their{" "}
        <span className="text-[var(--ink)]/80">.env</span>, and verifies the round trip.
      </p>
      <div
        className={`${FONT_MONO} mt-3 select-all break-all rounded-lg border border-white/15 bg-white/[0.03] p-3 text-xs text-[var(--ink)]/80`}
      >
        {cmd}
      </div>
      {/* Ground-band capsule. `accent` carries the text colour; the explicit
          borderColor restores the full-opacity edge — controlStyle fades a
          pill's border to 0.4 by default. */}
      <Button
        variant="pill"
        size="sm"
        accent={GROUND}
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(cmd);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          } catch {
            setCopied(false);
          }
        }}
        className="mt-3 px-4 text-xs"
        style={{ borderColor: GROUND }}
      >
        {copied ? "✓ copied" : "copy command"}
      </Button>
    </div>
  );
}
