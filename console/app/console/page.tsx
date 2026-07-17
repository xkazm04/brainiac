import { Suspense } from "react";

import { CONSOLE_MODULES, parseModule, type ConsoleModuleId } from "@/design/routes";

import ModuleBoundary from "./ModuleBoundary";

import AnalyticsModule from "./modules/analytics/Module";
import AnalyticsSkeleton from "./modules/analytics/Skeleton";
import AuditModule from "./modules/audit/Module";
import AuditSkeleton from "./modules/audit/Skeleton";
import DisputesModule from "./modules/disputes/Module";
import DisputesSkeleton from "./modules/disputes/Skeleton";
import DivergenceModule from "./modules/divergence/Module";
import DivergenceSkeleton from "./modules/divergence/Skeleton";
import DocsModule from "./modules/docs/Module";
import DocsSkeleton from "./modules/docs/Skeleton";
import GraphModule from "./modules/graph/Module";
import GraphSkeleton from "./modules/graph/Skeleton";
import HealthModule from "./modules/health/Module";
import HealthSkeleton from "./modules/health/Skeleton";
import IngestModule from "./modules/ingest/Module";
import IngestSkeleton from "./modules/ingest/Skeleton";
import KeysModule from "./modules/keys/Module";
import KeysSkeleton from "./modules/keys/Skeleton";
import MemoriesModule from "./modules/memories/Module";
import MemoriesSkeleton from "./modules/memories/Skeleton";
import ProjectsModule from "./modules/projects/Module";
import ProjectsSkeleton from "./modules/projects/Skeleton";
import ReviewsModule from "./modules/reviews/Module";
import ReviewsSkeleton from "./modules/reviews/Skeleton";
import SkillsModule from "./modules/skills/Module";
import SkillsSkeleton from "./modules/skills/Skeleton";
import StandardsModule from "./modules/standards/Module";
import StandardsSkeleton from "./modules/standards/Skeleton";

export const dynamic = "force-dynamic";

/*
 * The console — one route, ten modules, ?m= to choose.
 *
 * WHAT THIS IS NOT. It is not the tab-swap the public tour does. /demo holds
 * every fixture in the page and switches with no round trip because none of its
 * modules fetch anything. Every module here does, so a tab change is a soft
 * navigation that re-renders THIS page on the server and fetches only the module
 * asked for. The win is the URL and the chrome, not the latency: you still wait
 * for the data you actually asked for, which is the honest trade for not
 * fetching all ten on every visit (2026-07-15 decision).
 *
 * WHAT THE ROUTE GAVE AWAY, and how it is paid back:
 *  - error.tsx per segment → ModuleBoundary, keyed per module.
 *  - loading.tsx per segment → each module's Skeleton, in a keyed Suspense.
 *  - template.tsx entry motion → ModuleBoundary.
 * Both boundaries are keyed by module id, so switching tabs shows the incoming
 * module's own skeleton rather than holding the outgoing one's frame.
 *
 * The cost worth naming: one route means one client chunk, so a visit ships
 * every module's client code rather than just the module opened. Modest for an
 * internal operator console; the alternative was ten routes.
 */

const MODULES: Record<
  ConsoleModuleId,
  {
    Module: (props: {
      searchParams: Record<string, string | string[] | undefined>;
    }) => Promise<React.JSX.Element> | React.JSX.Element;
    Skeleton: () => React.JSX.Element;
  }
> = {
  analytics: { Module: AnalyticsModule, Skeleton: AnalyticsSkeleton },
  audit: { Module: AuditModule, Skeleton: AuditSkeleton },
  reviews: { Module: ReviewsModule, Skeleton: ReviewsSkeleton },
  disputes: { Module: DisputesModule, Skeleton: DisputesSkeleton },
  graph: { Module: GraphModule, Skeleton: GraphSkeleton },
  memories: { Module: MemoriesModule, Skeleton: MemoriesSkeleton },
  ingest: { Module: IngestModule, Skeleton: IngestSkeleton },
  health: { Module: HealthModule, Skeleton: HealthSkeleton },
  docs: { Module: DocsModule, Skeleton: DocsSkeleton },
  divergence: { Module: DivergenceModule, Skeleton: DivergenceSkeleton },
  standards: { Module: StandardsModule, Skeleton: StandardsSkeleton },
  skills: { Module: SkillsModule, Skeleton: SkillsSkeleton },
  projects: { Module: ProjectsModule, Skeleton: ProjectsSkeleton },
  keys: { Module: KeysModule, Skeleton: KeysSkeleton },
};

export async function generateMetadata({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const m = parseModule((await searchParams).m);
  const label = CONSOLE_MODULES.find((r) => r.segment === m)?.label ?? m;
  return { title: `Brainiac — ${label}` };
}

export default async function ConsolePage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const params = await searchParams;
  const m = parseModule(params.m);
  const { Module, Skeleton } = MODULES[m];
  return (
    <ModuleBoundary key={m}>
      <Suspense key={m} fallback={<Skeleton />}>
        <Module searchParams={params} />
      </Suspense>
    </ModuleBoundary>
  );
}
