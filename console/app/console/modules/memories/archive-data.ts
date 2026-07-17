// Shared substrate for the Archive.
//
// The corpus is now paginated, filtered, faceted and sorted by the SERVER
// (GET /v1/memories?…&facets=1). This module holds the client-safe shapes that
// travel from the server component to the Archive, the tiny helpers the row/
// scrubber rendering needs, and a demo mirror of the server contract so the
// public tour runs offline.
//
// The only corpus-wide thing the browser ever holds is the as-of SKELETON
// (id + validity window + status per row) — ~40 bytes/row — which is what keeps
// the time scrubber instant without paging full content.

import type {
  MemoryDetail,
  MemoryFacetMenu,
  MemoryRow,
  ValidityRow,
} from "@/lib/types";

// ── the filter, mirroring the server's query params ─────────────────────

/** The narrowing a maintainer applied — one value per dimension, exactly the
 *  params GET /v1/memories accepts. `team` is a team UUID (the facet `value`);
 *  its human name is the facet `label`. */
export interface ArchiveFilter {
  q?: string;
  kind?: string;
  status?: string;
  /** A team UUID. */
  team?: string;
  visibility?: string;
  /** A project UUID, or the sentinel `"none"` — the org-shared bucket is a
   *  selectable shelf, not an absence (PROJECT-PLAN PR2). */
  project?: string;
}

export type ArchiveSort = "recent" | "valid_from" | "valid_to";
export type ArchiveDir = "asc" | "desc";

/** One shelf of a facet menu: a value (a UUID for `team`), its display label,
 *  and how many rows it would land on in the current cross-filtered scope. */
export type ArchiveFacet = MemoryFacetMenu["kinds"][number];

/** The dimensions the facet rail / column headers filter, in rail order.
 *  Project sits beside team: team answers WHO wrote it, project WHAT it is
 *  about. */
export const FACET_KEYS = ["team", "project", "kind", "status", "visibility"] as const;
export type FacetKey = (typeof FACET_KEYS)[number];

export interface ArchiveData {
  live: boolean;
  /** The filtered depth as the server counts it (respects q + facets + as_of). */
  total: number;
  /** The cross-filtered menu — a dimension never shrinks its own shelf. */
  facets: MemoryFacetMenu;
  /** One page of rows, already filtered/sorted/windowed by the server. */
  rows: MemoryRow[];
  /** The whole visible corpus under the SAME filter minus as_of — ids + dates
   *  only. The time axis and the "true then" count are computed from this. */
  skeleton: ValidityRow[];
  /** The active filter, echoed back so the Archive can render/clear it. */
  filter: ArchiveFilter;
  /** Zero-based page index. */
  page: number;
  pageSize: number;
  sort: ArchiveSort;
  dir: ArchiveDir;
  /** The as-of instant (RFC3339), or null for "now" (the latest on record). */
  asOf: string | null;
}

/** An empty facet menu — the shape callers can always read five arrays off. */
export const emptyFacets = (): MemoryFacetMenu => ({
  kinds: [],
  statuses: [],
  teams: [],
  visibilities: [],
  projects: [],
});

/**
 * Nullable-and-optional: the generated API types mark Option<T> fields as
 * optional (utoipa's default), though the server always emits them as null.
 *
 * Lives here rather than in MemoryInspector because archive-index — pure, and
 * unit-tested without a DOM — pre-renders validity spans with it.
 */
export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toISOString().slice(0, 10);
}

/** The minimal validity shape both a full MemoryRow and a skeleton ValidityRow
 *  satisfy — the scrubber only ever needs the window. */
export interface HasWindow {
  valid_from?: string | null;
  valid_to?: string | null;
  created_at?: string | null;
}

/** Validity check for as-of scrubbing (null bounds = open interval). */
export function validAt(row: HasWindow, at: Date): boolean {
  const from = row.valid_from ? new Date(row.valid_from) : null;
  const to = row.valid_to ? new Date(row.valid_to) : null;
  if (from && from > at) return false;
  if (to && to <= at) return false;
  return true;
}

/**
 * The span the as-of scrubber covers: from the org's first record to its most
 * recent one.
 *
 * `valid_to` is deliberately excluded from the MAX, and that is the whole point
 * of this function. A memory's validity window runs into the future — the live
 * corpus carries TTLs out to 2028 — so maxing over `valid_to` put the scrubber's
 * right edge, and therefore its default playhead, eighteen months from now, past
 * every memory's expiry. The archive opened on "what did the org know on
 * 2028-01-07?" and answered, correctly and uselessly, "nothing".
 *
 * The question this surface asks is retrospective — what was true THEN — so the
 * axis is when things were learned, not when they are scheduled to lapse. Rows
 * still expire along the way; that is what `validAt` is for. Runs over the
 * SKELETON now, so `created_at` may be absent — it simply does not widen bounds.
 */
