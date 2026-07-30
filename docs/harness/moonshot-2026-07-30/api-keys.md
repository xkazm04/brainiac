# Moonshot scan — api-keys

> Total: 2
> Files read: 15

Grounding note (verified, not assumed): the schema already has an actor vocabulary —
`provenance(actor_kind, actor_id)` with `ActorKind::{Human,Agent,Pipeline}`, and the MCP
`memory_provenance` tool advertises "WHO it came from". But there is exactly **one** writer of
that table in the whole workspace (`brainiac-store/src/governance.rs:115`, called only from
`brainiac-pipeline/src/extract.rs:644`), and it hardcodes
`ActorKind::Pipeline, "extract-worker"`. So every memory in every org has the same author string.
`api_tokens` is the only table that knows the org's agents exist, and the only thing it records
about each one is `last_used_at`. Nothing downstream — not a memory, not a feedback verdict, not a
library fetch, not a page read — carries the id of the key that caused it.

---

## 1. The Agent Registry: turn every key into a named actor with a track record, and let that record govern

- **Severity**: Critical
- **Category**: agent-identity-graph
- **File**: `crates/brainiac-store/src/tokens.rs:13` (`ResolvedToken` — the id is fetched and
  thrown away), `crates/brainiac-server/src/auth.rs:114` (`AuthContext` — no token identity),
  `crates/brainiac-pipeline/src/extract.rs:644` (the one provenance writer, hardcoded
  `"extract-worker"`), `crates/brainiac-store/src/library/proposals.rs:68` (hour budget keyed on
  the *human*), `migrations/0001_init.sql:52` (`provenance` — the columns already exist),
  `console/app/console/modules/keys/keys-data.ts:70` (the demo data already *names* agents:
  "claude-code · pay-dev1", "ci-ingest · platform", "mcp-agent · analyst1" — the product's own
  fiction is a fleet roster; the backend cannot tell those three apart)

- **Scenario**: An org runs 40 coding agents. Each was minted a `brk_` key with a human-chosen
  name, and from that moment Brainiac treats the key as an **actor**, not a secret. Every memory
  an agent contributes, every feedback verdict it casts, every standard it proposes, every page
  and skill it consumes is stamped with that actor id. So the console can answer questions no
  memory server can answer today: *which of our agents are actually contributing knowledge, and
  which are polluting it?* `ci-ingest · platform` has written 212 memories, 89% promoted to
  canonical, 0 disputes — it is the org's most productive knowledge worker. `mcp-agent · analyst1`
  has written 34, 26 rejected at the gate and 3 later found contradicting canon — it is a
  liability, and the maintainer who spent three weeks reviewing its output can finally see why the
  queue felt like a treadmill. Two keys have not been used in 60 days and hold `admin`. One agent
  fired 140 `standard_propose` calls in a day and spent an entire team's proposal budget.
  Then the record starts *governing*, and this is where it stops being a dashboard: a proven
  agent's candidates take the light-touch promotion path; an unproven or regressing agent's writes
  are always routed to human review; a memory authored by a bad-record agent ranks below an
  equivalent one from a proven author in retrieval; proposal budgets are per-agent, not per-human,
  so one noisy actor cannot starve its colleagues. Trust becomes a property of the actor, learned
  from outcomes the governance loop already records — and it compounds every week the fleet runs.

- **Root cause**: The token is modelled as a *credential* (does this string authenticate?) and not
  as an *identity* (which actor did this?). `tokens::resolve()` selects
  `org_id, user_id, scopes, project_id` and deliberately drops `id`, so the actor is collapsed
  into the human it acts as before the request even reaches a handler. Downstream, every write path
  therefore has only a `user_id` to attribute to — which is why `extract.rs` writes a constant
  string instead. The ceiling this sets: **trust in Brainiac can only ever be per-memory, never
  per-actor.** Human review is the only quality mechanism, and human review does not scale with
  the number of agents — it is the one cost that grows linearly with fleet size. An org that goes
  from 3 agents to 40 gets 13x the review burden and no new leverage. Reputation is the only
  mechanism that scales *with* the fleet instead of against it, and it is currently impossible to
  compute because the data has no actor column filled in.

