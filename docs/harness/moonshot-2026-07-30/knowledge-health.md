# Moonshot scan — knowledge-health

> Total: 2
> Files read: 18

**Load-bearing question, answered: confirmed.** Staleness in this product is pure calendar
arithmetic. `compute_health_core` counts a memory stale with
`status = 'deprecated' OR (valid_to IS NOT NULL AND valid_to < now())`
(`crates/brainiac-server/src/console.rs:2351`), and `valid_to` is seeded at extraction from a
per-kind constant — `MemoryKind::default_ttl_days` → fact 365 / decision 540 / pattern 540 /
pitfall 365 / howto 180 (`crates/brainiac-core/src/types.rs:114`, applied at
`crates/brainiac-pipeline/src/extract.rs:328`). `currency_pillar(total, stale)`
(`crates/brainiac-core/src/health.rs:66`) is therefore a clock reading, not a truth reading. And
`memory_reverify` (`console.rs:1381`) resets that clock with **zero evidence**: it takes an
optional `days`, calls `extend_validity`, closes the open claims, and records nothing about *what
was checked, against what, by what method*. A maintainer who reads the memory carefully and a
maintainer who clicks the button while scrolling produce byte-identical state.

This is not a cosmetic gap, because the calendar number is already an **actuator**:
`publish_block_reason(currency, governance)` (`health.rs:145`) pauses external wiki sync on
`currency < 70`. Today the org's Confluence sync is gated on a countdown timer that any maintainer
can reset without looking at anything.

---

## 1. Proof-of-truth currency: a verification ledger and an autonomous falsification prover

- **Severity**: Critical
- **Category**: verifiable-truth
- **File**: `crates/brainiac-core/src/health.rs:66` (`currency_pillar`) · anchors:
  `crates/brainiac-server/src/console.rs:1381` (`memory_reverify`), `console.rs:2348-2364` (the
  stale count), `console.rs:1146-1190` (the feedback `reverified` path),
  `crates/brainiac-core/src/types.rs:114` (`default_ttl_days`),
  `crates/brainiac-server/src/sweeps.rs:266-326` (`run_one` dispatch),
  `crates/brainiac-core/src/health.rs:145` (`publish_block_reason`)

- **Scenario**: A VP opens Knowledge Health and the currency pillar no longer says "88% of rows
  have not timed out yet". It says **"91% of what we served last month was *proven* true within
  the last 30 days — here is the proof for any row you click."** Each memory carries a
  verification record with a *method* and *evidence*: a maintainer attestation naming what they
  read; a re-read of the originating source showing the claim still holds; a passing check;
  survival of a contradiction adjudication; or a prover verdict citing the artifact it consulted.
  Overnight, a `verify` sweep works down a consequence-ordered list of memories, sends each to an
  LLM prover whose *job is to falsify it*, and writes back one of three outcomes: **proven** (with
  citation, window extended), **falsified** (with citation, auto-deprecated through
  `apply_deprecation` so the pages recompose and the audit row lands), or **inconclusive** (the
  only case that reaches a human — and it arrives with the prover's notes, so the human starts
  from a draft instead of from nothing). A 400-day-old fact that the prover re-proved last night
  counts as current. A 3-day-old fact that a merge falsified this morning goes red *today*, not in
  177 days. Publishing gates on proof coverage, so the wiki sync can no longer be unblocked by
  clicking a button — only by producing evidence.

- **Root cause**: The product has an *epistemics* problem disguised as a *scheduling* problem.
  Every governance act in Brainiac is evidenced except the one that matters most for trust:
  promotion has `promotions` rows, contradiction resolution routes through
  `governance::apply_supersession`, disputes carry `memory_feedback` verdicts and resolution
  notes — but re-verification carries nothing. `ReverifyResponse` returns
  `{memory_id, reverified: true, valid_to, days, claims_closed}`; there is no `method`, no
  `evidence`, no `checked_against`. Because the act is unevidenced, the only thing the system can
  measure is elapsed time, so `currency_pillar` had no choice but to be a clock. The ceiling this
  caps: Brainiac's whole pitch is "retrieval quality an agent can trust, proven by an eval
  harness" — and the one claim it cannot prove is *that what it serves is still true*. `meridian`
  proves retrieval finds the right row; nothing proves the row is right.

