/*
 * Library page data — the normative layer, told honestly, told briefly.
 *
 * TWO RULES FOR THIS FILE (inherited from the pitch and /kb, enforced by
 * library-data.test.ts):
 *
 * 1. HONESTY: every capability carries an explicit `status`, and the status is
 *    the truth of the build plan's status log, not the truth we would like.
 *    Almost everything on this page is `roadmap` — the Library is a design with
 *    one shipped ancestor (the drift detector) and one shipped artifact (this
 *    page). The rule cuts both ways: understating the detector, which runs
 *    today, would be as wrong as overstating the unbuilt rest.
 *
 *      shipped      — merged, tested, running.
 *      in_progress  — being built right now.
 *      roadmap      — designed, not built.
 *
 * 2. AUDIENCE: this page is read by visitors, not contributors. No file paths,
 *    no internal table names, no section-sign references to documents the
 *    reader has never seen. Evidence lines say what was verified in plain
 *    words; the diagrams carry the mechanics.
 */

export type Status = "shipped" | "in_progress" | "roadmap";

/** The nav rail's sections, in reading order — drives the shared SectionRail. */
export const LIBRARY_SECTIONS = [
  { id: "drift", nav: "The drift" },
  { id: "layers", nav: "Third layer" },
  { id: "anatomy", nav: "Anatomy" },
  { id: "loop", nav: "Life of a rule" },
  { id: "agents", nav: "For agents" },
  { id: "never", nav: "Never" },
  { id: "status", nav: "Status" },
] as const;

export const STATUS_LABEL: Record<Status, string> = {
  shipped: "shipped",
  in_progress: "in progress",
  roadmap: "roadmap",
};

// ─────────────────────────────────────────────────────────────────────────────
// 1. The thesis
// ─────────────────────────────────────────────────────────────────────────────

export const THESIS =
  "A standard is a governed artifact with vital signs: provenance behind it, adoption in front of it, and one gate — a named human — between a pattern and a rule. When practice drifts, the rule notices. Not a retro, six months late.";

export const THESIS_BODY =
  "Every org writes standards. Almost none can say whether practice follows them, because the guide lives in the one place practice never visits. Brainiac treats a rule the way it treats a memory: versioned, provenance-carrying, human-ratified — and then measures whether the org actually lives by it.";

export const DRIFT_CAPTION =
  "Both teams read the same guide in January. Nothing measured the drift, so nothing stopped it.";

// ─────────────────────────────────────────────────────────────────────────────
// 2. The third layer — where the Library sits and which flows exist
// ─────────────────────────────────────────────────────────────────────────────

export const LAYERS_INTRO =
  "Memories are the descriptive layer: what happened, what is true. The knowledge base is the compiled layer: what we know, assembled into pages. The Library is the normative layer: what we should do — coding standards per tech stack, and skills your coding agents can pull down and run. The two upper layers meet: adopted rules render as a page in the wiki your company already reads, and because it is a page, it cannot rot there either.";

export interface Flow {
  from: string;
  to: string;
  label: string;
  /** The gate this flow passes through, if any. */
  gate?: string;
  allowed: boolean;
  note: string;
}

