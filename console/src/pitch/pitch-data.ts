/*
 * Pitch page data — the competitive case, with citations.
 *
 * RULE FOR THIS FILE: every number that reaches the page is either
 *   (a) a primary-source competitor/industry fact, with `src` set, or
 *   (b) one of OUR measured numbers, reproducible from this repo.
 *
 * Claims the research flagged as laundered or unverifiable are deliberately
 * ABSENT — notably the McKinsey "19% of time searching", the "Fortune 500 lose
 * $31.5B" figure, and the Zoomin staleness stat. A pitch that gets fact-checked
 * on a fake number loses everything the real evidence bought.
 *
 * Our numbers come from:
 *   results/history/2026-07-10-retrieval-qwen-text-embedding-v4.json
 *   results/contradiction-baseline.json
 *   results/extraction-baseline.json
 *   results/resolution-baseline.json
 *   uat/runs/2026-07-13-l2-real/report.md
 */

export interface Cite {
  label: string;
  href: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. The capture gap — why the incumbents structurally cannot win
// ─────────────────────────────────────────────────────────────────────────────

export interface GapStat {
  value: string;
  claim: string;
  note: string;
  cite: Cite;
}

export const CAPTURE_GAP: GapStat[] = [
  {
    value: "56%",
    claim: "say the only way to get the information they need is to ask a person or book a meeting.",
    note:
      "This is the exact fraction of org knowledge that no index can reach — published by the vendor of the incumbent wiki.",
    cite: {
      label: "Atlassian, State of Teams 2025 (n=12,000)",
      href: "https://www.atlassian.com/blog/state-of-teams-2025",
    },
  },
  {
    value: "42%",
    claim: "of institutional knowledge is unique to one individual and written down nowhere.",
    note: "When they leave, it leaves. Search cannot retrieve what was never text.",
    cite: {
      label: "Panopto Workplace Knowledge Report, 2018 (n=1,000)",
      href: "https://www.hrdive.com/news/inefficient-knowledge-sharing-costs-large-us-businesses-47m-a-year/527892/",
    },
  },
  {
    value: "61%",
    claim: "of developers spend more than 30 minutes a day just searching for answers.",
    note: "The cost is paid daily, by everyone, forever.",
    cite: {
      label: "Stack Overflow Developer Survey 2025",
      href: "https://survey.stackoverflow.co/2025",
    },
  },
  {
    value: "−7.2%",
    claim: "delivery stability per 25% increase in AI adoption — while documentation quality rose 7.5%.",
    note:
      "Google's own data. AI writes more prose and ships worse software. Volume of documentation is not knowledge.",
    cite: {
      label: "DORA, Accelerate State of DevOps 2024",
      href: "https://dora.dev/research/2024/dora-report/",
    },
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 2. The bifurcation — the strategic core of the whole pitch
// ─────────────────────────────────────────────────────────────────────────────

/**
 * x = enforced authorization: is who-may-read-this a database boundary, or a
 *     filter parameter the caller supplies?
 * y = governed truth: does the system capture tacit knowhow, attach provenance,
 *     gate it on human review, and adjudicate contradictions?
 *
 * Scores are our reading of each vendor's PUBLIC DOCS (see `why`), not a
 * benchmark. They are arguable — which is the point of showing the reasoning.
 */
export interface Player {
  /** Short label drawn on the chart — long names collide into mush. */
  name: string;
  /** Full name, shown in the tooltip. */
  full: string;
  x: number;
  y: number;
  camp: "search" | "memory" | "neither" | "us";
  /** Which side of the dot the label sits on, chosen to avoid overlaps. */
  side: "left" | "right";
  why: string;
}

export const PLAYERS: Player[] = [
  // Camp 1 — retrieval that respects permissions but believes nothing.
  { name: "Glean", full: "Glean", x: 9.2, y: 1.7, camp: "search", side: "left", why: "Source-mirrored ACLs, enforced at retrieval. Its MCP verbs are search / read_document. No capture verb, no adjudication verb anywhere in the product." },
  { name: "ChatGPT", full: "ChatGPT company knowledge", x: 8.6, y: 0.6, camp: "search", side: "left", why: "\"ChatGPT can only see the content a user is already authorized to view.\" Persists nothing. It is search, not memory." },
  { name: "Gemini", full: "Gemini for Workspace", x: 9.6, y: 1.15, camp: "search", side: "left", why: "\"If you don't have permission to see a file, the AI can't see it or use it either.\" Stateless. Remembers nothing." },
  { name: "Confluence / Rovo", full: "Confluence + Atlassian Rovo", x: 7.0, y: 1.4, camp: "search", side: "left", why: "Space permissions and page restrictions. Freshness is a third-party marketplace app — which exists because the platform doesn't do it." },

  // Neither camp: no permissions AND no governed truth.
  { name: "Obsidian", full: "Obsidian", x: 0.5, y: 0.8, camp: "neither", side: "right", why: "In a shared vault, every collaborator inherits the owner's permissions. No review workflow, no provenance, no staleness detection. The graph view is a rendering of the link table: untyped edges, no evidence, no time." },

  // Camp 2 — memory that believes things but respects nothing.
  { name: "Mem0", full: "Mem0", x: 1.2, y: 1.8, camp: "memory", side: "right", why: "org_id is a filter parameter, not an authorization boundary — their own docs say applications \"still need proper authentication and access-control boundaries around those IDs.\" The default pipeline is now ADD-only." },
  { name: "Supermemory", full: "Supermemory", x: 5.2, y: 2.0, camp: "memory", side: "right", why: "Genuine namespace isolation at the data layer — but namespace-level, not fact-level. No human review, no provenance, no org-level memory." },
  { name: "Zep / Graphiti", full: "Zep / Graphiti", x: 2.2, y: 5.2, camp: "memory", side: "right", why: "The best data model in the market: bi-temporal, episode lineage, real contradiction supersession. But it resolves conflicts by silent last-writer-wins, and nothing gates who may read a group graph." },
  { name: "Cognee", full: "Cognee", x: 7.0, y: 3.2, camp: "memory", side: "left", why: "The only independent with DB-layer tenant/role/user grants resolved before the query runs. But granularity is the dataset, not the fact — and the docs contain nothing on review, provenance, or contradiction." },
  { name: "Copilot Memory", full: "GitHub Copilot Memory", x: 5.4, y: 5.0, camp: "memory", side: "right", why: "Repo-scoped and write-access-gated; facts carry code citations and re-verify against the code on session start. But review is post-hoc delete, and no audit event fires when a memory is created or used." },
  { name: "Memory Stores", full: "Anthropic Memory Stores", x: 3.0, y: 6.0, camp: "memory", side: "right", why: "Immutable memver_ version chain, session attribution, filesystem-enforced read_only, compliance-grade redaction. But no permission-aware retrieval — isolation is by sharding into separate stores — and the review gate is explicitly left for you to build." },

  // The join.
  { name: "Brainiac", full: "Brainiac", x: 9.4, y: 9.3, camp: "us", side: "left", why: "Postgres row-level security runs inside the pgvector scan itself — an agent cannot retrieve what its operator cannot read, because the database refuses. And nothing becomes canonical without a named human approving it." },
];

export const BIFURCATION_LINE =
  "The industry has cleanly bifurcated into retrieval that respects permissions but believes nothing, and memory that believes things but respects nothing. Nobody has joined them.";

// ─────────────────────────────────────────────────────────────────────────────
// 3. Benchmark theater — the LOCOMO war
// ─────────────────────────────────────────────────────────────────────────────

/** One system (Zep), one benchmark, 14 months, a 36-point spread. */
export const LOCOMO_WAR = [
  { when: "Apr 2025", score: 65.99, who: "Mem0's paper scores Zep", tone: "rival" as const },
  { when: "May 2025", score: 84.0, who: "Zep scores Zep", tone: "self" as const },
  { when: "May 2025", score: 58.44, who: "Mem0 re-runs Zep's own code", tone: "rival" as const },
  { when: "May 2025", score: 75.14, who: "Zep silently edits the blog post", tone: "self" as const },
  { when: "2026", score: 94.7, who: "Zep's marketing site today", tone: "self" as const },
];

/** The ceiling the vendors are claiming to have cleared. */
export const LOCOMO_CEILING = 93.6;

export const LOCOMO_FACTS = [
  {
    stat: "6.4%",
    claim: "of LOCOMO's answer key is simply wrong",
    detail:
      "99 score-corrupting errors in 1,540 questions — hallucinated facts (the key says \"Ferrari 488 GTB\"; the transcript says \"a red sports car\"), broken date arithmetic, 24 speaker-attribution errors. The mathematical ceiling for a perfect system is 93.6%.",
    cite: {
      label: "Penfield Labs — audit of the LOCOMO answer key",
      href: "https://dev.to/penfieldlabs/we-audited-locomo-64-of-the-answer-key-is-wrong-and-the-judge-accepts-up-to-63-of-intentionally-33lg",
    },
  },
  {
    stat: "62.8%",
    claim: "of intentionally wrong answers are accepted by the standard judge",
    detail:
      "Vague answers that name the right topic and omit every detail pass nearly two-thirds of the time. The benchmark actively rewards mushy retrieval.",
    cite: {
      label: "Penfield Labs",
      href: "https://dev.to/penfieldlabs/we-audited-locomo-64-of-the-answer-key-is-wrong-and-the-judge-accepts-up-to-63-of-intentionally-33lg",
    },
  },
  {
    stat: "74.0%",
    claim: "is what a filesystem and grep score — beating the specialized memory infrastructure",
    detail:
      "Letta ran gpt-4o-mini with nothing but search_files and grep against Mem0's reported 68.5% for its best graph variant. Nobody has disputed the result in eleven months. If a vendor cannot beat grep, the vendor is selling you infrastructure, not intelligence.",
    cite: {
      label: "Letta — Is a Filesystem All You Need?",
      href: "https://www.letta.com/blog/benchmarking-ai-agent-memory/",
    },
  },
  {
    stat: "91.4%",
    claim: "is the only accuracy number in this market that survives cross-examination",
    detail:
      "Hindsight (Vectorize) on LongMemEval — independently reproduced by Virginia Tech's Sanghani Center with The Washington Post as research collaborator. A university lab and a newspaper, neither of whom sells a memory product, ran the system and got the number. That is the entire bar. It is the only one in this market that clears it.",
    cite: {
      label: "arXiv 2512.12818",
      href: "https://arxiv.org/abs/2512.12818",
    },
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 4. The retreat — they didn't fail to notice the problem. They walked away.
// ─────────────────────────────────────────────────────────────────────────────

export const RETREAT = [
  {
    who: "Cursor",
    what: "deleted Memories",
    when: "v2.1.17",
    detail:
      "Shipped in 1.0 as beta, never left beta, removed ~17 months later with no changelog entry. What they bet on instead: Team Rules — admin-authored, dashboard-managed, non-disableable. The most aggressive AI IDE on the market built implicit auto-memory, looked at it, and replaced it with rules a human wrote.",
    cite: { label: "Cursor forum — staff confirmation", href: "https://forum.cursor.com/t/memories-not-showing/143820" },
  },
  {
    who: "Windsurf",
    what: "retired Cascade",
    when: "1 Jul 2026",
    detail:
      "Its own docs disclaimed the feature before it died: \"For knowledge you want Cascade to reliably reuse, write it as a Rule — rather than relying on auto-generated Memories.\" The vendor told you not to trust the vendor's memory.",
    cite: { label: "Windsurf is now Devin Desktop", href: "https://devin.ai/" },
  },
  {
    who: "OpenAI",
    what: "made memory unauditable",
    when: "Jun 2026",
    detail:
      "\"Dreaming V3\" replaced the enumerable, user-editable saved-memories list with opaque background synthesis. Recall scores went up. There is now no per-fact provenance and no way to ask \"why do you believe this about me.\"",
    cite: { label: "OpenAI memory docs", href: "https://help.openai.com/en/articles/8590148-memory-faq" },
  },
  {
    who: "Mem0",
    what: "removed contradiction handling",
    when: "Apr 2026",
    detail:
      "Their engineering blog: the new algorithm \"collapses a two-pass extraction process into one, eliminating UPDATE and DELETE operations.\" Their docs now describe the pipeline as \"additive — new memories are added without overwriting or deleting existing memories.\" Say you're vegetarian in March and a meat-eater in June: both are memories, both retrievable, and nothing in the system knows the first one is dead. The category leader traded correctness for latency.",
    cite: {
      label: "Mem0 — the token-efficient memory algorithm",
      href: "https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm",
    },
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 5. The capability matrix
// ─────────────────────────────────────────────────────────────────────────────

export type Cell = "yes" | "partial" | "no";

export interface MatrixRow {
  capability: string;
  detail: string;
  cells: Record<string, Cell>;
}

export const MATRIX_VENDORS = ["Mem0", "Zep", "Cognee", "Copilot", "Memory Stores", "Glean", "Brainiac"] as const;

export const MATRIX: MatrixRow[] = [
  {
    capability: "Org-level learned memory",
    detail: "Knowledge captured from one person's session reaches another person's agent.",
    cells: { Mem0: "partial", Zep: "partial", Cognee: "yes", Copilot: "partial", "Memory Stores": "yes", Glean: "no", Brainiac: "yes" },
  },
  {
    capability: "Permission-aware retrieval, enforced by the database",
    detail: "Not a filter the caller passes — a boundary the storage layer refuses to cross.",
    cells: { Mem0: "no", Zep: "no", Cognee: "partial", Copilot: "partial", "Memory Stores": "no", Glean: "yes", Brainiac: "yes" },
  },
  {
    capability: "Per-fact provenance",
    detail: "Who asserted this, from which session, with which model, and when.",
    cells: { Mem0: "no", Zep: "partial", Cognee: "no", Copilot: "partial", "Memory Stores": "yes", Glean: "no", Brainiac: "yes" },
  },
  {
    capability: "Human review before canonical",
    detail: "An agent proposes. A named human promotes. Nothing becomes org truth on its own.",
    cells: { Mem0: "no", Zep: "no", Cognee: "no", Copilot: "no", "Memory Stores": "no", Glean: "no", Brainiac: "yes" },
  },
  {
    capability: "Contradiction adjudication",
    detail: "Two sources disagree right now — surface it to a reviewer, don't silently pick one.",
    cells: { Mem0: "no", Zep: "partial", Cognee: "no", Copilot: "partial", "Memory Stores": "partial", Glean: "no", Brainiac: "yes" },
  },
  {
    capability: "Temporal “as of”",
    detail: "What did we believe in March, and what superseded it?",
    cells: { Mem0: "no", Zep: "partial", Cognee: "partial", Copilot: "no", "Memory Stores": "partial", Glean: "no", Brainiac: "yes" },
  },
  {
    capability: "Bring your own model, self-hosted",
    detail: "The transcripts never leave your infrastructure.",
    cells: { Mem0: "partial", Zep: "no", Cognee: "yes", Copilot: "no", "Memory Stores": "no", Glean: "no", Brainiac: "yes" },
  },
];

/** The row that is empty for everyone but us — the thesis, in one line. */
export const EMPTY_ROW = "Human review before canonical";

// ─────────────────────────────────────────────────────────────────────────────
// 6. Our evidence — every number reproducible from this repo
// ─────────────────────────────────────────────────────────────────────────────

/** results/history/2026-07-10-retrieval-qwen-text-embedding-v4.json */
export const RETRIEVAL = {
  model: "qwen:text-embedding-v4",
  queries: 54,
  ndcg: 0.8765,
  mrr: 0.8566,
  recallAt5: 0.9271,
  temporalRank1: 0.9286,
  supersededInTop3: 0,
  rlsLeaks: 0,
  strata: [
    { name: "exact identifier", ndcg: 0.9649, n: 11 },
    { name: "cross-team graph", ndcg: 0.9262, n: 9 },
    { name: "temporal", ndcg: 0.9051, n: 6 },
    { name: "semantic", ndcg: 0.8114, n: 16 },
    { name: "non-English", ndcg: 0.7849, n: 6 },
  ],
};

/** results/contradiction-baseline.json + resolution + extraction */
export const PIPELINE = {
  contradiction: { precision: 1.0, recall: 0.75, direction: 1.0, falsePositive: 0.0 },
  resolution: { bCubedF1: 0.7943 },
  extraction: { precision: 0.8058, recall: 0.4167, f1: 0.5229 },
};

/**
 * uat/runs/2026-07-13-l2-real/report.md — the three-arm controlled trial.
 * A = cold agent. B = Claude's native memory, built generously. C = Brainiac.
 * The verdict is C − B, and it was allowed to come out negative.
 */
export interface UatJourney {
  key: string;
  title: string;
  gap: string;
  question: string;
  arms: { arm: "A" | "B" | "C"; label: string; correct: boolean | "partial"; tokens: number; verdict: string }[];
  reading: string;
}

export const UAT: UatJourney[] = [
  {
    key: "cross-team",
    title: "The answer lives in another team's repo",
    gap: "cross-team",
    question:
      "A data engineer must set a dedup window that matches the payments team's refund-worker retry cap. The number is not in her repo, and never will be.",
    arms: [
      { arm: "A", label: "Cold agent", correct: false, tokens: 3022, verdict: "Refused — “point me to the payments repo”" },
      { arm: "B", label: "Native memory baseline", correct: false, tokens: 1909, verdict: "“VALUE: unknown — cannot be determined from this repository”" },
      { arm: "C", label: "Brainiac", correct: true, tokens: 1160, verdict: "VALUE: 30 — cites mem-pay-0043 and its 2026-04-01 date" },
    ],
    reading:
      "A and B do not merely lose — they cannot play. All four cold/baseline runs correctly refused rather than guess. Only the arm that could read across the boundary finished the job.",
  },
  {
    key: "after-the-file",
    title: "The knowledge arrived and nobody wrote it down",
    gap: "after-the-file",
    question:
      "A web developer must judge a 15s client abort now that payments quietly raised the PSP timeout to 30s. The file that would have told her is stale.",
    arms: [
      { arm: "A", label: "Cold agent", correct: "partial", tokens: 2008, verdict: "Guessed “too low” — but marked it “inferred, not verified”" },
      { arm: "B", label: "Native memory baseline", correct: false, tokens: 2358, verdict: "“VERDICT: ok, VALUE: 15000” — ships a live double-charge bug" },
      { arm: "C", label: "Brainiac", correct: true, tokens: 731, verdict: "“too-low, 35000” — cites the timeout change and the double-charge pitfall" },
    ],
    reading:
      "The baseline was not slow here. It was confidently wrong, twice, and would have shipped the bug. A guess that happens to lean right is not knowing.",
  },
  {
    key: "control",
    title: "The control we expected to lose — and did",
    gap: "none",
    question:
      "A payments developer asks a question her own CLAUDE.md already answers. Brainiac should add nothing here. We ran it anyway.",
    arms: [
      { arm: "B", label: "Native memory baseline", correct: true, tokens: 417, verdict: "VALUE: 30 — in two turns, for free" },
      { arm: "C", label: "Brainiac", correct: true, tokens: 969, verdict: "VALUE: 30 — the same answer, at 2.3× the tokens" },
    ],
    reading:
      "Identical answer, 2.3× the cost. This is the redundancy tax, quantified — and it is published here because a trial that cannot lose is not a trial.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 7. The poisoning probe — the credibility centerpiece
// ─────────────────────────────────────────────────────────────────────────────

export const POISON = {
  premise:
    "A CLAUDE.md can only be wrong if someone on your team wrote something wrong. A shared memory store is a channel through which someone else's wrong belief — or a machine's hallucination — reaches your agent with institutional authority attached. That channel is the product. It is also the risk. So we attacked it.",
  rounds: [
    {
      round: "Round 1 — before the fixes",
      behavior: "Served the poison as fact, unflagged.",
      outcome: "silent poisoning",
      tone: "bad" as const,
    },
    {
      round: "Round 2 — after the governance floor",
      behavior:
        "We planted a harder poison: a canonical decoy carrying full, recent provenance — better provenance than the truth it contradicted. The agent traced both, believed the poison, and used its number.",
      outcome: "the poison won",
      tone: "bad" as const,
    },
    {
      round: "Round 3 — after contested-serving",
      behavior:
        "The identical poison now produces a refusal: the agent reports the claim is contested, declines to adjudicate, and escalates to a human.",
      outcome: "refused and escalated",
      tone: "good" as const,
    },
  ],
  quote:
    "The exact poison that walked the agent to the wrong number now produces a refuse-and-escalate.",
  moral:
    "We publish the round where we lost, because a vendor who only shows you round three is showing you a demo, not a system.",
};

// ─────────────────────────────────────────────────────────────────────────────
// 8. Where we lose — the section no competitor has
// ─────────────────────────────────────────────────────────────────────────────

export const WEAKNESSES = [
  {
    title: "Our extraction recall is 0.42.",
    body:
      "Precision is 0.81 — what we capture is right. But we currently drop more than half of what a session teaches. In a live flywheel run the extractor caught the sharpest pitfall and silently dropped a second learning sitting right beside it. This is the number we are fixing first, and it is on this page because you would have found it in our repo anyway.",
    metric: "recall 0.42 / precision 0.81",
  },
  {
    title: "On single-team work, we are dead weight.",
    body:
      "Our own controlled trial says so: where the answer is already in your CLAUDE.md, Brainiac returns the same answer for 2.3× the tokens. Turn it on for the teams and tasks that cross a boundary. Leave it off where a text file already wins.",
    metric: "2.3× cost, zero benefit",
  },
  {
    title: "The value depends on the agent choosing to ask.",
    body:
      "Our measured wins are real, but they hinge on the agent calling memory_context. You cannot retrieve the answer to a question you don't know to ask. Proactive session-start briefing — pushing what changed in your area — is the next thing we build, and until it ships this is a latent win, not a realized one.",
    metric: "the invocation gap",
  },
  {
    title: "Anthropic has already shipped much of the substrate.",
    body:
      "Memory Stores gives you workspace-scoped memory, an immutable version chain, session attribution and compliance redaction. What it does not give you is the review gate, fact-granularity, or permission-aware retrieval — and their docs say the API exists \"for building review workflows,\" leaving the gate to you. That is a product gap, not a research gap. We think the gate is the product. They may come to agree.",
    metric: "the honest threat",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 9. How it actually works
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The knowledge base layer — a named capability, honestly staged.
 *
 * KB0 (memory lifecycle facet + detail_md, and the Knowledge Health page) is
 * merged; KB1, the document layer itself, is under construction; publishing is
 * roadmap. This teaser must never imply otherwise — see docs/KB-PLAN.md and the
 * status stamps on /kb, which are pinned to the plan's status log by a test.
 */
export const KB_TEASER = {
  status: "in progress · v0.5",
  headline: "And then the pages compile themselves.",
  body:
    "A wiki rots because the page is where the knowledge lives. Brainiac inverts that: canonical memories are the only source of truth and a page is a compiled projection over them, dirty-marked and regenerated the moment a memory it cites is superseded. A contradiction resolved in the review queue propagates to every page that cited the losing claim. Truth flows one way — a human edit to a page re-enters through extraction and faces the same review gate as any agent proposal.",
  points: [
    { label: "shipped", body: "The memory lifecycle facet (shipped / in-flight / proposed) and structure-preserving payloads that pages compose from — plus the Knowledge Health score that will gate publishing." },
    { label: "in progress", body: "The document layer: composed and pinned sections, [m:uuid] citations, the dirty-marking compose worker, and an auto-publish policy." },
    { label: "roadmap", body: "One-way Confluence publishing over a PAT — org-visible memories only, paused automatically when the corpus health degrades." },
  ],
  href: "/kb",
};

export const PIPELINE_STAGES = [
  { n: "01", name: "Capture", body: "A session ends. The transcript enters the queue — not a doc someone remembered to write." },
  { n: "02", name: "Extract", body: "Your model, your keys, your infrastructure, distills facts, decisions, pitfalls and howtos. Each one lands as raw, with provenance attached." },
  { n: "03", name: "Resolve", body: "“payments API” and “payment-service” become one canonical entity — a soft, reversible link, never data surgery." },
  { n: "04", name: "Contradict", body: "The new claim is checked against what the org already believes. A conflict opens a contradiction, with a suggested resolution — it does not silently overwrite." },
  { n: "05", name: "Promote", body: "Policy decides: auto-promote, or route to a maintainer. Nothing reaches canonical without a named human signing for it." },
  { n: "06", name: "Serve", body: "Agents retrieve over MCP. Postgres row-level security runs inside the vector scan — an agent cannot see what its operator cannot see." },
];
