# Moonshot scan — analytics-dashboard

> Total: 2
> Files read: 12

Framing note before the findings. Everything this context renders today is **inventory**: how
many memories exist, in what state, written by whom, reviewed how fast. Every number on
`Observatory.tsx` and every field of `AnalyticsResponse` (`console.rs:1779`) can be computed
without a single agent ever asking Brainiac a question. That is the ceiling: an org cannot tell
from this dashboard whether the system is worth its Postgres bill, because **the product does not
record that it was used**. Verified, exhaustively: there is no query log, no served-memories log,
no miss record, no latency record, and no outcome record anywhere in the 42-table schema.
`retrieval::search` / `search_reranked` (`crates/brainiac-store/src/retrieval.rs:217`, `:234`) contain
zero writes and zero `tracing::` calls; all three chokepoints (`http.rs:369`, `mcp.rs:827`,
`mcp.rs:1084`) return JSON and forget. `mcp.rs:863` even *computes* `withheld_contested` — the count
of answers the governance model suppressed — hands it to the agent at `:934`, and drops it.

The only two event logs that exist are supply-side: `document_reads` (`migrations/0025`, "this page
was viewed") and `library_usage_events` (`migrations/0028_library_substrate.sql:149`, "this artifact
was handed over"). Neither is keyed to a question.

Both findings below aim at that. Finding 1 is the measurement product itself. Finding 2 is the
network good that measurement makes possible. Neither duplicates the knowledge-health snapshot,
practice divergence, or spend-ledger siblings — and where a sibling context also wants a demand
table (`sweep-pipeline.md` proposes `retrieval_demand` + a gap-mining sweep; `console-shell.md`
proposes a `retrieval_events` emit), treat the write path as **one shared substrate built once**:
they want it to mine gaps, this wants it to prove value and attribute outcomes. Building it twice
would be the failure mode.

---

## 1. The Knowledge P&L — make Brainiac the only memory system that can prove it changed an outcome

- **Severity**: Critical
- **Category**: value-attribution
- **File**: `console/src/observatory/Observatory.tsx:102` (the headline gauge row — five inventory
  counters, the page's thesis statement) · `crates/brainiac-server/src/console.rs:1794` (`analytics`)
  and `:1994` (`observatory`) — the producers, both pure `count(*)` over corpus tables ·
  `crates/brainiac-server/src/mcp.rs:863` (`withheld_contested` computed then discarded) ·
  `crates/brainiac-store/src/retrieval.rs:234` (`search_reranked`, no instrumentation) ·
  `crates/brainiac-store/src/feedback.rs:33` (verdict insert with no query linkage)

- **Scenario**: A VP Eng opens `/console` on a Monday and the top band no longer says
  *"canonical: 812 · total memories: 1,104 · pending review: 3"*. It says:

  > **4,109 questions answered · 91% answered rate · 214 engineer-hours returned this quarter ·
  > $2.1M avoided rework at loaded cost · causal lift +34% vs the withheld arm**

  and every one of those numbers is a link. Clicking *214 hours* lists the individual claims:
  *"`payments-api`, 2026-07-22 14:03 — agent asked `psp retry idempotency key`, was served memory
  `d41…` (a pitfall written by @maya in March), reported `used`, landed commit `8f2c1ab`. Estimated
  40m saved."* Clicking *91%* lists the 9% — the actual sentences agents asked that Brainiac had no
  good answer for, ranked by how many distinct repos asked.

  Procurement conversations change shape. Today the honest pitch is "our corpus is well governed."
  After this it is "here is the audited ledger of the 4,109 times we answered an agent, the 214
  hours that returned, and the randomized control arm that says a third of it is causal." A memory
  server that can produce that renews itself. One that shows a review-latency gauge does not.

- **Root cause**: The whole analytics surface is computed from the **write** side of the system
  (`memories`, `promotions`, `contradictions`, `entities`). The read side — the side users actually
  pay for — is not persisted at all, so no number about *use* can exist, and therefore no number
  about *value* can exist. Two structural consequences: (a) `memory_feedback` is the product's only
  return channel and it is attached to a memory with no `retrieval_event_id`, so a `helpful` verdict
  can never be counted as "the thing we retrieved worked" — only as "someone liked a row";
  (b) unanswered demand is invisible by construction, so the corpus grows toward whatever sources
  were easy to ingest rather than toward what agents keep failing to find. The precedent for fixing
  it is already in the codebase and deliberately narrow: `library_usage_events` records `fetch`
  server-side and takes `apply` on the agent's word (`mcp.rs:1987-1988`). That exact honesty split
  generalizes to retrieval.

- **Impact**: 10x on the *category* of the surface, not its resolution. Concretely:
  (1) **An answered-rate and a coverage number** — the two numbers a buyer cares about — become
  computable for the first time; today both are structurally unknowable.
  (2) **A defensible ROI figure with per-claim evidence**, which is the difference between a tool
  that survives a budget review and one that does not. Assume a 200-engineer org and even 15 min
  saved on 20% of 30k monthly agent retrievals: ~1,500 h/quarter, ~$220k at $150/h loaded — and
  crucially the ledger shows *which* memories earned it, which tells the org where to invest
  curation effort next.
  (3) **A causal claim, not a correlation.** A configurable 1-in-N withhold arm (default off) makes
  "retrieval improved the outcome" measurable instead of asserted, which no competitor in this
  space currently does and which is exactly the register of the project's stated mission
  ("an eval harness that proves every claim").
  (4) **Curation gets a gradient.** The top unanswered questions become the ingest backlog, closing
  the loop the project's flywheel is missing.

- **Fix sketch**: Five slices, in order. Each is independently shippable.

  1. **Record demand (the substrate — coordinate with `sweep-pipeline`/`console-shell`).** New
     migration, modelled line-for-line on `migrations/0025_document_reads.sql` (append-only:
     `GRANT SELECT, INSERT` only, org RLS policy, self-granting to `brainiac_app`):
     `retrieval_events (id bigint identity, org_id, project_id null, principal_user_id, token_id,
     surface check in ('http','mcp'), tool text, query_text text, query_norm text, returned_n int,
     top_score float8, withheld_contested int, latency_ms int, arm text default 'served',
     occurred_at)`, plus the join table that makes attribution possible:
     `retrieval_event_memories (event_id, memory_id, rank int, score float8)`.
     Write it fire-and-forget in its own tx with warn-only failure, using the existing wrapper
     pattern `record_library_usage_quietly` (`mcp.rs:1636`) / `docs::record_read`
     (`crates/brainiac-store/src/documents.rs:504`), at the three chokepoints: `http.rs:369`,
     `mcp.rs:827`, `mcp.rs:1084`. Note `http.rs:360 search` never commits its tx today — the emit
     must be a separate connection, not that tx. `latency_ms` has precedent too: `sweeps.rs:344
     record_result` is already the one path that persists duration.
     *Privacy is the design constraint, not an afterthought:* `query_text` is org-scoped under RLS
     and never leaves the tenant; `query_norm` (lowercased, identifier-preserving) is what any
     aggregate reads. Follow `0028:16-18`'s explicit "never a leaderboard" stance — keep
     `principal_user_id` for support debugging, and never render a per-person ranking.

  2. **Answered rate + the miss list (smallest slice that changes the genre).** New
     `GET /v1/analytics/value` beside `analytics`/`observatory` in `routes()`
     (`console.rs:49-51`): `answered_rate` (share of events with `returned_n > 0 AND top_score >=
     threshold`), `queries_7d/30d`, `p50/p95 latency`, and `top_unanswered` (group by `query_norm`
     where below threshold, ordered by distinct `project_id` then count). In the console, add one
     tile — *"Questions Brainiac could not answer"* — into the existing `lg:grid-cols-3` grid at
     `Observatory.tsx:146`, and extend `ObservatoryData` (`observatory-data.ts:14`) plus its demo
     fixture. This alone converts an ops wall into a demand instrument.

  3. **Close the loop with outcomes.** Add nullable `retrieval_event_id` to `memory_feedback` and
     bind it in `feedback::insert` (`feedback.rs:23`) so a `helpful` verdict finally means "this
     answer worked for that question". Then a new MCP tool `memory_outcome` (scope `write`,
     registered at `mcp.rs:223`/`:525` alongside `memory_feedback`), mirroring the shape and the
     trust posture of `skill_report_usage` (`mcp.rs:1989`): `{event_id, disposition:
     used|partially|not_relevant|not_found, repo_remote?, repo_path?, commit_sha?,
     minutes_saved_estimate?}`. Resolve `repo_remote` + `repo_path` to a project with the existing
     longest-prefix matcher `projects::find_by_remote`
     (`crates/brainiac-store/src/projects.rs:165`) — repo→project attribution is already solved and
     segment-boundary safe. Keep the honesty split explicit in the tool description: the server
     records what it served; the agent asserts what it did with it.

  4. **The value model, computed one way.** A `brainiac_core::value` module next to
     `brainiac_core::health` (already the home of `REVIEW_SLO_SECS`, imported at `console.rs:2295`)
     — the same "one computation, many consumers" discipline `compute_health_core`
     (`console.rs:2343`) established. Inputs: outcome dispositions, org-configurable per-kind
     minute constants (with published defaults, and a visible marker when defaults are in use so
     the number is never silently fabricated), loaded hourly cost. Outputs: `hours_returned`,
     `cost_avoided`, `answered_rate`, `coverage`. Render as a P&L band **above** the gauge row at
     `Observatory.tsx:102`, each figure linking to its evidence list. A number with no drill-down
     is a number nobody believes — the drill-down is the feature.

  5. **The causal arm.** Org-level `retrieval_holdout_n` (default 0 = off). When on, 1-in-N
     retrievals record `arm='withheld'` and return an empty/degraded result set; the outcome
     dispositions of the two arms give a lift estimate. Add a `value` profile to `brainiac-eval`
     alongside `retrieval_profile.rs` that replays recorded `query_norm` demand against the corpus,
     so answered-rate becomes a CI-gated quality number like every other claim in this project.

---

## 2. The Knowledge Index — turn per-org health into a cross-org benchmark, and analytics into a network good

- **Severity**: High
- **Category**: network-effect
- **File**: `crates/brainiac-server/src/console.rs:2906` (`snapshot_all_orgs` — an
  RLS-bypassing sweep that already computes comparable, content-free scores for **every org** and
  then files each one away where only its own tenant can see it) ·
  `migrations/0014_knowledge_health_snapshots.sql:9` (the table: aggregate scores only, by design) ·
  `console.rs:2343` (`compute_health_core` — one identical formula for all orgs, which is what makes
  cross-org comparison legitimate) · `crates/brainiac-server/src/sweeps.rs:267` (`execute` — five
  sweep kinds today) · `console/src/observatory/Observatory.tsx:102` and `:282` (the gauges and the
  themes chart, both of which currently have no reference frame)

- **Scenario**: Every gauge grows a ghost. *canonical 812* now sits against a faint band reading
  *cohort p61*. Governance reads *"78 — Watch · cohort p34, and slipping relative to peers for
  three weeks."* A new tile appears: **"What your peers know and you don't"** — *"`terraform`:
  63% of comparable orgs hold a canonical decision on it; you hold none. `kafka consumer rebalance`:
  a top-10 unanswered query for you and 41 peers — a standard exists."*

  Nobody can build that from inside one tenant, which is precisely why it is worth paying for. And
  it compounds: each org that opts in sharpens the benchmark for everyone, so the index gets more
  valuable exactly as it gets more adopted — the first genuine network effect in a product whose
  value is otherwise strictly per-tenant. Downstream, it becomes a market surface: the **annual
  Engineering Knowledge Index** ("the median engineering org's knowledge is 31% stale; top-quartile
  orgs resolve contradictions 6x faster"), a report that is both a category-defining artifact and
  the best demand-generation asset this product could own — publishable *because* it contains no
  customer content, only distributions over scores and topic vocabulary.

- **Root cause**: The architecture is single-tenant all the way up. RLS is (correctly) absolute, so
  every analytics number is computed inside one org's vantage and every score is an absolute with
  no reference frame — is 78 good? Nobody knows, including the buyer. Yet the hard part is
  *already built and already cross-org*: `snapshot_all_orgs` runs on the admin pool, walks
  `SELECT id FROM orgs`, and derives each org's TRUE totals through the same `compute_health_core`
  formula (`console.rs:2913`). The comparability substrate exists; only the aggregation step, the
  consent model, and the reference-frame UI are missing. `knowledge_health_snapshots` was
  deliberately built content-free ("Aggregate scores only — no memory content", `0014:5`), which
  means the privacy work that usually blocks this shape of feature is largely pre-paid.

- **Impact**: Adds the product's first compounding asset. Concretely:
  (1) **Reference frame** — every existing gauge becomes interpretable at zero new measurement
  cost, upgrading a whole dashboard's worth of numbers with one endpoint.
  (2) **Network effect** — value per tenant rises with tenant count, which is the difference between
  a tool and a platform, and creates a switching cost no feature copy can replicate.
  (3) **Topic-level coverage gaps** give curation a target the org would never have found alone,
  and each gap is a natural pull toward publishing/adopting a library standard — feeding the
  already-queued library publish path rather than competing with it.
  (4) **A category artifact.** The Index report is the kind of thing that defines a market and names
  its owner; it is also free demand-gen, computed from data the system already writes weekly.

- **Fix sketch**: Consent first, then aggregate, then render.

  1. **Opt-in, k-anonymous, by construction.** Add `orgs.cohort_opt_in bool default false` and
     `orgs.cohort_size_band text` (e.g. `<50 / 50-250 / 250+`, self-declared — the cohort key).
     Default OFF; contributing nothing until a maintainer opts in. Enforce a floor `k >= 5`
     contributing orgs per cohort: below it every cohort field returns `null` and the UI shows
     nothing rather than a misleading number. This is the same shape of guardrail the product
     already applies elsewhere (withhold rather than mislead — `mcp.rs:863`).

  2. **A `cohort` sweep, not a live query.** New kind in `sweeps.rs:267 execute` (sixth alongside
     `divergence`, `health_snapshot`, `raw_ttl`, `alerts`, `library`; seed the schedule row weekly
     like `0018:31`), scheduled to run **after** `health_snapshot` so it aggregates fresh rows.
     It runs on the admin pool and writes a tenant-free table
     `cohort_metrics (id, cohort_key, metric text, p25/p50/p75 float8, n_orgs int, computed_at)`
     from `knowledge_health_snapshots` over opted-in orgs — score plus each of the four pillars.
     Because it is a sweep, operators already have the control surface for it for free:
     `SweepControl.tsx` renders any schedule kind with a toggle, cadence, and run-now
     (`console/src/ops/SweepControl.tsx:73`), and `record_result` (`sweeps.rs:344`) already
     persists its status and duration.

  3. **Topic coverage, vocabulary only.** A second cohort table
     `cohort_topics (cohort_key, topic_norm, topic_kind, orgs_with_canonical int, n_orgs int)` built
     from `canonical_entities` name/kind (already the basis of the Loudest-themes chart's SQL at
     `console.rs:2049`) — names and kinds only, never memory content, and suppressed below the same
     `k` floor. Optionally join the miss list from finding 1 to produce the strongest row in the
     whole product: *a question you and 41 peers keep failing to answer*.

  4. **`GET /v1/analytics/cohort`.** RLS-scoped read of the caller's cohort row plus their own
     latest snapshot, returning percentile placements and the coverage-gap list; `null` everywhere
     when not opted in or below `k`, following the honest-fallback pattern the console already uses
     (`withDemoFallback`, `console/app/console/modules/analytics/Module.tsx:20`).

  5. **Render the frame.** In `Observatory.tsx`, extend `ObservatoryData`
     (`observatory-data.ts:14`) with an optional `cohort` block and add (a) a percentile ghost
     marker inside each headline tile at `:102`, (b) a `cohort pXX` line in the Governance column
     at `:175`, (c) the "peers know / you don't" tile in the grid at `:146`. Smallest first slice
     that proves the whole idea: **one number** — the org's percentile on overall `score` — behind
     the existing grade in the health module. If a leader reacts to "p34" the way past prototypes
     showed they react to "slipping three weeks running", the rest of the index earns its build.
