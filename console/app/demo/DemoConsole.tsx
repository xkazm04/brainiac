"use client";

/*
 * The demo tour — one page, seven modules, no routing.
 *
 * It used to be seven routes under /demo, each a ~10-line wrapper around a
 * component and a fixture. Nothing on any of them was fetched, gated, or
 * paginated, so every tab click paid for a full navigation — skeleton, remount,
 * scroll reset — to swap static content the browser could have held all along.
 * The tour reads as one surface, so it is now one surface: the modules are tabs
 * over a content pane, exactly as the operator console's own module chrome works
 * (app/console/(modules)/layout.tsx).
 *
 * The URL still carries the module (`/demo?m=graph`), pushed with the History
 * API rather than the router — so deep links, the back button and a shared link
 * all keep working, and none of them cost a round trip. The old paths still
 * resolve: next.config.ts redirects /demo/<module> onto the query.
 *
 * Fixtures arrive as props from page.tsx (a server component) because one of
 * them — the disputes bench's — lives in a `server-only` module. That is the
 * whole reason this file takes data instead of importing it.
 */

import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";

import CortexMap from "../console/(modules)/graph/CortexMap";
import type { CortexData } from "../console/(modules)/graph/cortex-data";
import DisputeBench from "../console/(modules)/disputes/DisputeBench";
import type { DisputeData } from "../console/(modules)/disputes/disputes-data";
import Archive from "../console/(modules)/memories/Archive";
import type { ArchiveData } from "../console/(modules)/memories/archive-data";

import { band, FONT_MONO, GOLD, LABEL } from "@/design/theme";
import type { ContradictionQueueItem, PromotionQueueItem } from "@/lib/governance-api";
import type { KnowledgeHealth, PracticeDivergences } from "@/lib/types";
import PracticeDivergenceReport from "@/divergence/PracticeDivergence";
import KnowledgeHealthReport from "@/health/KnowledgeHealth";
import Observatory from "@/observatory/Observatory";
import type { ObservatoryData } from "@/observatory/observatory-data";

import ReviewGate from "./ReviewGate";

export interface DemoData {
  observatory: ObservatoryData;
  promotions: PromotionQueueItem[];
  contradictions: ContradictionQueueItem[];
  disputes: DisputeData;
  cortex: CortexData;
  archive: ArchiveData;
  health: KnowledgeHealth;
  divergences: PracticeDivergences;
}

/*
 * The tour, in reading order. `title` is the document title the module claims
 * while it is on screen — the per-route <title>s the split pages used to own,
 * which a single route would otherwise have silently dropped.
 */
const MODULES = [
  {
    id: "overview",
    label: "overview",
    blurb: "governance health",
    title: "Brainiac — the demo org",
  },
  {
    id: "reviews",
    label: "the gate",
    blurb: "promotions awaiting a human",
    title: "Brainiac — demo · the review gate",
  },
  {
    id: "disputes",
    label: "disputes",
    blurb: "contradictions, adjudicated",
    title: "Brainiac — demo · disputes",
  },
  {
    id: "graph",
    label: "graph",
    blurb: "canonical entities",
    title: "Brainiac — demo · cortex map",
  },
  {
    id: "memories",
    label: "archive",
    blurb: "the corpus, as-of any date",
    title: "Brainiac — demo · archive",
  },
  {
    id: "health",
    label: "health",
    blurb: "is the knowledge rotting?",
    title: "Brainiac — demo · knowledge health",
  },
  {
    id: "divergence",
    label: "standards",
    blurb: "same problem, solved two ways",
    title: "Brainiac — demo · standards",
  },
] as const;

export type DemoModuleId = (typeof MODULES)[number]["id"];

export const DEMO_MODULE_IDS: readonly string[] = MODULES.map((m) => m.id);

const DEFAULT_MODULE: DemoModuleId = "overview";

const parseModule = (raw: string | null): DemoModuleId =>
  (MODULES.find((m) => m.id === raw)?.id ?? DEFAULT_MODULE) as DemoModuleId;