- **Impact**: The unlock is fleet-scale governance at flat human cost. Concretely: if a third of
  agent writes come from agents with a >80% historical promotion rate, auto-routing those past the
  gate cuts review volume by a third on day one and grows as the record thickens — while the
  reject-prone minority gets *more* scrutiny, not less. It also creates the first honest answer to
  "is our AI investment working?": per-agent contribution accepted vs rejected, per-agent
  consumption, dormant-but-privileged keys. And it makes Brainiac the **system of record for an
  org's AI fleet** — a category no one owns. Every company deploying coding agents will need a
  place that can name them, score them, and steer them; Brainiac's `api_tokens` table is already
  the only place in the stack where all of them appear. That is a durable platform position:
  whoever owns the fleet roster owns the control plane above it.
  Note it also reconciles with this repo's own privacy doctrine rather than violating it:
  `migrations/0028_library_substrate.sql:146` states the invariant "team, never person… NEVER A
  LEADERBOARD (L-never #1)". An agent is not a person. Attributing to a machine identity the
  operator named and can revoke gives the org full accountability *without* building the
  per-employee surveillance that invariant exists to forbid — the leaderboard is of robots.

- **Fix sketch**:
  1. **Carry the identity** (tiny, no schema change): add `id: Uuid` to `ResolvedToken`
     (`tokens.rs:13`) and to the `RETURNING` list in `tokens::resolve` (`tokens.rs:36` — the row
     is already being UPDATEd, this is free). Thread it as `agent_id: Option<Uuid>` through
     `AuthContext` (`auth.rs:114`) and `McpState` (`mcp.rs:188`), exactly parallel to how
     `project_id` was threaded for migration 0034. `None` = env bootstrap operator.
  2. **Stamp writes**: add `agent_token_id uuid` to `sources`, set it on ingest/`memory_add`, and
     change `extract.rs:644` to write `ActorKind::Agent` + the agent id when the source came from
     an agent key (falling back to today's `pipeline/extract-worker` for sweeps and backfills).
     Same column on `memory_feedback` (0004), `library_usage_events` (0028 — agent id, no user
     column added, invariant intact), and `document_reads` (0025, which today records only
     `via IN ('http','mcp')` and says "we do not pretend to know who they are" — now we can).
  3. **Smallest slice that proves it**: `memory_provenance` (mcp.rs:2097, http.rs:988) returns the
     agent's *name* from `api_tokens.name` instead of the constant `"extract-worker"`, and the
     Keys page grows one column: memories contributed / promoted / rejected per key, computed by a
     single grouped query over `provenance → memories.status`. That alone is a demoable product
     truth no competitor shows, and it validates the whole plumbing before any policy depends on it.
  4. **The scoreboard**: a `fleet_agent_stats` view (writes, promotion rate, disputes raised
     against, contradictions introduced, feedback verdicts upheld/overturned, reads, last seen) +
     `GET /v1/fleet/agents`, rendered as a Fleet module beside Keys.
  5. **Close the loop**: key the proposal hour budget on the agent instead of `p.author`
     (`proposals.rs:73`, `skills.rs:240`); add an agent-record term to the promotion policy
     decision; add it as a ranking signal on the already-shipped recency+feedback seam. Ship 5
     last — it only earns trust once 4 has been true for a few weeks of real data.

---

## 2. Recall notices: when a belief turns out to be wrong, reach the agents that already acted on it

- **Severity**: High
- **Category**: fleet-feedback-loop
- **File**: `crates/brainiac-server/src/mcp.rs:1515` (the existing precedent — a served page
  records a read), `migrations/0025_document_reads.sql` (the read-event log, actor-blind by
  design), `migrations/0001_init.sql:181` (`contradictions.resolved_at` — the moment a belief
  becomes wrong is already timestamped), `memories.superseded_by` (0001:80),
  `crates/brainiac-store/src/tokens.rs:36` (the identity that makes an addressable fleet possible)

- **Scenario**: A maintainer resolves a contradiction: "we use Redis for session state" is
  reversed — it has been Postgres since March. Today that resolution makes the *corpus* correct and
  stops there. In the new world the resolution screen also says: **"this belief was served 14 times
  to 6 agents across 3 repos in the last 9 days"** — and the org can *reach them*. Every subsequent
  MCP tool response those agents receive carries a `notices` envelope: *"a belief you relied on 4
  hours ago in `payments-api` has been REVERSED. Old: … New: … You may have written code on the old
  assumption."* An agent starting a session polls `fleet_notices` and self-corrects before writing a
  line. The console grows a **Recalls** board where the maintainer sees blast radius per reversal
  and can dispatch a re-check to the specific agents and repos affected. Brainiac stops being a
  library agents read from and becomes a **fleet nervous system**: governance decisions propagate
  outward to the work, not just inward to the database.

- **Root cause**: The retrieval surface is strictly pull-only and anonymous. Brainiac serves a
  memory, the agent acts, and the transaction ends with nothing recorded about *who* received
  *what* — so at the moment a belief is invalidated there is no addressable set of consumers, and
  no channel to reach them if there were. The constraint is doubled: no actor identity (finding 1)
  and no served-memory ledger. Everything else needed already exists — supersession application,
  contradiction resolution, dispute triage and feedback verdicts all already know the exact instant
  a belief flipped. That knowledge currently dead-ends in the corpus.

- **Impact**: This is the difference between *correct knowledge* and *corrected work*, and it is
  the highest-stakes gap in the product's own mission ("retrieval quality an agent can trust"). A
  wrong canonical memory today silently propagates into every PR that touched it in the window
  between the write and the reversal; nothing ever walks that back. With recall, the mean time from
  "the org realises X is wrong" to "every agent stops acting on X" drops from *never* to the
  agent's next tool call — minutes. It also turns governance work into visibly high-leverage work:
  a maintainer resolving one contradiction can see they just protected 6 agents and 3 repos, which
  is the single strongest argument for the review queue that the product could possibly show
  (KB-PLAN already reaches for this instinct with `document_reads.was_dirty`: "a dirty page nobody
  reads is a chore, a dirty page being read is misleading someone right now" — recall is that idea
  finished, at memory granularity and *addressed to the reader*). Strategically it is a moat: a
  push channel to a named fleet is the surface every later fleet-control feature hangs off, and it
  is only buildable by whoever owns the identity table.
  Distinct from the sibling demand-ledger context: that covers what agents *asked for and did not
  find*. This covers what they *were given* and what happened to it afterwards.

- **Fix sketch**:
  1. **Serve ledger**: new `memory_serves(org_id, agent_id, memory_id, tool, project_id,
     served_at)`, append-only with the exact grant/RLS shape of `document_reads` (0025) — insert
     from the result assembly in `memory_search` / `memory_context` / `memory_list` in `mcp.rs`,
     one batched insert per call, mirroring the `doc_get` read-record at `mcp.rs:1515`. High volume
     by nature: give it a retention window from the start via the existing sweep machinery
     (`0018_sweep_schedules`, `0024_raw_ttl_sweep`) — a 30-day window is all recall needs.
  2. **Smallest slice that proves it** (read-only, no push): one query behind the contradiction
     resolution and dispute screens — *"who was served this memory before it flipped?"* — rendered
     as blast radius on the resolve dialog. This is immediately valuable, ships without touching
     the agent protocol, and makes the case for step 3.
  3. **Notice generation**: a `fleet_notices(agent_id, memory_id, kind, old/new, created_at,
     acknowledged_at)` table written by the paths that already know a belief flipped —
     apply-supersessions, `contradictions` resolution, dispute outcomes — joined against
     `memory_serves` inside the retention window.
  4. **Push**: inject an optional `notices` array into MCP tool results in the dispatch path
     (`mcp.rs:413`+), capped and prioritised so it can never displace the answer; add a
     `fleet_notices` MCP tool (scope `read`) for session-start self-check, and mark delivery
     acknowledged when the agent has seen it. The tool descriptions in this file already coach
     agents on mid-task behaviour — this is the same voice, one step further: the server now
     volunteers a correction instead of waiting to be asked.
  5. **Recalls board** in the console beside Fleet: reversals ranked by blast radius, per-agent
     acknowledgement state, dispatch-recheck action.