- **Impact**: This is the difference between a freshness dashboard (a commodity — every wiki has
  a "last updated" column) and **the first memory system that ships a warrant with every belief**.
  Concretely: (a) falsification latency collapses from a TTL half-life measured in *hundreds of
  days* to a sweep cadence measured in *hours*, on exactly the memories that matter; (b) the
  publish breaker becomes defensible — "we pause your Confluence sync when we can no longer prove
  what we'd push", which is a sentence a security-conscious buyer can take to a review board,
  where "when a timer expires" is not; (c) it creates a claim no competitor can copy cheaply,
  because it needs the governance substrate this repo already has (provenance, supersession,
  recomposition, audit); (d) it turns maintainer effort superlinear — the prover clears the
  mechanical majority, humans spend their attention only on the genuinely ambiguous, which is the
  only way a corpus of 100k memories is maintainable by a team of five. Note the honest ceiling
  on today's design: with `howto` at 180 days and `decision` at 540, an org's currency pillar is
  ~structurally pinned near 100 for the first year and a half of its life regardless of how much
  of the corpus has quietly gone wrong. The instrument literally cannot fail early, which is the
  worst property a health instrument can have.

- **Fix sketch**:
  1. **Slice one — make the act evidenced, and split the number in two (small, provable, no
     prover yet).** New migration adding `memory_verifications` (`id, org_id, memory_id, method
     CHECK (method IN ('human_attest','source_recheck','check_evidence','contradiction_survived',
     'prover')), verdict CHECK (verdict IN ('proven','falsified','inconclusive')), evidence text,
     evidence_ref, verifier_user_id, confidence real, verified_at`), append-only at the grant and
     org-RLS'd — copy the policy/grant shape of `document_reads` in `migrations/0025` verbatim,
     including the "no UPDATE, no DELETE" grant stance. Extend `ReverifyBody` with required
     `method` + `evidence`, write the ledger row inside the existing transaction in
     `memory_reverify` and in the `"reverified"` arm of the feedback handler (`console.rs:1146`).
     Then add `verified` to `HealthCore` (`console.rs:2312`) — `count(*) FILTER (WHERE EXISTS
     (SELECT 1 FROM memory_verifications v WHERE v.memory_id = m.id AND v.verdict='proven' AND
     v.verified_at > now() - interval '30 days'))` — and add
     `proof_coverage_pillar(total, verified)` beside `currency_pillar` in `core/health.rs`, which
     is IO-free and unit-testable exactly like the existing pillar tests. Ship it as a *second*
     number first, not a redefinition: `knowledge_health_snapshots` has orgs' trend lines keyed on
     the current formula, and `core/health.rs:24-38` already articulates the house rule against
     silently redefining a tracked composite.
  2. **Slice two — the prover.** A new `"verify"` arm in `sweeps.rs::run_one` beside
     `"divergence"` (which already shows the pattern: pull `ProviderRouter::from_env`, run a
     multi-minute LLM pass on the admin pool). Select candidates by consequence, not date (see
     finding 2 for the ranking substrate; until it exists, `visibility='org'` + nearest `valid_to`
     is an honest interim). Prompt shape: give the prover the memory content, its `provenance`
     row, and its `memory_entities` neighbourhood, and demand a citation for any verdict —
     borrow the malformed-resilience posture already proven in `pipeline/src/extract.rs`. On
     `falsified`, route through `governance::apply_deprecation` (already deprecates + closes
     `valid_to` + recomposes dependent pages + writes the audit row — `console.rs:1207`), never
     inline SQL. On `proven`, `memories::extend_validity`. On `inconclusive`, file a review item.
  3. **Slice three — flip the actuator.** `publish_block_reason` gains a proof-coverage floor
     alongside `PUBLISH_MIN_CURRENCY`, and the console renders proof coverage as the currency
     pillar's real answer to its own stated question, "Is what we serve still true — or expired
     and never re-verified?" (`KnowledgeHealth.tsx:62`) — a question the current number does not
     actually answer. MCP `memory_search` gains `verified_at`/`verification_method` next to the
     existing `valid_to` and `feedback` trust block (`mcp.rs:880-900`), so an agent can weight a
     *proven* memory above a merely *unexpired* one. Add a `meridian` prover profile — the
     harness already has extraction and contradiction profiles; a falsification-precision axis is
     the same shape, and without it an auto-deprecating prover is unshippable.

  *Deliberately not in scope:* the sibling context owns memory→code anchors. This design consumes
  anchors as the strongest `source_recheck` evidence when they exist, but must not depend on them
  — provenance re-read, check evidence, and human attestation carry it alone.

