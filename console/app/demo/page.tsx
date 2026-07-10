import Link from "next/link";

import Observatory from "@/observatory/Observatory";
import { DEMO_OBSERVATORY } from "@/observatory/observatory-data";
import { band, FONT_MONO, LABEL } from "@/design/theme";

// Public demo — no token, no API call: visitors see the Observatory exactly
// as an operator would, running on the Meridian fixture org. Static and
// safe to expose.
export const metadata = {
  title: "Brainiac — Live Demo",
  description:
    "The Brainiac Observatory on a demo organization — governed AI knowledge, measured.",
};

export default function DemoPage() {
  return (
    <div>
      <section className="mx-auto max-w-7xl px-6 pt-8">
        <div
          className="flex flex-wrap items-center justify-between gap-4 rounded-lg border p-5"
          style={{ borderColor: band("beta", 68, 0.3), background: band("beta", 60, 0.05) }}
        >
          <div>
            <div className={LABEL} style={{ color: band("beta") }}>
              visitor demo · org “meridian” (synthetic fintech, 3 teams)
            </div>
            <h1 className="mt-1.5 text-2xl font-semibold tracking-tight text-white">
              This is what your organization&apos;s memory looks like, governed.
            </h1>
            <p className={`${FONT_MONO} mt-1.5 max-w-2xl text-sm leading-relaxed text-[#e9edff]/55`}>
              Every number below is produced by the real pipeline — capture → extract →
              resolve → contradict → promote — on a fixture org. Plug in your own teams
              and the wall goes live.
            </p>
          </div>
          <div className="flex items-center gap-3">
            <Link
              href="/"
              className={`${FONT_MONO} rounded-full border px-5 py-2.5 text-sm font-medium transition hover:bg-white/5`}
              style={{ borderColor: band("gamma"), color: band("gamma") }}
            >
              see how it works
            </Link>
            <a
              href="https://github.com/xkazm04/brainiac"
              target="_blank"
              rel="noreferrer"
              className={`${FONT_MONO} rounded-full border border-white/15 px-5 py-2.5 text-sm text-[#e9edff]/70 transition hover:border-white/40 hover:text-white`}
            >
              github
            </a>
          </div>
        </div>
      </section>
      <Observatory data={DEMO_OBSERVATORY} />
    </div>
  );
}