export const INTAKE: Flow[] = [
  {
    from: "the drift detector",
    to: "rule candidate",
    label: "mined",
    gate: "triage",
    allowed: true,
    note: "Scheduled sweeps already listen across teams for the same practice solved different ways. An unclaimed drift becomes a candidate — with the evidence attached.",
  },
  {
    from: "a coding agent, mid-session",
    to: "rule candidate",
    label: "proposed",
    gate: "the same triage, rate-limited",
    allowed: true,
    note: "An agent that found a better pattern proposes it. The proposal joins the same queue as the mined candidates — deduplicated first, so ten agents finding the same thing make one candidate, not ten.",
  },
  {
    from: "candidate",
    to: "adopted rule",
    label: "ratify",
    gate: "a named human",
    allowed: true,
    note: "Only a maintainer adopts, deprecates, or retires a rule. The gate is the same one every memory passes — one door into anything normative.",
  },
  {
    from: "any agent or sweep",
    to: "adopted rule",
    label: "direct write",
    allowed: false,
    note: "A model that can decree a rule can put institutional authority on an unreviewed guess. This path does not exist, at any tier.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 3. The anatomy — five properties, each drawn, each stamped
// ─────────────────────────────────────────────────────────────────────────────

export interface Property {
  key: string;
  title: string;
  status: Status;
  /** ONE idea. The figure carries the mechanism; this carries the why. */
  body: string;
  /** What was verified — or where the commitment lives — in visitor language. */
  evidence: string;
}

export const PROPERTIES: Property[] = [
  {
    key: "detector",
    title: "The drift detector already runs",
    status: "shipped",
    body: "Two teams solve the same problem at slightly different frequencies — each locally reasonable, the beat only audible org-wide. A scheduled sweep names the practice, files the divergence, and recommends one standard. Today that recommendation lands on a board; the Library is where it becomes an artifact.",
    evidence:
      "running today: sweeps file practice divergences with a recommended standard, adjudicated by the org's own model, ratified by a human",
  },
  {
    key: "atom",
    title: "The rule is the atom",
    status: "shipped",
    body: "Not a forty-page style guide — one rule, individually addressed: its stack, its statement, a good and a bad example, how strongly it binds, and whether it is proposed, adopted, or retired. You cannot measure a document. You can measure a rule.",
    evidence:
      "verified: rules are stored, versioned, and served one at a time, fetched by stack over the same permission-aware surface as memories",
  },
  {
    key: "provenance",
    title: "No unattributed rules",
    status: "shipped",
    body: "Every adopted rule carries the memories behind it — the incident, the resolved dispute, the drift that motivated it — or an explicit mark naming the human who decreed it without evidence. A rule that cannot say why it exists cannot ask anyone to follow it.",
    evidence:
      "verified: the database itself refuses an adoption with neither evidence nor a named signature — no code path can skip it",
  },
  {
    key: "skills",
    title: "Skills are versioned bundles",
    status: "shipped",
    body: "A skill is a packaged procedure your coding agents already know how to load — stored in the same governed store, versioned like a release, served over the same permission-aware surface as everything else. The org's best prompts stop living in one person's dotfiles.",
    evidence:
      "verified: only versions a named human published are ever served — a draft returns nothing, to humans and agents alike",
  },
  {
    key: "vitals",
    title: "Adoption is a vital sign",
    status: "shipped",
    body: "Fetches, checks, and usage flow back per team — never per person. A rule the org adopted and then quietly stopped following turns up on the leadership report by itself, beside the score; a skill nobody has pulled in a month does too. The library that cannot rot, because rot has a number and the number goes red where a leader is already looking.",
    evidence:
      "verified: a rule unused for a month raises an attention item on its own — and a rule adopted yesterday does not, because new is not dead",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 4. The life of a rule — the loop as one rail
// ─────────────────────────────────────────────────────────────────────────────

export interface Stage {
  n: string;
  name: string;
  status: Status;
  body: string;
}

export const RULE_STAGES: Stage[] = [
  {
    n: "01",
    name: "Detect",
    status: "shipped",
    body: "A sweep hears two teams solving one problem two ways and names the practice.",
  },
  {
    n: "02",
    name: "Triage",
    status: "shipped",
    body: "Mined and proposed candidates land in one queue, deduplicated — rejected ones stay rejected.",
  },
  {
    n: "03",
    name: "Adopt",
    status: "shipped",
    body: "A named human ratifies the rule: statement, examples, how strongly it binds.",
  },
  {
    n: "04",
    name: "Serve",
    status: "shipped",
    body: "Agents fetch the rules for their stack at session start — and the skills to apply them.",
  },
  {
    n: "05",
    name: "Measure",
    status: "shipped",
    body: "Adoption and drift flow back per team. The rule has a pulse anyone can read.",
  },
  {
    n: "06",
    name: "Retire",
    status: "shipped",
    body: "A rule practice has abandoned surfaces itself, and a human retires it in the open — not silently ignored forever.",
  },
];

export const LOOP_LEDE =
  "Nobody schedules a standards review. The loop runs: drift is detected, the mining sweep files candidates into one deduplicated queue, a human adopts or rejects — and a rejection is remembered, not re-asked — agents fetch, usage flows back per team, and a rule practice has abandoned raises its hand on the leadership report. The whole loop runs today. The one thing it will never do is retire a rule for you: nothing normative changes without a named human, and that includes taking something away.";

// ─────────────────────────────────────────────────────────────────────────────
// 5. For agents — the programmatic surface
// ─────────────────────────────────────────────────────────────────────────────

export const AGENTS = {
  status: "shipped" as Status,
  headline: "Your agents pull the org's judgment, not just its facts.",
  body: "The same tools an agent already uses to search memories now serve the Library: fetch the adopted rules for a stack before writing code, pull a published skill bundle by name, report usage back — counted for the team, never the person. And proposing a pattern is one more tool call — which makes a candidate, never a rule.",
  rows: [
    {
      scope: "lib:read",
      body: "Fetch adopted rules and published skill bundles; report usage. What an agent's token carries.",
    },
    {
      scope: "lib:propose",
      body: "Submit a pattern as a candidate — rate-limited per hour, collapsed onto anything the org already decided. An agent proposing a rejected idea is told so, instead of reopening the argument.",
    },
    {
      scope: "lib:publish",
      body: "Adopt, deprecate, ratify. The maintainer scope — a token minted to read standards must not be able to decree one.",
    },
  ],
};

// ─────────────────────────────────────────────────────────────────────────────
// 6. What it will never do
// ─────────────────────────────────────────────────────────────────────────────

export const NEVER = [
  {
    title: "Never a leaderboard.",
    body: "Usage is counted by team, never by person — enforced where the data is written, not by a dashboard's good manners. Telemetry that can rank people gets gamed, then feared, and the signal dies with the trust.",
  },
  {
    title: "No agent decrees a rule.",
    body: "Agents and sweeps propose candidates. A named human adopts. The same asymmetry that governs memories and pages, for the same reason.",
  },
  {
    title: "No silent enforcement.",
    body: "The Library informs — agents fetch rules and check themselves. It does not fail your build or block your merge. If enforcement ever comes, it arrives opt-in, announced, with its own consent design.",
  },
  {
    title: "No rules without a why.",
    body: "Provenance or a named decree — there is no third kind. An orphaned convention with no evidence and no owner is exactly the thing this layer exists to retire.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 7. The status ladder — the honesty spine of this whole page
// ─────────────────────────────────────────────────────────────────────────────

export interface Phase {
  id: string;
  name: string;
  status: Status;
  body: string;
}

export const LADDER: Phase[] = [
  {
    id: "LB0",
    name: "Substrate",
    status: "shipped",
    body: "Rules and skills as first-class records with the same row-level permissions as everything else — and the bridge that turns a ratified drift into a rule candidate carrying its evidence. Two promises moved into the database itself: a rule cannot be adopted without evidence or a named signature, and usage has nowhere to store a person.",
  },
  {
    id: "LB1",
    name: "Distribution",
    status: "shipped",
    body: "The read surface shipped first: agents fetch adopted rules by stack and published skills by name, usage flows back by team, and the maintainer gate — ratify, adopt, deprecate — holds its own key that a reading token never carries.",
  },
  {
    id: "LB2",
    name: "Console",
    status: "shipped",
    body: "The standards tree — stack, category, rule — and the skills catalog, each rule with its provenance and its readable pulse; the gate's controls mirror the database exactly, and adopting an evidence-free rule spells out the decree before the second click.",
  },
  {
    id: "LB3",
    name: "Mining",
    status: "shipped",
    body: "Sweeps propose candidates from unclaimed drifts, reinforced practices, and settled disputes — a generator, never an authority. Saying no once means not being asked again: rejections are remembered for a season, then the signal may earn a second look.",
  },
  {
    id: "LB4",
    name: "Agent proposals",
    status: "shipped",
    body: "Agents propose patterns mid-session — rate-limited per hour, collapsed onto anything the org already decided, and marked with their origin so a maintainer always sees who is asking. Deliberately last, and the sequencing paid off: the dedup that tames the noisy channel is the one the mining sweep proved first.",
  },
  {
    id: "LB5",
    name: "This page",
    status: "shipped",
    body: "The public explanation, governed by the same honesty rule it describes — with tests that fail the build if a stamp overstates or understates.",
  },
];

/** The one thing a reader should be able to do: check us. */
export const CHECK_US =
  "Every phase on this ladder is now built, tested, and running — and that sentence is worth exactly as much as the tests behind it, so: automated tests pin every stamp here to the build plan's status log and fail the build if one drifts in either direction, in either direction meaning a roadmap stamp that quietly became shipped AND a shipped capability we forgot to claim. The plan ships in the open with the product, including the parts we decided not to build and why.";