---

## 2. The knowledge-debt ledger: consequence-ranked maintenance with a forecast payoff, dispatched to agents

- **Severity**: High
- **Category**: consequence-ranked-remediation
- **File**: `crates/brainiac-server/src/console.rs:2519-2783` (the `attention` list) · anchors:
  `crates/brainiac-core/src/health.rs:59-110` (the pure pillar + composite functions),
  `migrations/0014_knowledge_health_snapshots.sql` (aggregates only),
  `migrations/0025_document_reads.sql` (`was_dirty` — the pattern to copy),
  `migrations/0028_library_substrate.sql:149` (`library_usage_events`),
  `crates/brainiac-server/src/mcp.rs:433-560` (tool registry),
  `console/src/health/KnowledgeHealth.tsx:226-249` (renders it as read-only prose)

- **Scenario**: "Needs your attention" stops being a paragraph and becomes **the org's knowledge
  debt ledger**. Every item is a durable, addressable object with an owner, a permalink, and a
  one-click action — and it is ranked by **consequence**, not by push order: how many agent
  sessions and page reads consumed this belief in the last 30 days × how wide its blast radius is
  (org / cross-team / cross-project) × how bad the failure class is. A pitfall that 40 agent
  sessions pulled last week outranks a 400-day-expired private note, and the list says so. Above
  it sits the number that changes behaviour: **"clearing these six items moves you 61 → 78."**
  After they're cleared, the ledger shows whether it actually did. And the whole queue is exposed
  over MCP, so a coding agent that just opened `refund-worker.rs` is handed *the two maintenance
  items about the code it is already looking at* — "this pitfall claims the retry cap is 30s;
  confirm or correct while you're here" — and files the verification as a side effect of work
  someone was already doing.

- **Root cause**: Three compounding caps. (a) **No identity.** `attention` is an ephemeral
  `Vec<KhAttention>` of `{severity, kind, headline, detail}` strings rebuilt on every GET. No id,
  no memory/document id, no owner, no state. It cannot be linked to, assigned, snoozed, closed, or
  counted — so the console can only *print* it (`KnowledgeHealth.tsx:244`), and the surface's own
  stated centre of gravity is a dead end. (b) **No consumption signal for memories.** The repo
  already learned this lesson twice and only applied it to secondary artifacts: `document_reads`
  records page reads *including `was_dirty`*, with the migration comment naming it "the signal
  that RANKS rot: a dirty page nobody reads is a chore, a dirty page being read is misleading
  someone right now" — and `library_usage_events` does the same for standards and skills. The
  **primary artifact, memories served through MCP, has no such table at all.** So harm can be
  ranked for pages and rules and not for the thing the product is actually for. Ranking today is
  literally source-code order, capped at `LIMIT 5` per category (`console.rs:2529`). (c) **No
  counterfactual.** `knowledge_health_snapshots` stores four pillars and three counters — enough
  to draw a line, never enough to answer "what would fixing *this* be worth?", so the score
  advises nobody on what to do next.

