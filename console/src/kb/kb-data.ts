/*
 * Knowledge-base page data — the document layer, told honestly.
 *
 * RULE FOR THIS FILE (inherited from pitch-data.ts:1-19, and stricter):
 * every capability on this page carries an explicit `status`, and the status is
 * the truth as of the KB-PLAN status log (docs/KB-PLAN.md), not the truth we
 * would like. A doc layer whose own marketing page describes an unbuilt phase as
 * shipped would be the exact failure this product exists to fix — a page that
 * quietly presents a roadmap intent as architecture. So:
 *
 *   shipped      — merged, tested, running. Check the KB-PLAN status log.
 *   in_progress  — being built right now (KB1). Named as such everywhere.
 *   roadmap      — designed, not built. Named as such everywhere.
 *
 * The three values are the same lifecycle facet the memory layer itself carries
 * (migration 0015: shipped | in_flight | proposed). We eat our own dog food:
 * this page's claims are lifecycle-split exactly like a composed page's claims.
 *
 * Sources for every number here:
 *   docs/KB-PLAN.md (phase ladder + status log)
 *   docs/ARCHITECTURE.md §8 (the document layer)
 *   results/kb0-extraction.json (the KB0 eval gate)
 */

export type Status = "shipped" | "in_progress" | "roadmap";

export const STATUS_LABEL: Record<Status, string> = {
  shipped: "shipped",
  in_progress: "in progress",
  roadmap: "roadmap",
};

// ─────────────────────────────────────────────────────────────────────────────
// 1. The thesis
// ─────────────────────────────────────────────────────────────────────────────

export const THESIS =
  "A page is a projection over canonical memories, not a second source of truth. When the memory changes, the page recompiles. That is the whole anti-rot mechanism.";

export const THESIS_BODY =
  "Every wiki rots for one structural reason: the page is where the knowledge lives, so the page can drift from reality and nothing in the system knows. Brainiac inverts it. The canonical memory graph is the only source of truth; a page is a compiled view over it, dirty-marked the moment a memory it cites is superseded, deprecated or resolved against. A contradiction adjudicated in the review queue propagates to every page that cited the losing claim — you do not go find the pages. There are no pages to go find.";

// ─────────────────────────────────────────────────────────────────────────────
// 2. The asymmetry — the load-bearing design decision (KB-PLAN D1)
// ─────────────────────────────────────────────────────────────────────────────

export interface Flow {
  from: string;
  to: string;
  label: string;
  /** The gate this flow passes through, if any. */
  gate?: string;
  allowed: boolean;
  note: string;
}

