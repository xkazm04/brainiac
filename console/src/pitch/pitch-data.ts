/*
 * Pitch content.
 *
 * THE RULE THIS FILE NOW FOLLOWS (and previously broke):
 *
 * The reader is an engineer, not a buyer. Persuade with mechanism and with
 * problems they have personally hit — never with a metric that sounds precise
 * and isn't.
 *
 * Deliberately removed in the honesty pass:
 *   - the "benchmark theater" section. We attacked the market for staging
 *     accuracy numbers and then staged our own. Both halves are gone.
 *   - the borrowed industry stat tiles (56% / 42% / 61% / −7.2%). Secondhand
 *     survey percentages, several with laundered citation chains, none of which
 *     say anything about whether OUR design is right.
 *   - our own retrieval/extraction score tiles (NDCG@10, F1). Real numbers, but
 *     measured on a synthetic fixture org we wrote ourselves. Quoting them as
 *     headline evidence is the same trick we were criticizing.
 *
 * What is left is checkable: the mechanisms (which you can read in the schema
 * and the SQL), the competitors' own documentation (linked), a small controlled
 * experiment reported with its n, and the places this design is the wrong tool.
 */

export interface Cite {
  label: string;
  href: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. The problem — three structural failures, not statistics
// ─────────────────────────────────────────────────────────────────────────────

export interface Problem {
  key: string;
  title: string;
  /** ONE line, set under the figure — naming the scene the drawing depicts.
      The figure carries the story; this is its caption, not a paragraph. */
  scene: string;
  /** WHY no amount of discipline fixes it. The load-bearing sentence, and the
      only prose the section keeps. */
  structural: string;
}

export const PROBLEMS: Problem[] = [
  {
    key: "boundary",
    title: "The answer is in another team's repo.",
    scene: "Payments moves the PSP timeout to 30s. The web repo's 15s abort quietly becomes a double charge.",
    structural:
      "No file that ships inside one repo can carry a decision made in another. That is not a discipline problem you can fix with better docs — it is a boundary, and a per-repo file is on the wrong side of it.",
  },
  {
    key: "retraction",
    title: "The decision was reversed. The document did not notice.",
    scene: "A cap is raised, then reverted a sprint later. The runbook still says 30s — confidently.",
    structural:
      "A document has no write-back path from reality. Code changes because a PR merges; a doc changes only if a human remembers, is rewarded, and has time. Those two rates are not close.",
  },
  {
    key: "attestation",
    title: "A model wrote a belief into your record. Nobody signed it.",
    scene: "Someone speculated in a session. An agent stored it. The next developer received it as policy.",
    structural:
      "Every memory product in this market lets a model assert a fact into shared state with no asserting principal, no evidence, and no expiry. You cannot ask who said this, or whether it still holds — the fields do not exist.",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 2. The bifurcation — argued from the vendors' own docs, not from scores
// ─────────────────────────────────────────────────────────────────────────────

export interface Player {
  name: string;
  full: string;
  x: number;
  y: number;
  camp: "search" | "memory" | "neither" | "us";
  side: "left" | "right";
  why: string;
}

export const PLAYERS: Player[] = [
  { name: "Glean", full: "Glean", x: 9.2, y: 1.7, camp: "search", side: "left", why: "Source-mirrored ACLs, enforced at retrieval. Its MCP verbs are search / read_document. There is no capture verb and no adjudication verb anywhere in the product." },
  { name: "ChatGPT", full: "ChatGPT company knowledge", x: 8.6, y: 0.6, camp: "search", side: "left", why: "\"ChatGPT can only see the content a user is already authorized to view.\" Persists nothing. It is search, not memory." },
  { name: "Gemini", full: "Gemini for Workspace", x: 9.6, y: 1.15, camp: "search", side: "left", why: "\"If you don't have permission to see a file, the AI can't see it or use it either.\" Stateless. Remembers nothing." },
  { name: "Confluence / Rovo", full: "Confluence + Atlassian Rovo", x: 7.0, y: 1.4, camp: "search", side: "left", why: "Space permissions and page restrictions. Freshness is a third-party marketplace app, which exists because the platform does not do it." },

  { name: "Obsidian", full: "Obsidian", x: 0.5, y: 0.8, camp: "neither", side: "right", why: "In a shared vault every collaborator inherits the owner's permissions. No review workflow, no provenance, no staleness detection. The graph view is a rendering of the link table: untyped edges, no evidence, no time." },

  { name: "Mem0", full: "Mem0", x: 1.2, y: 1.8, camp: "memory", side: "right", why: "org_id is a filter parameter, not an authorization boundary — their own docs say applications \"still need proper authentication and access-control boundaries around those IDs.\" The default pipeline is now ADD-only: contradictory facts coexist and the ranker decides." },
  { name: "Supermemory", full: "Supermemory", x: 5.2, y: 2.0, camp: "memory", side: "right", why: "Genuine namespace isolation at the data layer, though it stops at the namespace and never reaches the individual fact. No human review, no provenance, no org-level memory." },
  { name: "Zep / Graphiti", full: "Zep / Graphiti", x: 2.2, y: 5.2, camp: "memory", side: "right", why: "The best data model in the market: bi-temporal, episode lineage, real contradiction supersession. But conflicts resolve by silent last-writer-wins, and nothing gates who may read a group graph." },
  { name: "Cognee", full: "Cognee", x: 7.0, y: 3.2, camp: "memory", side: "left", why: "The only independent with DB-layer tenant/role/user grants resolved before the query runs. But granularity is the dataset, not the fact, and the docs contain nothing on review, provenance, or contradiction." },
  { name: "Copilot Memory", full: "GitHub Copilot Memory", x: 5.4, y: 5.0, camp: "memory", side: "right", why: "Repo-scoped and write-access-gated; facts carry code citations and re-verify against the code on session start. But review is post-hoc delete, and no audit event fires when a memory is created or used." },
  { name: "Memory Stores", full: "Anthropic Memory Stores", x: 3.0, y: 6.0, camp: "memory", side: "right", why: "Immutable version chain, session attribution, filesystem-enforced read_only, compliance-grade redaction. But no permission-aware retrieval; isolation is by sharding into separate stores. And the review gate is explicitly left for you to build." },

  { name: "Brainiac", full: "Brainiac", x: 9.4, y: 9.3, camp: "us", side: "left", why: "Row-level security runs inside the vector scan itself, and nothing becomes canonical without a named human. The two halves of this chart, joined." },
];

export const BIFURCATION_LINE =
  "One half of this market respects permissions and believes nothing. The other half believes things and respects nothing. Nobody has joined them.";

// ─────────────────────────────────────────────────────────────────────────────
// 3. The mechanisms — the actual argument. Each one is checkable in the repo.
// ─────────────────────────────────────────────────────────────────────────────

export interface Mechanism {
  key: string;
  title: string;
  /** The one-sentence claim. */
  claim: string;
  /** How it works, concretely enough to disagree with. */
  how: string;
  /** What everyone else does instead — the reason this matters. */
  instead: string;
  /** A real artifact: schema, policy, or status ladder. */
  artifact?: string;
  band: "gamma" | "alpha" | "theta" | "beta" | "delta";
}

export const MECHANISMS: Mechanism[] = [
  {
    key: "rls",
    title: "The scan never sees a row you may not read.",
    claim:
      "An agent cannot retrieve what its operator cannot read, because the database refuses, not because the code remembered to filter.",
    how:
      "Visibility is a row-level security policy on the memories table. The retrieval path opens a transaction as the calling principal, so the pgvector similarity scan is evaluated against rows the policy already excluded. There is no code path that returns a memory the caller may not see, because the planner never sees one either.",
    instead:
      "Every independent scopes memory with a caller-supplied user_id or org_id. It is a filter you pass, and passing the wrong one returns someone else's knowledge. Mem0's own docs tell you to build the access-control boundary yourself.",
    artifact: `CREATE POLICY memories_read ON memories FOR SELECT USING (
  org_id = current_setting('app.org_id')::uuid
  AND ( visibility = 'org'
     OR (visibility = 'team' AND team_id IN (
           SELECT team_id FROM team_members
            WHERE user_id = current_setting('app.user_id')::uuid))
     OR (visibility = 'private' AND owner_user_id = current_setting('app.user_id')::uuid))
);`,
    band: "alpha",
  },
  {
    key: "provenance",
    title: "Every claim names who asserted it.",
    claim:
      "Any fact in the graph traces back to who or what produced it, from which session, with which model, in which pipeline run.",
    how:
      "Memories, entities and edges all point at a provenance row. It records the actor kind (human, agent, or pipeline), the actor, the model reference when a model produced it, the originating source, and the run id — so a claim can be replayed to its origin rather than merely cited.",
    instead:
      "Ask ChatGPT why it believes something about your company and there is no answer to give: its 2026 memory rewrite replaced the enumerable, user-editable list with opaque background synthesis. The recall scores went up. The accountability went to zero.",
    artifact: `provenance (
  actor_kind   text,   -- human | agent | pipeline
  actor_id     text,
  model_ref    text,   -- "qwen:qwen-max" when a model wrote it
  source_id    uuid,   -- the session it came from
  pipeline_run_id uuid -- replay the exact run
)`,
    band: "delta",
  },
  {
    key: "contradiction",
    title: "When two claims collide, a human adjudicates.",
    claim:
      "A contested claim is never quietly resolved, and it is never served to an agent as though it were settled.",
    how:
      "A new claim is checked against what the org already believes. A conflict opens a contradiction row holding both memories and a suggested resolution, and a contested claim is not served as fact: the agent is told it is contested and escalates instead of adjudicating. Resolution supersedes the loser. It never deletes it.",
    instead:
      "Zep, the best data model in this market, resolves conflicts by silent last-writer-wins. Mem0 removed the ability entirely in April 2026: the pipeline is ADD-only, so both claims live in the store and whichever one the ranker happens to surface becomes the truth of that session.",
    band: "theta",
  },
  {
    key: "temporal",
    title: "Ask what the organisation believed in March.",
    claim:
      "You get March's answer, from the database. No excavating git history and Slack scrollback to work out what was true at the time.",
    how:
      "Every memory carries a validity window and a forward pointer to whatever superseded it. Retrieval excludes deprecated claims by default but never destroys them, so an as-of read reconstructs the org's belief at a date. That is also what makes a reversal safe: the old claim stops being served the moment the new one lands.",
    instead:
      "A markdown file has one tense: now. When a decision is reversed, the stale line just sits there being confidently wrong, and nothing in the system knows it died.",
    artifact: `memories (
  valid_from     timestamptz,
  valid_to       timestamptz,  -- NULL = still true
  superseded_by  uuid          -- what replaced it
)`,
    band: "beta",
  },
  {
    key: "gate",
    title: "An agent proposes. A named human promotes.",
    claim:
      "Nothing becomes canonical org knowledge on its own.",
    how:
      "A memory moves raw → candidate → canonical. Policy decides whether a hop is automatic or routed to a maintainer, and the promotion row records which rule fired and who reviewed it. Reaching canonical always requires a human. The audit trail is a table you can query.",
    instead:
      "This is the row that is empty for every other product on this page. Cursor built implicit memory and deleted it rather than govern it. GitHub Copilot lets owners review only after the fact. It fires no audit event when a memory is created or used at all.",
    artifact: `raw ──▶ candidate ──▶ canonical
     policy        a human signs
     (auto)        (always)`,
    band: "gamma",
  },
  {
    key: "byom",
    title: "Your model. Your keys. Your box.",
    claim: "The transcripts never leave your infrastructure.",
    how:
      "Extraction, entity resolution and contradiction detection all run through a bring-your-own-model gateway against endpoints you control. One Rust binary; Postgres is the only mandatory dependency; the whole thing fits on a 1 vCPU box. The only outbound calls are to your own model endpoints.",
    instead:
      "Most of this market is a hosted API you ship your sessions to. Your engineers' raw problem-solving — the most sensitive text your company produces — becomes someone else's training corpus or, at best, someone else's breach surface.",
    band: "gamma",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 4. The retreat — primary-source facts, not survey percentages
// ─────────────────────────────────────────────────────────────────────────────

export const RETREAT = [
  {
    who: "Cursor",
    what: "deleted Memories",
    when: "v2.1.17",
    detail:
      "Shipped in 1.0 as beta, never left beta, removed with no changelog entry. What they bet on instead: Team Rules — admin-authored, non-disableable, written by a person. The most aggressive AI IDE on the market built implicit memory, looked at it, and replaced it with rules a human wrote.",
    cite: { label: "Cursor forum — staff confirmation", href: "https://forum.cursor.com/t/memories-not-showing/143820" },
  },
  {
    who: "Windsurf",
    what: "retired Cascade",
    when: "1 Jul 2026",
    detail:
      "Its own documentation disclaimed the feature before it died: “For knowledge you want Cascade to reliably reuse, write it as a Rule — rather than relying on auto-generated Memories.” The vendor telling you not to trust the vendor's memory.",
    cite: { label: "Windsurf is now Devin Desktop", href: "https://devin.ai/" },
  },
  {
    who: "OpenAI",
    what: "made memory unauditable",
    when: "Jun 2026",
    detail:
      "The saved-memories list — enumerable, user-editable, inspectable — was replaced by opaque background synthesis. Recall improved. There is now no per-fact provenance and no way to ask why it believes something about you.",
    cite: { label: "OpenAI memory documentation", href: "https://help.openai.com/en/articles/8590148-memory-faq" },
  },
  {
    who: "Mem0",
    what: "removed contradiction handling",
    when: "Apr 2026",
    detail:
      "Their engineering blog: the new algorithm “collapses a two-pass extraction process into one, eliminating UPDATE and DELETE operations.” The docs now describe the pipeline as additive: new memories are added without overwriting existing ones. Contradictory facts coexist, and nothing in the system knows which one is dead.",
    cite: {
      label: "Mem0 — the token-efficient memory algorithm",
      href: "https://mem0.ai/blog/mem0-the-token-efficient-memory-algorithm",
    },
  },
];

export const RETREAT_LEDE =
  "These are not companies that failed to notice the problem. They are companies that looked straight at it and walked away: governance is friction, and friction is the opposite of what a zero-config memory API is selling.";

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
    capability: "Permission enforced by the database",
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
    capability: "Contradiction adjudicated, not overwritten",
    detail: "Two sources disagree right now — surface it to a reviewer instead of silently picking one.",
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

export const EMPTY_ROW = "Human review before canonical";

// ─────────────────────────────────────────────────────────────────────────────
// 6. What we actually tested — reported with its n, and with the control
// ─────────────────────────────────────────────────────────────────────────────

export const TRIAL = {
  design:
    "Three arms on the same tasks: a cold agent, Claude's native memory built the way a competent senior would build it, and Brainiac on top of that baseline. The baseline was written first and written generously, before any task ran, so it could not be tuned to lose. Two tasks were controls where we predicted Brainiac would add nothing.",
  caveat:
    "Two samples per arm. That is enough to rule out a fluke and nowhere near enough to be a statistic: the direction is unambiguous, the magnitude is indicative. We are telling you the n because the alternative is the thing we object to in everyone else's marketing.",
  rows: [
    {
      task: "A data engineer needs a dedup window that must match the payments team's refund retry cap.",
      gap: "the answer is in another team's repo",
      cold: "Refused — asked to be pointed at the payments repo.",
      baseline: "Refused — “cannot be determined from this repository.”",
      brainiac: "Answered, and cited the memory and the date the decision was made.",
      verdict: "win" as const,
      reading:
        "The baseline did not lose slowly. It could not play. It correctly declined rather than guess, which is the right behaviour, and also exactly the ceiling of a per-repo file.",
    },
    {
      task: "A web developer must judge a 15s client abort after payments quietly raised the PSP timeout to 30s.",
      gap: "the decision was reversed and the file did not notice",
      cold: "Guessed “too low” — and explicitly marked it unverified.",
      baseline: "Declared the 15s abort fine. Twice. That ships the double-charge bug.",
      brainiac: "Flagged it, citing the timeout change and the double-charge pitfall.",
      verdict: "win" as const,
      reading:
        "Here the baseline was not slow. It was confidently wrong. A guess that happens to lean right is not the same as knowing, and the cold agent said so itself.",
    },
    {
      task: "A payments developer asks something their own CLAUDE.md already answers.",
      gap: "none — this is the control",
      cold: "—",
      baseline: "Correct, in two turns, for free.",
      brainiac: "The same answer, for roughly twice the tokens and twice the turns.",
      verdict: "loss" as const,
      reading:
        "Identical answer, more than double the cost. This is the result that proves the experiment wasn't rigged: where the design should add nothing, it adds nothing but overhead.",
    },
  ],
};

// ─────────────────────────────────────────────────────────────────────────────
// 7. The adversarial probe — a mechanism story, not a score
// ─────────────────────────────────────────────────────────────────────────────

export const POISON = {
  premise:
    "A file in your repo can only be wrong if someone on your team wrote something wrong. A shared store is a new channel: someone else's mistaken belief — or a machine's hallucination — can now reach your agent wearing institutional authority. That channel is the product, so it is also the risk. We attacked it.",
  rounds: [
    {
      round: "First attempt",
      behavior: "We planted a false canonical claim. It was served as fact, unflagged, and the agent used it.",
      outcome: "silent poisoning",
      tone: "bad" as const,
    },
    {
      round: "After the governance floor",
      behavior:
        "So we planted a harder one: a false claim carrying full, recent provenance — better provenance than the truth it contradicted. The agent traced both, believed the poison, and used its number.",
      outcome: "the poison won",
      tone: "bad" as const,
    },
    {
      round: "After contested-serving",
      behavior:
        "A claim inside an unresolved contradiction is no longer served as fact. The identical poison now produces a refusal: the agent reports the claim is contested, declines to adjudicate, and escalates to a human.",
      outcome: "refused and escalated",
      tone: "good" as const,
    },
  ],
  moral:
    "The second round is the interesting one, and it is the reason contested-serving exists. A vendor who only shows you the third round is showing you a demo, not a system.",
};

// ─────────────────────────────────────────────────────────────────────────────
// 8. Where this is the wrong tool
// ─────────────────────────────────────────────────────────────────────────────

export const WEAKNESSES = [
  {
    title: "On single-team work, this is dead weight.",
    body:
      "Where the answer already lives in your own repo's file, Brainiac returns the same answer for roughly double the cost. Turn it on where knowledge crosses a boundary. Leave it off where a text file already wins. Our own control task says so.",
    metric: "the redundancy tax",
  },
  {
    title: "Extraction still drops things.",
    body:
      "What we capture is usually right; how much of a session we capture is not yet good enough. In a live run the extractor caught the sharpest pitfall in a note and silently dropped a second learning sitting beside it. A store that quietly loses a fraction of every session erodes trust exactly the way it should. This is the thing we are fixing first.",
    metric: "recall, not precision",
  },
  {
    title: "It only helps if the agent asks.",
    body:
      "Every win we have measured depends on the agent choosing to call the memory tool. You cannot retrieve the answer to a question you don't know to ask. Until a session-start briefing pushes what changed in your area, the benefit stays latent.",
    metric: "the invocation gap",
  },
  {
    title: "Anthropic has already built much of the substrate.",
    body:
      "Memory Stores gives you workspace-scoped memory, an immutable version chain, session attribution and compliance redaction. What it does not give you is the review gate, fact-granularity, or permission-aware retrieval. Their docs say the API exists “for building review workflows,” leaving the gate to you. Closing that gap is product work, and they have every reason to do it. We think the gate is the product. They may come to agree.",
    metric: "the honest threat",
  },
];

// ─────────────────────────────────────────────────────────────────────────────
// 9. The wiki that cannot rot (KB teaser) — unchanged
// ─────────────────────────────────────────────────────────────────────────────

export const KB_TEASER = {
  status: "shipped · publishing built, not switched on",
  headline: "And then the pages compile themselves.",
  body:
    "A wiki rots because the page is where the knowledge lives. Brainiac inverts that: canonical memories are the only source of truth, and a page is a compiled projection over them, regenerated the moment a memory it cites is superseded. Resolve a contradiction in the review queue and it propagates to every page that cited the losing claim. A human edit to a page re-enters through extraction and faces the same gate as any agent proposal.",
  points: [
    { label: "shipped", body: "The memory lifecycle facet, the structure-preserving payloads pages compose from, and the health score that gates publishing." },
    { label: "shipped", body: "The document layer: pages, sections, revisions and the dependency index; the compose worker; per-claim citations; the publish policy." },
    { label: "built, not switched on", body: "One-way Confluence publishing: org-visible memories only, paused automatically when corpus health degrades. Merged and tested, and off." },
  ],
  href: "/kb",
};

// ─────────────────────────────────────────────────────────────────────────────
// 10. The pipeline
// ─────────────────────────────────────────────────────────────────────────────

export const PIPELINE_STAGES = [
  { n: "01", name: "Capture", body: "A session ends and its transcript enters the queue. Nobody has to remember to write anything down." },
  { n: "02", name: "Extract", body: "Your model, your keys, distills facts, decisions, pitfalls and howtos. Each lands as raw, with provenance attached." },
  { n: "03", name: "Resolve", body: "“payments API” and “payment-service” become one canonical entity — a soft, reversible link, never data surgery." },
  { n: "04", name: "Contradict", body: "The claim is checked against what the org believes. A conflict opens a contradiction; it does not overwrite." },
  { n: "05", name: "Promote", body: "Policy decides: auto-promote, or route to a maintainer. Canonical always requires a named human." },
  { n: "06", name: "Serve", body: "Agents retrieve over MCP, inside the caller's row-level security. They cannot see what their operator cannot see." },
];

// ─────────────────────────────────────────────────────────────────────────────
// Navigation — the sections, in order. Drives the sticky nav and the spine.
// ─────────────────────────────────────────────────────────────────────────────

export interface NavSection {
  id: string;
  /** The eyebrow inside the section — lowercase, in the page's voice. */
  label: string;
  /** The display name in the nav rail. Title-case, no article: a nav is a set
   *  of destinations, and "the problem" reads as prose rather than a label. */
  nav: string;
}

export const SECTIONS: NavSection[] = [
  { id: "problem", label: "the problem", nav: "Problem" },
  { id: "quadrant", label: "the gap", nav: "Gap" },
  { id: "mechanisms", label: "how it works", nav: "Mechanisms" },
  { id: "retreat", label: "the retreat", nav: "Retreat" },
  { id: "matrix", label: "the matrix", nav: "Matrix" },
  { id: "trial", label: "what we tested", nav: "Evidence" },
  { id: "limits", label: "where it loses", nav: "Limits" },
];