export function timeBounds(rows: HasWindow[]): { min: Date; max: Date } {
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const r of rows) {
    // valid_to widens the floor (a window can close before its row was written
    // in a backfill) but never the ceiling.
    for (const t of [r.valid_from, r.valid_to, r.created_at]) {
      if (!t) continue;
      const ms = new Date(t).getTime();
      if (Number.isFinite(ms)) min = Math.min(min, ms);
    }
    for (const t of [r.valid_from, r.created_at]) {
      if (!t) continue;
      const ms = new Date(t).getTime();
      if (Number.isFinite(ms)) max = Math.max(max, ms);
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max) || min >= max) {
    return { min: new Date("2025-06-01T00:00:00Z"), max: new Date("2026-07-10T00:00:00Z") };
  }
  return { min: new Date(min), max: new Date(max) };
}

// ── demo corpus (chains mirror the Meridian fixtures) ───────────────────

const row = (
  id: string,
  content: string,
  kind: string,
  team: string,
  opts: Partial<MemoryRow> = {},
): MemoryRow => ({
  id: `dm-${id}`,
  // Titles are nullable forever (migration 0023 onwards only), so the demo
  // corpus carries BOTH kinds on purpose: the last two rows below have none,
  // and the archive must render them as claims rather than as broken labels.
  title: null,
  content,
  kind,
  status: "canonical",
  visibility: "team",
  team,
  team_id: `t-${team}`,
  // Projects are APPLICATIONS, deliberately not team names: two teams feed
  // one app, and org-shared (null) rows are normal — the demo teaches the
  // model, so it must not conflate the two axes.
  project: null,
  project_id: null,
  valid_from: null,
  valid_to: null,
  superseded_by: null,
  created_at: "2026-06-20T10:00:00Z",
  confidence: 0.9,
  ...opts,
});

export const DEMO_ROWS: MemoryRow[] = [
  row("psp-10s", "psp-gateway client timeout is 10 seconds", "fact", "payments", {
    project: "payments-api",
    project_id: "dp-payments-api",
    title: "psp-gateway client timeout: 10s",
    status: "deprecated",
    visibility: "org",
    valid_from: "2025-09-01T00:00:00Z",
    valid_to: "2026-05-01T00:00:00Z",
    superseded_by: "dm-psp-30s",
    created_at: "2025-09-01T10:00:00Z",
  }),
  row("psp-30s", "psp-gateway client timeout raised to 30 seconds after the PSP incident review", "decision", "payments", {
    project: "payments-api",
    project_id: "dp-payments-api",
    title: "psp-gateway client timeout: 30s",
    visibility: "org",
    valid_from: "2026-05-01T00:00:00Z",
    created_at: "2026-05-01T09:00:00Z",
  }),
  row("ckv1", "checkout v1 is the live checkout flow for all merchants", "fact", "payments", {
    project: "checkout-web",
    project_id: "dp-checkout-web",
    title: "checkout v1 is the live flow",
    status: "deprecated",
    visibility: "org",
    valid_from: "2025-06-01T00:00:00Z",
    valid_to: "2026-02-01T00:00:00Z",
    superseded_by: "dm-ckv2",
    created_at: "2025-06-01T08:00:00Z",
  }),
  row("ckv2", "checkout v2 replaced checkout v1 as the live checkout flow; v1 endpoints are frozen", "decision", "payments", {
    project: "checkout-web",
    project_id: "dp-checkout-web",
    title: "checkout v2 replaces checkout v1",
    visibility: "org",
    valid_from: "2026-02-01T00:00:00Z",
    created_at: "2026-02-01T08:00:00Z",
  }),
  row("jenkins", "production deploys go through the Jenkins pipelines in deploy-tools", "fact", "platform", {
    title: "Jenkins is the production deploy path",
    status: "deprecated",
    visibility: "org",
    valid_from: "2025-01-01T00:00:00Z",
    valid_to: "2026-03-01T00:00:00Z",
    superseded_by: "dm-argocd",
    created_at: "2025-01-05T08:00:00Z",
  }),
  row("argocd", "ArgoCD is the only supported production deploy path since March 2026", "decision", "platform", {
    title: "ArgoCD is the only deploy path",
    visibility: "org",
    valid_from: "2026-03-01T00:00:00Z",
    created_at: "2026-03-01T08:00:00Z",
  }),
  row("feast-100", "the feast online serving p99 target is 100ms", "fact", "data", {
    project: "feature-store",
    project_id: "dp-feature-store",
    title: "feast p99 serving target: 100ms",
    status: "deprecated",
    valid_from: "2025-08-01T00:00:00Z",
    valid_to: "2026-03-01T00:00:00Z",
    superseded_by: "dm-feast-50",
    created_at: "2025-08-01T08:00:00Z",
  }),
  row("feast-50", "the feast online serving p99 target tightened to 50ms after the fraud latency review", "decision", "data", {
    project: "feature-store",
    project_id: "dp-feature-store",
    title: "feast p99 serving target: 50ms",
    valid_from: "2026-03-01T00:00:00Z",
    created_at: "2026-03-02T08:00:00Z",
  }),
  row("decline", "decline code 05 spikes are issuer-side; retrying burns PSP quota and reads as fraud velocity", "pitfall", "payments", {
    project: "payments-api",
    project_id: "dp-payments-api",
    title: "decline code 05 spikes are issuer-side",
    created_at: "2026-06-12T14:00:00Z",
  }),
  row("recon", "reconcile PSP settlement files against ledger-service with the deploy CLI recon command", "howto", "payments", {
    project: "payments-api",
    project_id: "dp-payments-api",
    title: "reconciling PSP settlement files",
    created_at: "2026-05-18T09:00:00Z",
  }),
  row("minor-units", "all monetary amounts in the feature store are integer minor units by contract", "decision", "data", {
    project: "feature-store",
    project_id: "dp-feature-store",
    title: "feature store amounts are minor units",
    created_at: "2026-06-25T11:00:00Z",
  }),
  row("backfill", "backfill DAG must not run concurrently with the hourly ingest — partition locks deadlock", "pitfall", "data", {
    project: "feature-store",
    project_id: "dp-feature-store",
    title: "backfill DAG deadlocks the hourly ingest",
    created_at: "2026-07-01T16:00:00Z",
  }),
  row("opa-exc", "request a deploy exception via an override PR into infra-live/policies; OPA needs two maintainer approvals", "howto", "platform", {
    title: "requesting a deploy exception",
    visibility: "org",
    created_at: "2026-06-05T10:00:00Z",
  }),
  row("msk-disk", "MSK broker storage autoscaling is not enabled — disk expansion is a manual infra-live change", "fact", "platform", {
    title: "MSK storage autoscaling is off",
    created_at: "2026-06-28T13:00:00Z",
  }),
  row("raw-1", "raw candidate: settlement recon runs at 07:00 daily", "fact", "payments", {
    project: "payments-api",
    project_id: "dp-payments-api",
    status: "raw",
    created_at: "2026-07-09T07:30:00Z",
  }),
  row("cand-1", "candidate: browser autofill fires duplicate tokenization on new card forms", "pitfall", "payments", {
    project: "checkout-web",
    project_id: "dp-checkout-web",
    status: "candidate",
    created_at: "2026-07-08T15:00:00Z",
  }),
];