- **Impact**: Turns a weekly report card into a **work system with a measured ROI**, which is the
  difference between a metric a VP glances at and a metric a VP runs a ritual around. (a) The
  forecast is the unlock: because the pillar math in `core/health.rs` is *already pure and
  IO-free* — a deliberate design choice made so the publish breaker couldn't drift from the
  dashboard — you can re-run `composite_score` over a hypothetical post-fix corpus state for
  essentially free, and attach a projected point-gain to every item. No new math, no model, no
  migration: just calling functions that exist with different arguments. That single move converts
  "our score is 61" into "here is the ordered list of six actions and what each is worth," and
  makes the trend line a *forecastable target* rather than weather. (b) Consequence ranking fixes
  a real inversion: today a maintainer's scarce attention is spent in the order the Rust file
  happens to push items, which means the org's highest-blast-radius rot is routinely below a
  never-read page's info row. (c) The MCP dispatch is the 10x force multiplier and the network
  effect: maintenance stops being a chore that competes with feature work and becomes ambient
  work distributed to whoever is already in the right file with the right context loaded. An org
  of 200 engineers running agents becomes 200 part-time knowledge maintainers instead of two
  full-time ones — and it is the same insight that made the *contradiction* surface work, applied
  to upkeep. (d) It compounds with finding 1: consequence ranking is exactly the candidate
  selector the prover sweep needs, and the prover is exactly what clears the mechanical tail of
  this queue. Together they close the loop — the system decides what matters, proves what it can,
  and asks a human only where it must.

- **Fix sketch**:
  1. **Slice one — the missing signal (smallest thing that proves the whole idea).** New
     migration `memory_reads`, a direct transposition of `migrations/0025_document_reads.sql`:
     `org_id, memory_id, via ('http'|'mcp'), was_expired boolean, read_at`, append-only grant,
     same RLS policy shape, same two indexes. Record it where the memories are actually served —
     `mcp::memory_search` (`mcp.rs:820`), `memory_context`, and `memory_list` already build the
     hit list and already batch `feedback::trust_for` and
     `governance::open_contradictions_for` over `ids`, so this is one more batched write on the
     same slice, not an N+1. Surface `memory_reads_30d` / `agent_memory_reads_30d` /
     `expired_memory_reads_30d` in `KhSignals` beside the page-read trio, and follow the repo's
     own stated posture (`console.rs:2626-2629`): measure first, calibrate the pillar lever later.
     Even this slice alone is a shipping win — "7 agent sessions this month were served a belief
     the org had already moved past" is the single most alarming true sentence this product can
     say, and it currently cannot say it.
  2. **Slice two — give attention items identity and a rank.** Persist them: `attention_items`
     (`id, org_id, kind, severity, subject_type, subject_id, headline, detail, consequence real,
     projected_gain int, assignee_user_id, state ('open'|'snoozed'|'resolved'), first_seen_at,
     resolved_at`), upserted by kind+subject so an item that persists keeps its id and accrues an
     age (the same "cadence is the debounce" logic `alerts.rs:18` already reasons about). Move the
     ranking into a pure `consequence_of(reads_30d, expired_reads_30d, blast, harm_class)` in
     `core/health.rs` next to `rule_is_dormant`, unit-tested the same way — and drop the
     source-order push list and the per-category `LIMIT 5`. `alerts.rs::alert_sweep` then pushes
     *items with ids and links* instead of restating aggregates.
  3. **Slice three — the forecast.** `projected_gain` per item = `composite_score(...)` recomputed
     with that item's contribution removed (one fewer cross-team contradiction; `stale − n`;
     backlog drained), differenced against the live score. Pure function calls, no IO. Render it
     in `KnowledgeHealth.tsx` as the headline above the list — "clearing these six moves you 61 →
     78" — which finally makes good on the file's own opening stance that "a score with no next
     action is a vanity metric" (`KnowledgeHealth.tsx:8`). Store the realised delta on resolution
     so the ledger can be honest when a projection was wrong.
  4. **Slice four — dispatch.** New MCP tool `knowledge_maintenance` (register beside
     `memory_search`/`memory_provenance` at `mcp.rs:433`): given the session's repo fingerprint —
     auto-attribution already ships — return the top-N open items whose subject memories anchor to
     that project/entity set, each with the action that closes it (reverify with evidence,
     deprecate, resolve contradiction, retire rule). Closure flows through the finding-1
     verification ledger, so an agent-supplied confirmation is recorded with method `prover` or
     `check_evidence` and never as a bare clock reset.

  *Deliberately not this:* the shipped feedback→dispute triage queue is **reactive** — it collects
  what readers *reported*. This is **proactive**: it ranks rot nobody has complained about yet, by
  what it is costing right now, and it forecasts the payoff of fixing it. The overlap is that a
  dispute becomes one high-consequence input among several.
