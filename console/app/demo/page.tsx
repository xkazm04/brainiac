import type { Metadata } from "next";

import Observatory from "@/observatory/Observatory";
import { DEMO_OBSERVATORY } from "@/observatory/observatory-data";
import { FONT_MONO, GOLD, LABEL } from "@/design/theme";

export const metadata: Metadata = {
  title: "Brainiac — the demo org",
  description:
    "Walk a governed knowledge base end to end on a synthetic org: the review gate, contradictions, the canonical graph, the archive, and a knowledge-health score.",
};

export default function DemoOverviewPage() {
  return (
    <div>
      <section className="mx-auto max-w-7xl px-6 pt-8">
        <div className={LABEL} style={{ color: GOLD }}>
          the overview
        </div>
        <h1 className="mt-2 max-w-3xl text-3xl font-semibold leading-tight tracking-tight text-white md:text-4xl">
          This is what your organization&apos;s memory looks like once someone is
          accountable for it.
        </h1>
        <p
          className={`${FONT_MONO} mt-4 max-w-2xl text-sm leading-relaxed`}
          style={{ color: "rgba(233,237,255,0.55)" }}
        >
          Every number below was produced by the real pipeline — capture → extract →
          resolve → contradict → promote — running on a fixture org. The tabs above walk
          the same surfaces an operator uses. Plug in your own teams and the wall goes
          live.
        </p>
      </section>
      <Observatory data={DEMO_OBSERVATORY} />
    </div>
  );
}
