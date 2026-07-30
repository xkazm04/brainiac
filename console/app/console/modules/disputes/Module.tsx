import DemoBanner from "@/components/DemoBanner";
import { configFromEnv } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";
import { int, one } from "@/design/routes";
import { feedbackQueue, type DecayBand } from "@/lib/governance-api";

import DisputeBench from "./DisputeBench";
import {
  DECAY_BANDS,
  DEMO_FLAGGED,
  PAGE_SIZE,
  demoPage,
  type DisputeData,
  type DisputeFilter,
} from "./disputes-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Disputes",
};

type Params = Record<string, string | string[] | undefined>;

/** These two filters distinguish "unset" from 0, which `int` cannot express. */
const numOrUndef = (v: string | string[] | undefined): number | undefined => {
  const n = Number(one(v));
  return Number.isFinite(n) ? n : undefined;
};

const BAND_SET = new Set(DECAY_BANDS.map((b) => b.band));
const asBand = (v: string | string[] | undefined): DecayBand | undefined => {
  const s = one(v);
  return s && BAND_SET.has(s as DecayBand) ? (s as DecayBand) : undefined;
};

/** The URL is the single source of truth for the filter/page, so a filtered
 *  view is shareable and survives refresh. Everything is parsed and clamped
 *  HERE, once, then handed to both the live fetch and the demo mirror. */
export function parseFilter(params: Params): { filter: DisputeFilter; page: number } {
  const filter: DisputeFilter = {
    kind: one(params.kind) || undefined,
    teamId: one(params.team) || undefined,
    project: one(params.project) || undefined,
    band: asBand(params.band),
    minClaims: numOrUndef(params.minClaims),
    minAgeHours: numOrUndef(params.minAge),
  };
  const page = int(params.page);
  return { filter, page };
}

// Live queue when the server is reachable; the demo shape (behind an
// unconditional DemoBanner, actions disabled) when it isn't — never 500s.
export default async function DisputesPage({ searchParams }: { searchParams: Params }) {
  const { filter, page } = parseFilter(searchParams);

  const { data, live } = await withDemoFallback<DisputeData>(async () => {
    const out = await feedbackQueue(configFromEnv(), {
      ...filter,
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    });
    return {
      live: true,
      flagged: out.flagged,
      total: out.total,
      facets: out.facets,
      filter,
      page,
      pageSize: PAGE_SIZE,
    };
  }, demoPage(DEMO_FLAGGED, filter, page, PAGE_SIZE));

  return (
    <>
      {!live && <DemoBanner />}
      <DisputeBench data={data} />
    </>
  );
}