/** The module's own URL — the overview is /demo bare, not /demo?m=overview. */
const hrefFor = (id: DemoModuleId): string =>
  id === DEFAULT_MODULE ? "/demo" : `/demo?m=${id}`;

function Overview({ data }: { data: ObservatoryData }) {
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
      <Observatory data={data} />
    </div>
  );
}

function Module({ id, data }: { id: DemoModuleId; data: DemoData }) {
  switch (id) {
    case "reviews":
      return <ReviewGate promotions={data.promotions} contradictions={data.contradictions} />;
    // DEMO_DISPUTES / DEMO_CORTEX / DEMO_ARCHIVE all carry live:false, which each
    // component already honours: it synthesizes drill-in detail client-side rather
    // than calling a gated /api route, and it disables its write controls.
    case "disputes":
      return <DisputeBench data={data.disputes} />;
    case "graph":
      return <CortexMap data={data.cortex} />;
    case "memories":
      return <Archive data={data.archive} />;
    case "health":
      return <KnowledgeHealthReport data={data.health} />;
    case "divergence":
      return <PracticeDivergenceReport data={data.divergences} />;
    case "overview":
      return <Overview data={data.observatory} />;
  }
}

export default function DemoConsole({ data }: { data: DemoData }) {
  const reduce = useReducedMotion();
  // Read on the first render, not in an effect: a visitor landing on
  // /demo?m=graph must get the graph, not the overview and then a flinch.
  const params = useSearchParams();
  const [active, setActive] = useState<DemoModuleId>(() => parseModule(params.get("m")));

  // Back/forward walk the tour, because each tab pushed a history entry — the
  // same thing the seven routes gave us, kept without the seven routes.
  useEffect(() => {
    const onPop = () =>
      setActive(parseModule(new URLSearchParams(window.location.search).get("m")));
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  useEffect(() => {
    document.title = MODULES.find((m) => m.id === active)?.title ?? MODULES[0].title;
  }, [active]);

  const go = useCallback(
    (id: DemoModuleId) => {
      if (id === active) return;
      setActive(id);
      window.history.pushState(null, "", hrefFor(id));
      // A tab is a new surface, not a scroll position on the old one.
      window.scrollTo({ top: 0, behavior: reduce ? "auto" : "smooth" });
    },
    [active, reduce],
  );

  return (
    <>
      <div
        role="tablist"
        aria-label="demo modules"
        className="mx-auto flex max-w-7xl flex-wrap gap-2 border-b border-white/[0.08] px-6 pb-4"
      >
        {MODULES.map((m) => {
          const on = m.id === active;
          return (
            <button
              key={m.id}
              type="button"
              role="tab"
              aria-selected={on}
              aria-controls="demo-panel"
              title={m.blurb}
              onClick={() => go(m.id)}
              className={`${FONT_MONO} cursor-pointer rounded-lg border px-3 py-2 text-xs transition`}
              style={{
                borderColor: on ? "hsla(46,90%,68%,0.45)" : "rgba(233,237,255,0.10)",
                background: on ? "hsla(46,90%,60%,0.07)" : "transparent",
                color: on ? GOLD : "rgba(233,237,255,0.6)",
              }}
            >
              <span className="font-medium">{m.label}</span>
              {/* The blurb is a nicety; it must not push the tour onto a second
                  line and orphan a tab. Below xl it goes to the title. */}
              <span
                className="ml-2 hidden text-[10px] xl:inline"
                style={{ color: on ? band("gamma", 68, 0.5) : "rgba(233,237,255,0.3)" }}
              >
                {m.blurb}
              </span>
            </button>
          );
        })}
      </div>

      <div id="demo-panel" role="tabpanel">
        {reduce ? (
          <Module id={active} data={data} />
        ) : (
          <AnimatePresence mode="wait">
            <motion.div
              key={active}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
            >
              <Module id={active} data={data} />
            </motion.div>
          </AnimatePresence>
        )}
      </div>
    </>
  );
}