export const ASYMMETRY: Flow[] = [
  {
    from: "canonical memories",
    to: "composed page",
    label: "compose",
    gate: "visibility cap + policy",
    allowed: true,
    note: "Composition runs the section's binding through retrieval as a synthetic principal capped at the page's visibility tier. Team-private knowledge physically cannot enter an org page — the same RLS path that serves agents, not a filter the composer remembered to apply.",
  },
  {
    from: "human edit on a page",
    to: "canonical memories",
    label: "re-extract",
    gate: "the same review gate",
    allowed: true,
    note: "An edit to a composed section is not saved as prose. It goes back through extraction as candidate memories and faces the review queue like any other agent proposal. You get told: your change was captured as N proposed updates. A human editing the wiki is just another ingestion source.",
  },
  {
    from: "composed page",
    to: "canonical memories",
    label: "direct write-back",
    allowed: false,
    note: "Never. Bidirectional sync recreates the two-sources-of-truth problem the layer exists to eliminate. There is exactly one door into org truth and a named human stands in it.",
  },
  {
    from: "agent",
    to: "page",
    label: "direct page write",
    allowed: false,
    note: "Never. Agents write memories; pages follow from them. An agent that can author a page directly can author an unsigned belief with institutional formatting on it.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 3. The four properties — each with its honest status
// ─────────────────────────────────────────────────────────────────────────────

export interface Property {
  key: string;
  title: string;
  status: Status;
  claim: string;
  body: string;
  /** Where a reviewer can check this claim in the repo. */
  evidence: string;
}

export const PROPERTIES: Property[] = [
  {
    key: "projection",
    title: "Pages are projections",
    status: "in_progress",
    claim: "regenerated from memories, never edited into truth",
    body: "documents / document_sections / document_revisions / document_dependencies, the compose worker on the existing SKIP-LOCKED queue, and inline [m:uuid] citations that render as footnotes for humans and structured refs for agents. This is KB1 — being built now. Nothing about it is live yet, and this page will say so until it is.",
    evidence: "docs/ARCHITECTURE.md §8.1–8.2 · docs/KB-PLAN.md KB1",
  },
  {
    key: "lifecycle",
    title: "Shipped and intended are different colours",
    status: "shipped",
    claim: "lifecycle: shipped | in_flight | proposed, on every memory",
    body: "The most common way a wiki lies is by presenting a roadmap intent as shipped architecture, in the same typeface, with no marker. Every canonical memory now carries a lifecycle facet, extraction populates it, and the facet firewall coerces an unknown value to shipped rather than dropping the memory. Composed pages render the split: what is in the product, and what is on its way.",
    evidence: "migrations/0015_memory_facets.sql · docs/ARCHITECTURE.md §2.3",
  },
  {
    key: "structure",
    title: "The config survives the summary",
    status: "shipped",
    claim: "detail_md — a structure-preserving payload beside the sentence",
    body: "Extraction used to flatten everything to one distilled sentence, which is a hard quality ceiling for a page: a retry policy is a table, not a clause. Memories now carry an optional detail_md — the code block, the config snippet, the table — redacted through the same secret firewall as the content and clipped. A composed page can show you the actual thing.",
    evidence: "migrations/0015_memory_facets.sql · fixtures/v1/memories/gold.yaml",
  },
  {
    key: "health-gate",
    title: "A degraded corpus stops publishing",
    status: "roadmap",
    claim: "Knowledge Health as a circuit breaker, not a report",
    body: "The health composite is live today at /health — four pillars, consistency, currency, liquidity, governance. What is not built yet is wiring it as an actuator: when the currency or governance pillar drops below threshold, external sync PAUSES and pages hold their last revision behind a verification-pending stamp. An auto-synced wiki is an amplifier; a stalled review queue must not broadcast stale beliefs to the whole company at machine speed. That wiring is KB3.",
    evidence: "shipped: /health (KB0) · breaker: docs/KB-PLAN.md KB3, D7",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 4. How a page is built — the compose pipeline
// ─────────────────────────────────────────────────────────────────────────────

export interface Stage {
  n: string;
  name: string;
  status: Status;
  body: string;
}

export const COMPOSE_STAGES: Stage[] = [
  {
    n: "01",
    name: "Bind",
    status: "in_progress",
    body: "A section is either composed — bound to a memory query (entities, kinds, as-of) — or pinned: human prose that regeneration never touches. A page is a sequence of the two.",
  },
  {
    n: "02",
    name: "Cap",
    status: "in_progress",
    body: "The binding runs through retrieval as a synthetic principal capped at the page's visibility tier. Canonical memories only. An org page cannot see a team-private memory because Postgres refuses, not because the composer filtered.",
  },
  {
    n: "03",
    name: "Compose",
    status: "in_progress",
    body: "Your model writes the section prose and cites its sources inline as [m:uuid]. Every claim on a page points back at a memory that a named human signed for. A claim with no citation does not auto-publish.",
  },
  {
    n: "04",
    name: "Diff & decide",
    status: "in_progress",
    body: "The new revision is diffed against the current one. A typed policy engine decides: a small additive diff with every claim traceable auto-publishes; a structural change or the deletion of a previously published claim goes to the same review queue as a promotion.",
  },
  {
    n: "05",
    name: "Gate",
    status: "roadmap",
    body: "Before any external publish, the Knowledge Health composite is consulted. Currency or governance below threshold: sync pauses, the page holds its last revision, and it says so on its face.",
  },
  {
    n: "06",
    name: "Publish",
    status: "roadmap",
    body: "One Publisher trait, pluggable targets: Git (docs/, semver) and Confluence (PAT). One-way, org-visible only, generated-content banner, provenance links back to the console.",
  },
];

export const DIRTY_LOOP =
  "A canonical memory is inserted, superseded or deprecated → document_dependencies (the inverted index: which pages this memory feeds) → every dependent page is marked dirty → the compose worker rebuilds it. Nobody schedules a doc review. Nobody is asked to go check whether the page is still true.";

// ─────────────────────────────────────────────────────────────────────────────
// 5. Confluence — the incumbent becomes a render target (roadmap, KB3)
// ─────────────────────────────────────────────────────────────────────────────

export const CONFLUENCE = {
  status: "roadmap" as Status,
  headline: "You do not have to abandon your wiki. We keep it honest.",
  body: "The Confluence adapter is designed and unbuilt — it is phase KB3 in docs/KB-PLAN.md. When it lands it is a one-way render target over a PAT: Brainiac pushes composed pages into the spaces your company already reads, with a generated-content banner and links back to the provenance. Confluence stops being a competing source of truth and becomes a surface.",
  invariants: [
    {
      title: "One-way, always",
      body: "Pages are pushed, never pulled. Direct edits in Confluence are overwritten on the next compose. Harvesting them back as an ingestion source is a later increment, not a promise we make on day one.",
    },
    {
      title: "org-visible memories only",
      body: "External publish leaves RLS behind entirely, so only org-visible canonical memories may compose into a synced page. Team and private knowledge renders in the console and nowhere else. A leaked private memory in a company wiki is not a score deduction — it is an unrecoverable trust event.",
    },
    {
      title: "Health-gated",
      body: "A degraded corpus pauses the sync instead of broadcasting it. The eval gate for the publish path is zero leaks, as a build failure, not a warning.",
    },
  ],
};

// ─────────────────────────────────────────────────────────────────────────────
// 6. Turning it on — scoping (roadmap, KB3)
// ─────────────────────────────────────────────────────────────────────────────

export const SCOPES = {
  status: "roadmap" as Status,
  body: "The KB layer is designed as an org capability flag, with KB scopes on API tokens alongside the existing memory scopes. Our own controlled trial says the memory layer is dead weight on single-team work; a layer nobody needs should be a layer nobody pays for. Neither the flag nor the scopes are implemented yet — they land with KB3.",
  rows: [
    { scope: "kb:read", body: "Read composed pages — console and, later, the MCP doc tools. The scope an agent's token should carry." },
    { scope: "kb:compose", body: "Trigger a recompose. Held by the worker and by maintainers, not by every agent." },
    { scope: "kb:publish", body: "Push to an external target. The one that can broadcast, and therefore the one that is hardest to get." },
  ],
};

// ─────────────────────────────────────────────────────────────────────────────
// 7. What it will never do — the section the incumbents don't have
// ─────────────────────────────────────────────────────────────────────────────

export const NEVER = [
  {
    title: "No bidirectional sync.",
    body: "Not as a setting, not as an enterprise tier. The moment a page can write to truth without the review gate, the wiki is a second source of truth again and rot is back. The asymmetry is the product.",
  },
  {
    title: "No agent writing a page directly.",
    body: "Agents propose memories. Pages are compiled from the memories that survived a human. An agent that can author a page can author an unsigned belief that looks institutional.",
  },
  {
    title: "No LLM-invented diagrams.",
    body: "A hallucinated arrow between two services is indistinguishable from an architecture decision. The only diagrams on the roadmap are deterministic projections of the entity graph — compiled from edges that already exist, zero model involvement. LLM-proposed diagrams, if they ever ship, enter through the same review gate as prose, and every edge must cite a memory.",
  },
  {
    title: "No private memory on an external surface.",
    body: "External publish is org-visibility only, and the leak count in the eval is zero-as-a-build-failure. We would rather publish a thinner page.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 8. The status ladder — the honesty spine of this whole page
// ─────────────────────────────────────────────────────────────────────────────

export interface Phase {
  id: string;
  name: string;
  status: Status;
  body: string;
  /** Only set for shipped phases: how a reviewer verifies it. */
  gate?: string;
}

export const LADDER: Phase[] = [
  {
    id: "KB0",
    name: "Substrate",
    status: "shipped",
    body: "The memory lifecycle facet (shipped | in_flight | proposed) and detail_md structure-preserving payloads, end to end: migration, core types, extraction prompt + facet firewall, store, retrieval, fixtures and gold. Plus the Knowledge Health console page at /health.",
    gate: "Extraction eval on real qwen-max: recall 0.381, precision 0.727 vs the 0.417 / 0.806 baseline — inside the gate, and mid-band of the documented identical-config spread. One noisy sample: it shows no detectable regression, it does not prove the facets are free. results/kb0-extraction.json.",
  },
  {
    id: "KB1",
    name: "Document layer core",
    status: "in_progress",
    body: "The tables, the RLS, the compose job on the queue, dirty-marking, [m:uuid] citations, the diff and the auto-publish policy — plus a docs eval profile whose gates are hallucination = 0 for anything auto-published and leak = 0 as a build failure. Under construction as you read this.",
  },
  {
    id: "KB2",
    name: "Read surfaces",
    status: "roadmap",
    body: "Console page reader with per-claim provenance popovers and revision diffs; MCP doc_get / doc_search so an agent reads pages the way it reads memories; entity pages auto-scaffolded where the knowledge actually is rather than where someone remembered to create a page.",
  },
  {
    id: "KB3",
    name: "Publishing",
    status: "roadmap",
    body: "The Publisher trait, the Git target, the Confluence adapter, the KB token scopes and org flag, and the health circuit breaker wired as an actuator.",
  },
  {
    id: "KB4",
    name: "Round-trip",
    status: "roadmap",
    body: "The human edit closing the loop: page edit → extraction → candidate memories → recompose, with the propagation SLA measured rather than asserted, and a docs signal feeding the health score.",
  },
];

/** The one thing a reader should be able to do: check us. */
export const CHECK_US =
  "Every claim on this page is labelled shipped, in progress, or roadmap, and each one names the file or the phase where you can check it. The plan, including the parts we have not built, is in the repo: docs/KB-PLAN.md.";
