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
 * (app/console/layout.tsx).
 *
 * The URL still carries the module (`/demo?m=graph`), pushed with the History
 * API rather than the router — so deep links, the back button and a shared link
 * all keep working, and none of them cost a round trip. The old paths still
 * resolve: next.config.ts redirects /demo/<module> onto the query.
 *
 * WHAT A MODULE IS. Every one of them renders the operator's own component from
 * app/console/modules/ — never a copy. The tour's job is to be the console on
 * fixture data, so the only things this file adds are the tab bar, the fixture
 * wiring, and ModuleIntro: the demo's framing copy, which is kept OUT of the
 * shared components so /console never carries a word of pitch. `label` tracks
 * the operator's nav (src/design/routes.ts) for the same reason — a visitor
 * should learn the console's vocabulary here, not a demo dialect of it.
 *
 * Fixtures arrive as props from page.tsx (a server component) because one of
 * them — the disputes bench's — lives in a `server-only` module. That is the
 * whole reason this file takes data instead of importing it.
 */

import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";

import CortexMap from "../console/modules/graph/CortexMap";
import type { CortexData } from "../console/modules/graph/cortex-data";
import DisputeBench from "../console/modules/disputes/DisputeBench";
import type { DisputeData } from "../console/modules/disputes/disputes-data";
import Archive from "../console/modules/memories/Archive";
import type { ArchiveData } from "../console/modules/memories/archive-data";

import { band, FONT_MONO, GOLD } from "@/design/theme";
import type { ContradictionQueueItem, PromotionQueueItem } from "@/lib/governance-api";
import type { KnowledgeHealth, PracticeDivergences } from "@/lib/types";
import DemoStandards from "./DemoStandards";
import KnowledgeHealthReport from "@/health/KnowledgeHealth";
import Observatory from "@/observatory/Observatory";
import type { ObservatoryData } from "@/observatory/observatory-data";

import SkillsCatalog from "../console/modules/skills/SkillsCatalog";
import { DEMO_SKILL_DETAILS, DEMO_SKILLS } from "../console/modules/skills/skills-data";
import StandardsBoard from "../console/modules/standards/StandardsBoard";
import {
  DEMO_STANDARD_DETAILS,
  DEMO_STANDARDS,
} from "../console/modules/standards/standards-data";

import DemoReviews from "./DemoReviews";
import ModuleIntro, { type ModuleIntroCopy } from "./ModuleIntro";

export interface DemoData {
  observatory: ObservatoryData;
  promotions: PromotionQueueItem[];
  contradictions: ContradictionQueueItem[];
  counts: { status: string; count: number }[];
  disputes: DisputeData;
  cortex: CortexData;
  archive: ArchiveData;
  health: KnowledgeHealth;
  divergences: PracticeDivergences;
}

/*
 * The tour, in reading order.
 *
 * `id` is the console's own module segment (/console/<id>), which is also what
 * the ?m= query carries — the one exception is `memories`, which the operator
 * nav labels "archive". `label` is that nav label, so the tour teaches the
 * console's vocabulary. `docTitle` is what the module claims while it is on
 * screen: the per-route <title>s the split pages used to own, which one route
 * would otherwise drop. `title`/`description` are the demo's voice, and appear
 * nowhere else in the product.
 */
