/*
 * The console's front door on an empty org.
 *
 * `/console` opens on analytics, and analytics over zero memories is five
 * zeroed tiles and a dead end — the first thing a stranger ever saw. This is
 * what they see instead, until one canonical memory exists (see first-run.ts
 * for the predicate and why each guard is there).
 *
 * It teaches the ONE thing the product is for: get an agent a memory it can
 * retrieve from. That takes three parties in a fixed order, and the order is
 * the part everyone gets wrong — the pairing cannot be approved unless the
 * repo is already registered under a project (onboard.rs: approval derives the
 * project from the whitelist match; an unmatched request can only be denied).
 * So registering the repo is step 1, not a thing you discover from a failure.
 *
 * The middle step is deliberately the SAME component the Keys module shows —
 * the good on-ramp already existed, it was just buried behind one of fourteen
 * `?m=` modules with nothing pointing at it. This promotes it; it does not
 * write a second one.
 */

import Link from "next/link";

import { FONT_DISPLAY, FONT_MONO, GROUND, INK_FAINT, LABEL } from "@/design/theme";

import OnboardCommand from "./OnboardCommand";
import { FORCE_ANALYTICS_VIEW } from "./first-run";

const QUICKSTART_URL = "https://github.com/xkazm04/brainiac/blob/master/docs/QUICKSTART.md";

function Step({
  n,
  who,
  title,
  children,
}: {
  n: number;
  who: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <li className="flex gap-4">
      <span
        className={`${FONT_MONO} mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full border text-xs`}
        style={{ borderColor: GROUND, color: GROUND }}
        aria-hidden
      >
        {n}
      </span>
      <div className="min-w-0 flex-1">
        <div className={LABEL} style={{ color: INK_FAINT }}>
          {who}
        </div>
        <h2 className={`${FONT_DISPLAY} mt-1 text-lg font-semibold tracking-tight text-white`}>
          {title}
        </h2>
        <div className="mt-2">{children}</div>
      </div>
    </li>
  );
}

const body = `${FONT_MONO} text-xs leading-relaxed text-[var(--ink)]/55`;

export default function FirstRun({
  apiUrl,
  /** True when raw/candidate rows exist but nothing has been promoted yet —
   *  the org is mid-pipeline, not unconnected, and the advice differs. */
  midPipeline = false,
}: {
  apiUrl: string;
  midPipeline?: boolean;
}) {
  return (
    <section className="mx-auto max-w-3xl px-6 py-12">
      <div className={LABEL} style={{ color: GROUND }}>
        ground · first run
      </div>
      <h1 className={`${FONT_DISPLAY} mt-1 text-3xl font-semibold tracking-tight text-white`}>
        {midPipeline
          ? "Content is in — nothing is canonical yet."
          : "Give your agent a memory."}
      </h1>
      <p className={`${body} mt-3 max-w-2xl`}>
        {midPipeline ? (
          <>
            Sources have arrived and the pipeline extracted from them, but no memory has been
            promoted to <span className="text-[var(--ink)]/80">canonical</span> yet — so retrieval
            still returns nothing an agent should trust. Clear the queue in{" "}
            <Link href="/console?m=reviews" className="underline" style={{ color: GROUND }}>
              reviews
            </Link>
            , and this page becomes the Observatory.
          </>
        ) : (
          <>
            This org has no memories yet, so the dashboards would only show zeroes. Brainiac is
            worth something the moment one repo&apos;s agent can retrieve what the org already
            learned — that takes three steps, in this order.
          </>
        )}
      </p>

      {!midPipeline && (
        <ol className="mt-8 space-y-8">
          <Step n={1} who="operator · here" title="Register the repo under a project">
            <p className={body}>
              Open{" "}
              <Link href="/console?m=projects" className="underline" style={{ color: GROUND }}>
                projects
              </Link>
              , create a project, and add the repo&apos;s <code>git remote</code> to its
              allow-list. This must happen first: a pairing request whose remote matches no
              project cannot be approved, only denied.
            </p>
          </Step>

          <Step n={2} who="developer · their checkout" title="Install the onboarding skill">
            <OnboardCommand apiUrl={apiUrl} title="run this in the repo" />
          </Step>

          <Step n={3} who="operator · here" title="Approve the pairing">
            <p className={body}>
              The skill prints a short code and waits. Approve it on the right-hand rail of{" "}
              <Link href="/console?m=projects" className="underline" style={{ color: GROUND }}>
                projects
              </Link>{" "}
              (it polls, so the request appears while you watch). Approval mints a
              project-scoped read/write key — never <code>admin</code> — straight into the
              developer&apos;s <code>.env</code>. The code expires in 15 minutes.
            </p>
          </Step>
        </ol>
      )}

      <div className="mt-10 flex flex-wrap items-center gap-x-6 gap-y-2 border-t border-white/10 pt-5">
        <a
          href={QUICKSTART_URL}
          target="_blank"
          rel="noreferrer"
          className={`${FONT_MONO} text-xs underline`}
          style={{ color: GROUND }}
        >
          full quickstart →
        </a>
        <Link
          href="/console?m=keys"
          className={`${FONT_MONO} text-xs text-[var(--ink)]/40 underline`}
        >
          mint a key by hand
        </Link>
        {/* Never trap the operator: an empty Observatory is a legitimate thing
            to want to look at, and `?view=charts` is the way back to it. */}
        <Link
          href={`/console?m=analytics&view=${FORCE_ANALYTICS_VIEW}`}
          className={`${FONT_MONO} text-xs text-[var(--ink)]/40 underline`}
        >
          show the empty dashboard anyway
        </Link>
      </div>
    </section>
  );
}
