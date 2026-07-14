/*
 * Knowledge-base page data — the document layer, told honestly, told briefly.
 *
 * TWO RULES FOR THIS FILE:
 *
 * 1. HONESTY (inherited from the pitch page, enforced by kb-data.test.ts):
 *    every capability carries an explicit `status`, and the status is the truth
 *    of the build plan's status log, not the truth we would like.
 *
 *      shipped      — merged, tested, running.
 *      built_off    — merged and tested, DELIBERATELY NOT ENABLED. Exists
 *                     because "shipped" and "roadmap" are both lies about
 *                     publishing: the code is real, and no org publishes
 *                     anything until the extraction-quality gate clears —
 *                     a compiled page inherits the trustworthiness of the
 *                     memories under it.
 *      in_progress  — being built right now.
 *      roadmap      — designed, not built.
 *
 *    The rule cuts both ways: understating a shipped capability is as wrong as
 *    overstating an unbuilt one.
 *
 * 2. AUDIENCE (enforced by kb-data.test.ts): this page is read by visitors, not
 *    contributors. No file paths, no internal table names, no section-sign
 *    references to documents the reader has never seen. Evidence lines say what
 *    was verified in plain words; the diagrams carry the mechanics.
 */

export type Status = "shipped" | "built_off" | "in_progress" | "roadmap";

/** The nav rail's sections, in reading order — drives the shared SectionRail. */
export const KB_SECTIONS = [
  { id: "rot", nav: "The rot" },
  { id: "asymmetry", nav: "Asymmetry" },
  { id: "anatomy", nav: "Anatomy" },
  { id: "pipeline", nav: "Rebuild" },
  { id: "publishing", nav: "Publishing" },
  { id: "never", nav: "Never" },
  { id: "status", nav: "Status" },
];

export const STATUS_LABEL: Record<Status, string> = {
  shipped: "shipped",
  built_off: "built · not enabled",
  in_progress: "in progress",
  roadmap: "roadmap",
};

// ─────────────────────────────────────────────────────────────────────────────
// 1. The thesis
// ─────────────────────────────────────────────────────────────────────────────

export const THESIS =
  "A page is a projection over canonical memories: a compiled read, never a place knowledge lives. When the memory changes, the page recompiles. That is the whole anti-rot mechanism.";

export const THESIS_BODY =
  "Every wiki rots for one structural reason: the page is where the knowledge lives, so it drifts and nothing notices. Brainiac compiles the page from governed memories instead — the drift has nowhere to live.";

export const ROT_CAPTION =
  "Both pages take the same four hits. Only one of them is still telling the truth in December.";

