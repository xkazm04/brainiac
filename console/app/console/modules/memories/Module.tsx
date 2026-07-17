import DemoBanner from "@/components/DemoBanner";
import { configFromEnv, listMemories, memoryValidity, type ApiConfig } from "@/lib/api";
import { withDemoFallback } from "@/lib/demo-fallback";

import Archive from "./Archive";
import {
  DEMO_ROWS,
  PAGE_SIZE,
  demoArchive,
  emptyFacets,
  type ArchiveData,
  type ArchiveDir,
  type ArchiveFilter,
  type ArchiveSort,
} from "./archive-data";

export const dynamic = "force-dynamic";

export const metadata = {
  title: "Brainiac — Archive",
};

/*
 * The archive, server-paginated.
 *
 * This used to loop `listMemories` up to 5000 rows into the browser and do all
 * of search / facets / cross-filter / sort / as-of / paginate in-memory. The
 * server now owns every one of those (GET /v1/memories?…&facets=1), so this
 * component makes exactly TWO calls:
 *
 *   (a) the page + its cross-filtered facet menu, under the full filter incl.
 *       as_of;
 *   (b) the tiny as-of skeleton (id + validity window) under the SAME filter
 *       minus as_of — ~40 bytes/row — which is all the client needs to scrub
 *       the time axis instantly and re-query the page on release.
 *
 * The URL is the single source of truth (parse here, once), so a filtered view
 * is shareable and survives refresh; `withDemoFallback` keeps it from ever
 * 500ing when the backend is down.
 */

type Params = Record<string, string | string[] | undefined>;

const one = (v: string | string[] | undefined): string | undefined =>
  Array.isArray(v) ? v[0] : v;

const SORTS: ArchiveSort[] = ["recent", "valid_from", "valid_to"];
const asSort = (v: string | string[] | undefined): ArchiveSort => {
  const s = one(v);
  return s && (SORTS as string[]).includes(s) ? (s as ArchiveSort) : "recent";
};
const asDir = (v: string | string[] | undefined): ArchiveDir =>
  one(v) === "asc" ? "asc" : "desc";

export function parseFilter(params: Params): {
  filter: ArchiveFilter;
  page: number;
  sort: ArchiveSort;
  dir: ArchiveDir;
  asOf: string | null;
} {
  const filter: ArchiveFilter = {
    q: one(params.q) || undefined,
    kind: one(params.kind) || undefined,
    status: one(params.status) || undefined,
    team: one(params.team) || undefined,
    visibility: one(params.visibility) || undefined,
    project: one(params.project) || undefined,
  };
  const n = Number(one(params.page));
  const page = Number.isFinite(n) ? Math.max(0, Math.floor(n)) : 0;
  return { filter, page, sort: asSort(params.sort), dir: asDir(params.dir), asOf: one(params.as_of) || null };
}

/** The filter as query params — empties omitted. Shared by both live calls. */
function filterParams(f: ArchiveFilter): Record<string, string> {
  const p: Record<string, string> = {};
  if (f.q) p.q = f.q;
  if (f.kind) p.kind = f.kind;
  if (f.status) p.status = f.status;
  if (f.team) p.team = f.team;
  if (f.visibility) p.visibility = f.visibility;
  if (f.project) p.project = f.project;
  return p;
}

async function fetchArchive(
  cfg: ApiConfig,
  filter: ArchiveFilter,
  page: number,
  sort: ArchiveSort,
  dir: ArchiveDir,
  asOf: string | null,
): Promise<ArchiveData> {
  const base = filterParams(filter);
  const listParams: Record<string, string> = {
    ...base,
    sort,
    dir,
    facets: "1",
    limit: String(PAGE_SIZE),
    offset: String(page * PAGE_SIZE),
  };
  if (asOf) listParams.as_of = asOf;

  // (a) the page + facet menu, (b) the as_of-excluded skeleton — in parallel.
  const [list, validity] = await Promise.all([
    listMemories(cfg, listParams),
    memoryValidity(cfg, base),
  ]);

  return {
    live: true,
    total: list.total,
    facets: list.facets ?? emptyFacets(),
    rows: list.memories,
    skeleton: validity.rows,
    filter,
    page,
    pageSize: PAGE_SIZE,
    sort,
    dir,
    asOf,
  };
}

export default async function MemoriesPage({ searchParams }: { searchParams: Params }) {
  const { filter, page, sort, dir, asOf } = parseFilter(searchParams);

  const { data, live } = await withDemoFallback<ArchiveData>(
    () => fetchArchive(configFromEnv(), filter, page, sort, dir, asOf),
    demoArchive(DEMO_ROWS, filter, page, PAGE_SIZE, sort, dir, asOf),
  );

  return (
    <>
      {!live && <DemoBanner />}
      <Archive data={data} />
    </>
  );
}
