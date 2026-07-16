import type { Metadata } from "next";

import Library from "@/library/Library";

// Public, static, example-data-only — like the pitch at "/" and the KB
// explainer at /kb, it makes no API call and holds no token. This surface
// explains the normative layer (standards + skills); the working modules,
// when they land per docs/LIBRARY-PLAN.md, live behind the console gate.
export const metadata: Metadata = {
  title: "Brainiac — the library with a pulse",
  description:
    "Coding standards per tech stack and skills for coding agents, as governed artifacts: every rule carries the provenance behind it and the adoption in front of it, one named human gates anything normative, and usage telemetry — counted by team, never by person — retires dead rules out loud. Every capability is stamped shipped, in progress, or roadmap.",
};

export default function LibraryPage() {
  return <Library />;
}