// ─────────────────────────────────────────────────────────────────────────────
// 2. The asymmetry — the load-bearing design decision
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
    note: "Pages are built by permission-capped retrieval over signed memories — a team's private knowledge physically cannot enter an org-wide page.",
  },
  {
    from: "human edit on a page",
    to: "canonical memories",
    label: "re-extract",
    gate: "the same review gate",
    allowed: true,
    note: "Your edit becomes proposed knowledge and faces the same human review gate as any agent proposal. Only then does the page say it.",
  },
  {
    from: "composed page",
    to: "canonical memories",
    label: "direct write-back",
    allowed: false,
    note: "Bidirectional sync would make the wiki a second source of truth again. One door into truth, and a named human stands in it.",
  },
  {
    from: "agent",
    to: "page",
    label: "direct page write",
    allowed: false,
    note: "Agents propose memories; pages follow. An agent that can author a page can publish an unsigned belief with institutional formatting on it.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 3. The five properties — each drawn, each stamped
// ─────────────────────────────────────────────────────────────────────────────

export interface Property {
  key: string;
  title: string;
  status: Status;
  /** ONE sentence. The figure carries the mechanism; this carries the why. */
  body: string;
  /** What was verified, in visitor language — never a file path. */
  evidence: string;
}

export const PROPERTIES: Property[] = [
  {
    key: "projection",
    title: "Pages are projections",
    status: "shipped",
    body: "Resolve a contradiction and every page that cited the losing claim rebuilds onto the winner — nobody edits anything, nobody is asked to go check.",
    evidence:
      "measured on a live model: 100% claim coverage, zero unbacked claims, zero permission leaks",
  },
  {
    key: "lifecycle",
    title: "Shipped and intended are different colours",
    status: "shipped",
    body: "Printing intent in the same typeface as reality is how wikis lie, so a page renders what is real and what is planned as visibly different things.",
    evidence: "on every memory since the substrate release — visible on every composed page",
  },
  {
    key: "structure",
    title: "The config survives the summary",
    status: "shipped",
    body: "A retry policy is a table. Flatten it into a sentence and the numbers are gone. The memory keeps the artifact beside the sentence, and the page shows the real thing.",
    evidence: "artifacts are copied character-for-character onto the page — a model never retypes them",
  },
  {
    key: "health-gate",
    title: "A degraded corpus stops publishing",
    status: "built_off",
    body: "Below the floor, external publishing pauses and pages hold the last human-approved version. Silence beats confident staleness.",
    evidence: "verified: degrade the corpus in a test, and the live page keeps its last good revision",
  },
  {
    key: "round-trip",
    title: "Captured, never saved",
    status: "shipped",
    body: "Your text is never written into the page — the next rebuild would silently revert it. It becomes proposed knowledge, faces the same review as any agent, and the section then says it on its own.",
    evidence: "the button says captured, never saved. A test fails the build if that ever changes",
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
    status: "shipped",
    body: "A section is a query over memories — or pinned human prose that regeneration never touches.",
  },
  {
    n: "02",
    name: "Cap",
    status: "shipped",
    body: "Retrieval runs as a principal that can only see what the page's audience may see. The database refuses; nobody has to remember.",
  },
  {
    n: "03",
    name: "Compose",
    status: "shipped",
    body: "Your own model writes the prose and must cite a signed memory for every claim.",
  },
  {
    n: "04",
    name: "Diff & decide",
    status: "shipped",
    body: "Additive, fully-cited changes publish themselves. Anything that drops a published claim waits for a human.",
  },
  {
    n: "05",
    name: "Gate",
    status: "built_off",
    body: "Publishing pauses the moment the health score says the corpus can't be trusted outward.",
  },
  {
    n: "06",
    name: "Publish",
    status: "built_off",
    body: "One-way push to Git or Confluence. Org-visible knowledge only, a banner on every page.",
  },
];

export const DIRTY_LOOP =
  "A memory is superseded, deprecated, or loses a contradiction → every page that cited it is marked stale → the worker rebuilds them. Nobody schedules a doc review. There is nothing to review.";

// ─────────────────────────────────────────────────────────────────────────────
// 5. Confluence — the incumbent becomes a render target
// ─────────────────────────────────────────────────────────────────────────────

export const CONFLUENCE = {
  status: "built_off" as Status,
  headline: "You will not have to abandon your wiki. We will keep it honest.",
  body: "Switch publishing on and compiled pages push into the spaces your company already reads, with a banner on top and a source link behind every claim. Confluence stops competing for truth and becomes a display. This is merged and tested. It is not switched on.",
  invariants: [
    {
      title: "One-way, always",
      body: "When enabled, pages push and never pull. Edits made in the wiki are overwritten on the next rebuild, and the banner says so before you type.",
    },
    {
      title: "Org-visible only",
      body: "Publishing leaves the permission system behind, so only org-wide knowledge may leave. A private memory on a public page is an unrecoverable trust event.",
    },
    {
      title: "Health-gated",
      body: "A degraded corpus will pause the sync rather than broadcast itself at machine speed.",
    },
  ],
};

// ─────────────────────────────────────────────────────────────────────────────
// 6. Turning it on — scoping
// ─────────────────────────────────────────────────────────────────────────────

export const SCOPES = {
  status: "built_off" as Status,
  body: "An org-level switch, off by default — a layer you don't need is a layer you don't pay for. Publishing credentials are never stored: no database dump can ever write to your wiki.",
  rows: [
    {
      scope: "kb:read",
      body: "Read compiled pages — the console and the agent tools. What an agent's token carries.",
    },
    {
      scope: "kb:publish",
      body: "Sign a revision into the org's mouth and, with a target configured, into its wiki. The hardest scope to get.",
    },
  ],
};

// ─────────────────────────────────────────────────────────────────────────────
// 7. What it will never do — the section the incumbents don't have
// ─────────────────────────────────────────────────────────────────────────────

export const NEVER = [
  {
    title: "No bidirectional sync.",
    body: "No setting unlocks it. No enterprise tier either. A page that can write to truth without review is a second source of truth, and the rot is back.",
  },
  {
    title: "No agent writing a page directly.",
    body: "Agents propose memories. Pages are compiled from what survived a human.",
  },
  {
    title: "No LLM-invented diagrams.",
    body: "A hallucinated arrow between two services is indistinguishable from an architecture decision. Diagrams, when they come, are compiled from the graph — never imagined.",
  },
  {
    title: "No private memory on an external surface.",
    body: "The leak count on the publish path is a build failure at zero. We would rather publish a thinner page.",
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
  /** Only set for shipped phases: what the verification run showed, in words. */
  gate?: string;
  /** The gate's numbers, as chips — visitor-readable, no file names. */
  stats?: string[];
}

export const LADDER: Phase[] = [
  {
    id: "KB0",
    name: "Substrate",
    status: "shipped",
    body: "Every memory learned its lifecycle and kept its artifact — plus the Knowledge Health report everything later gates on.",
    gate: "Extraction quality re-measured on a live model after the change: inside the guardrail, no detectable regression.",
    stats: ["recall 0.38 vs 0.42 baseline", "precision 0.73 vs 0.81", "inside the ±0.15 noise band"],
  },
  {
    id: "KB1",
    name: "Document layer core",
    status: "shipped",
    body: "Pages, sections, revisions and the dependency index; the compose worker; per-claim citations; the publish policy.",
    gate: "Composition measured on a live model. The first number is quality; the other four are hard gates that fail the build.",
    stats: [
      "claim coverage 100%",
      "unbacked claims 0",
      "permission leaks 0",
      "human prose altered 0",
      "stale pages served 0",
    ],
  },
  {
    id: "KB2",
    name: "Read surfaces",
    status: "shipped",
    body: "The reader: every sentence opens the memory behind it. Agents read pages through the same tools they read memories, and pages scaffold themselves where knowledge crosses teams.",
  },
  {
    id: "KB3",
    name: "Publishing",
    status: "built_off",
    body: "Git and Confluence targets, token scopes, the org switch, the health breaker — merged, tested, off. Publishing amplifies the substrate, so it waits for the substrate's own quality gate.",
  },
  {
    id: "KB4",
    name: "Round-trip",
    status: "shipped",
    body: "An edit becomes proposed knowledge, faces review, and recomposes. Propagation itself is measured, so a wiki that stops self-healing goes red in front of a leader.",
  },
  {
    id: "KB5",
    name: "Public surfaces",
    status: "shipped",
    body: "This page, the pitch, the feature story — governed by the same honesty rule they describe, with tests that fail the build if a stamp overstates or understates.",
  },
];

/** The one thing a reader should be able to do: check us. */
export const CHECK_US =
  "Every claim on this page carries a stamp — shipped, built · not enabled, or roadmap — and automated tests fail the build if a stamp drifts from the truth in either direction. The full build plan, including the part we built and deliberately left switched off, ships in the open with the product.";