const MODULES: (ModuleIntroCopy & {
  id: string;
  label: string;
  blurb: string;
  docTitle: string;
})[] = [
  {
    id: "analytics",
    width: "max-w-7xl",
    label: "analytics",
    blurb: "the wall",
    docTitle: "Brainiac — the demo org",
    title: "This is what your organization’s memory looks like once someone is accountable for it.",
    description:
      "Every number below was produced by the real pipeline — capture → extract → resolve → contradict → promote — running on a fixture org. The tabs above walk the same surfaces an operator uses. Plug in your own teams and the wall goes live.",
  },
  {
    id: "reviews",
    width: "max-w-5xl",
    label: "reviews",
    blurb: "promotions awaiting a human",
    docTitle: "Brainiac — demo · the review gate",
    title: "An agent proposes. A named human promotes.",
    description:
      "This is the row that is empty for every other memory product. Nothing here is org truth yet — a machine extracted it from a real session, policy routed it into this queue, and it waits until a maintainer signs for it. The controls below are inert in the demo; in the console they are the only way anything becomes canonical.",
  },
  {
    id: "disputes",
    width: "max-w-6xl",
    label: "disputes",
    blurb: "contradictions, adjudicated",
    docTitle: "Brainiac — demo · disputes",
    title: "A memory can be true the day it is written and false a quarter later.",
    description:
      "Every memory carries a half-life — the validity window its kind was given. A reader’s claim is evidence it is decaying faster than the clock says. The bench plots what is dying before anyone re-verified it, and answering pushes a memory back to healthy, collapses it now, or leaves it decaying on schedule.",
  },
  {
    id: "graph",
    width: "max-w-7xl",
    label: "graph",
    blurb: "canonical entities",
    docTitle: "Brainiac — demo · cortex map",
    title: "Three teams, three names, one thing.",
    description:
      "Payments says “Kafka”, platform says “MSK cluster”, data says “the event bus”. Each is right locally, and none of them find each other. The graph binds the dialects into one canonical entity a query in any of them reaches — and permission, not the UI, still decides who sees what hangs off it.",
  },
  {
    id: "memories",
    width: "max-w-6xl",
    label: "archive",
    blurb: "the corpus, as-of any date",
    docTitle: "Brainiac — demo · archive",
    title: "What did the org believe last April?",
    description:
      "Nothing here is ever deleted — a superseded memory is kept, dated, and outranked. That is what makes the archive answerable as of any date: scrub the clock back and you get the corpus the org was actually serving its agents then, not today’s corpus with the losers removed.",
  },
  {
    id: "health",
    width: "max-w-5xl",
    label: "health",
    blurb: "is the knowledge rotting?",
    docTitle: "Brainiac — demo · knowledge health",
    title: "One number a leader can be held to.",
    description:
      "Consistency, currency, liquidity, governance — four pillars folded into one composite, and one unresolved cross-team contradiction caps the grade no matter how many good memories sit under it. It is a gate, not a dashboard: publishing to the company wiki pauses while the score sits below threshold.",
  },
  {
    id: "divergence",
    width: "max-w-5xl",
    label: "drift",
    blurb: "same problem, solved two ways",
    docTitle: "Brainiac — demo · drift",
    title: "Not a contradiction — a detune.",
    description:
      "Two teams solving the same problem slightly differently, each locally reasonable, the drift invisible from inside either one. A scheduled sweep listens across the whole field, names the practice, and proposes a single standard for a platform lead to ratify — with the provenance, and the model that adjudicated it, on the card.",
  },
  {
    id: "standards",
    width: "max-w-6xl",
    label: "standards",
    blurb: "the org's ratified rules",
    docTitle: "Brainiac — demo · standards",
    title: "A ratified drift becomes a rule — with the evidence still attached.",
    description:
      "The Library's rule shelf: one rule at a time, each carrying why it exists (the incident, the resolved dispute, the drift), how strongly it binds, and whether practice actually follows it — counted per team, never per person. Proposals wait at the gate; only what a named human adopted is ever served to an agent.",
  },
  {
    id: "skills",
    width: "max-w-5xl",
    label: "skills",
    blurb: "procedures agents pull and run",
    docTitle: "Brainiac — demo · skills",
    title: "The org's best prompts stop living in one person's dotfiles.",
    description:
      "A skill is a versioned bundle in the format coding agents already load — stored, governed, and served like everything else here. Drafts are listed and serve nothing until a named human signs one; the shelf ranks by pulse, so the skill nobody pulls is the first candidate to retire.",
  },
] as const;

export type DemoModuleId = (typeof MODULES)[number]["id"];

const DEFAULT_MODULE: DemoModuleId = "analytics";

const parseModule = (raw: string | null): DemoModuleId =>
  MODULES.find((m) => m.id === raw)?.id ?? DEFAULT_MODULE;

/** The module's own URL — the entry module is /demo bare, not /demo?m=analytics. */
const hrefFor = (id: DemoModuleId): string =>
  id === DEFAULT_MODULE ? "/demo" : `/demo?m=${id}`;

function Module({ id, data }: { id: DemoModuleId; data: DemoData }) {
  switch (id) {
    // Each fixture carries live:false, which each component already honours: it
    // synthesizes drill-in detail client-side rather than calling a gated /api
    // route, and it disables its write controls.
    case "reviews":
      return (
        <DemoReviews
          promotions={data.promotions}
          contradictions={data.contradictions}
          counts={data.counts}
        />
      );
    case "disputes":
      return <DisputeBench data={data.disputes} demo />;
    case "graph":
      return <CortexMap data={data.cortex} />;
    case "memories":
      return <Archive data={data.archive} />;
    case "health":
      return <KnowledgeHealthReport data={data.health} />;
    // The one module whose prototype round is still open — the switcher lives
    // here because the dev org has no divergences to judge a layout against.
    case "divergence":
      return <DemoStandards data={data.divergences} />;
    // The Library modules render their own demo fixtures (imported statically —
    // the same objects the console modules fall back to offline), with
    // live=false so the gate's controls never mount over fabricated rules.
    case "standards":
      return (
        <StandardsBoard standards={DEMO_STANDARDS} details={DEMO_STANDARD_DETAILS} live={false} />
      );
    case "skills":
      return <SkillsCatalog skills={DEMO_SKILLS} details={DEMO_SKILL_DETAILS} />;
    case "analytics":
      return <Observatory data={data.observatory} />;
    default:
      return null;
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

  const current = MODULES.find((m) => m.id === active) ?? MODULES[0];

  useEffect(() => {
    document.title = current.docTitle;
  }, [current]);

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
          <>
            <ModuleIntro {...current} />
            <Module id={active} data={data} />
          </>
        ) : (
          <AnimatePresence mode="wait">
            <motion.div
              key={active}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
            >
              <ModuleIntro {...current} />
              <Module id={active} data={data} />
            </motion.div>
          </AnimatePresence>
        )}
      </div>
    </>
  );
}