// ── demo mirror of the server contract (parity for the offline tour) ────
//
// The live archive arrives filtered, faceted, sorted and paged by the server.
// These reproduce that over the demo fixture so the tour is not a dead husk
// offline — and they double as the readable spec of what the server does.

const facetField: Record<FacetKey, (r: MemoryRow) => string> = {
  team: (r) => r.team_id,
  project: (r) => r.project_id ?? "none",
  kind: (r) => r.kind,
  status: (r) => r.status,
  visibility: (r) => r.visibility,
};

const labelFor: Record<FacetKey, (r: MemoryRow) => string> = {
  team: (r) => r.team,
  project: (r) => r.project ?? "org-shared",
  kind: (r) => r.kind,
  status: (r) => r.status,
  visibility: (r) => r.visibility,
};

const matchesFacets = (r: MemoryRow, f: ArchiveFilter, except?: FacetKey): boolean => {
  if (except !== "team" && f.team && r.team_id !== f.team) return false;
  if (except !== "project" && f.project && (r.project_id ?? "none") !== f.project) return false;
  if (except !== "kind" && f.kind && r.kind !== f.kind) return false;
  if (except !== "status" && f.status && r.status !== f.status) return false;
  if (except !== "visibility" && f.visibility && r.visibility !== f.visibility) return false;
  return true;
};

const matchesQuery = (r: MemoryRow, q: string): boolean => {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    r.content.toLowerCase().includes(needle) ||
    (r.title?.toLowerCase().includes(needle) ?? false)
  );
};

/** Cross-filtered facet menu: each dimension is counted against every OTHER
 *  active narrowing but never its own, so a shelf never shrinks itself away. */
