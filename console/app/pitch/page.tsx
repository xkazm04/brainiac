import type { Metadata } from "next";

import Pitch from "@/pitch/Pitch";

// The presentation deck. Public, static, example-data-only: it makes no API call
// and holds no token. This is the argument — the competitive case, the evidence,
// and the cases where we lose. The landing page is the console home at "/".
export const metadata: Metadata = {
  title: "Brainiac — nothing becomes what your company knows until someone signs it",
  description:
    "Governed organizational memory for coding agents. An agent proposes; a named human promotes. Permission-aware retrieval enforced by Postgres RLS, per-fact provenance, contradiction adjudication — and a published controlled trial we allowed ourselves to lose.",
};

export default function PitchPage() {
  return <Pitch />;
}