function computeFacets(all: MemoryRow[], f: ArchiveFilter, at: Date | null): MemoryFacetMenu {
  const menu = emptyFacets();
  const dims: Record<FacetKey, "kinds" | "statuses" | "teams" | "visibilities" | "projects"> = {
    team: "teams",
    project: "projects",
    kind: "kinds",
    status: "statuses",
    visibility: "visibilities",
  };
  for (const key of FACET_KEYS) {
    const tally = new Map<string, { label: string; count: number }>();
    for (const r of all) {
      if (!matchesQuery(r, f.q ?? "")) continue;
      if (at && !validAt(r, at)) continue;
      if (!matchesFacets(r, f, key)) continue;
      const value = facetField[key](r);
      const entry = tally.get(value) ?? { label: labelFor[key](r), count: 0 };
      entry.count += 1;
      tally.set(value, entry);
    }
    menu[dims[key]] = [...tally.entries()]
      .map(([value, { label, count }]) => ({ value, label, count }))
      .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
  }
  return menu;
}

const sortKey = (r: MemoryRow, sort: ArchiveSort): number => {
  const t =
    sort === "valid_from" ? r.valid_from :
    sort === "valid_to" ? r.valid_to :
    r.created_at;
  const ms = t ? new Date(t).getTime() : NaN;
  return Number.isFinite(ms) ? ms : Number.NEGATIVE_INFINITY;
};

/**
 * Build a full ArchiveData page from the demo fixture: filter, cross-facet,
 * sort, window, and derive the (as_of-excluded) skeleton — the same envelope
 * the server returns, so the Archive cannot tell live from demo apart from the
 * `live` flag.
 */
export function demoArchive(
  all: MemoryRow[],
  filter: ArchiveFilter,
  page: number,
  pageSize: number,
  sort: ArchiveSort,
  dir: ArchiveDir,
  asOf: string | null,
): ArchiveData {
  const at = asOf ? new Date(asOf) : null;
  const facets = computeFacets(all, filter, at);

  // Skeleton: the filter WITHOUT as_of — the whole visible corpus's windows.
  const skeletonRows = all.filter((r) => matchesQuery(r, filter.q ?? "") && matchesFacets(r, filter));
  const skeleton: ValidityRow[] = skeletonRows.map((r) => ({
    id: r.id,
    status: r.status,
    valid_from: r.valid_from ?? null,
    valid_to: r.valid_to ?? null,
  }));

  // The page: same filter, WITH as_of, sorted then windowed.
  const matched = skeletonRows.filter((r) => (at ? validAt(r, at) : true));
  const sign = dir === "asc" ? 1 : -1;
  const ordered = [...matched].sort((a, b) => {
    const d = sortKey(a, sort) - sortKey(b, sort);
    return d !== 0 ? sign * d : 0;
  });
  const pages = Math.max(1, Math.ceil(ordered.length / pageSize));
  const safePage = Math.min(Math.max(0, page), pages - 1);
  const start = safePage * pageSize;

  return {
    live: false,
    total: ordered.length,
    facets,
    rows: ordered.slice(start, start + pageSize),
    skeleton,
    filter,
    page: safePage,
    pageSize,
    sort,
    dir,
    asOf,
  };
}

/** One screenful of catalog. The DOM never holds more rows than this. The
 *  server clamps `limit` to 1..200; 80 is the legible page the archive walks. */
export const PAGE_SIZE = 80;

/** The default (unfiltered, first-page, sorted-recent) demo view. */
export const DEMO_ARCHIVE: ArchiveData = demoArchive(
  DEMO_ROWS,
  {},
  0,
  PAGE_SIZE,
  "recent",
  "desc",
  null,
);

export function demoDetail(id: string): MemoryDetail {
  const m = DEMO_ROWS.find((r) => r.id === id) ?? DEMO_ROWS[0];
  const successor = m.superseded_by ? DEMO_ROWS.find((r) => r.id === m.superseded_by) : null;
  const predecessor = DEMO_ROWS.find((r) => r.superseded_by === m.id);
  const link = (r: MemoryRow, depth: number) => ({
    id: r.id,
    content: r.content,
    status: r.status,
    valid_from: r.valid_from,
    valid_to: r.valid_to,
    depth,
  });
  return {
    memory: m,
    provenance: {
      actor_kind: "pipeline",
      actor_id: "extract-worker",
      model_ref: "qwen:qwen-max",
      source_kind: "session_transcript",
      source_ref: "demo-session-114",
    },
    entities: [
      { name: "psp-gateway", kind: "service", team: m.team },
      { name: "retry backoff rules", kind: "concept", team: m.team },
    ],
    promotions: [
      {
        from_status: "raw",
        to_status: "candidate",
        policy_decision: "auto_approved",
        policy_rule: `${m.kind}.high_confidence`,
        reviewed_at: null,
        created_at: m.created_at,
      },
      {
        from_status: "candidate",
        to_status: "canonical",
        policy_decision: "approved",
        policy_rule: "human.maintainer",
        reviewed_at: m.created_at,
        created_at: m.created_at,
      },
    ],
    chain: {
      predecessors: predecessor ? [link(predecessor, -1)] : [],
      successors: successor ? [link(successor, 1)] : [],
    },
  };
}
