# Moonshot scan — practice-divergence-ui

> Total: 2
> Files read: 18

## 1. Give a divergence an identity, a lifetime, and a burn-down — the org's Alignment Index

- **Severity**: Critical
- **Category**: durable-instrument
- **File**: `crates/brainiac-pipeline/src/divergence.rs:240-269` (the `DELETE ... INSERT` replace)
  · `migrations/0016_practice_divergences.sql:11-24` (no natural key, no state, no `first_detected_at`)
  · `crates/brainiac-server/src/console.rs:2981-3002` (the read that omits `id`)
  · `crates/brainiac-pipeline/src/library_sweep.rs:104-129` + `:63-77` (`blocked()` keyed on the re-minted id)
  · `console/src/divergence/PracticeDivergence.tsx:148-154` · `console/src/divergence/variants/StandardsVariantBoard.tsx:277-290`
- **Scenario**: A platform lead opens the board in October and does not read sixty
  arguments. They read four numbers: *alignment 71, up 9 this quarter; 3 practices
  converged; 1 re-diverged; median time-to-standardize 34 days*. Every row carries
  its own age and direction — "open 5 months, widening: data joined last month" or
  "converging: risk adopted the standard three weeks ago." The matrix gains a time
  axis, so the interference pattern animates: you watch payments and platform pull
  apart on retry policy across four sweeps, then snap together after ratification.
  Ratified and consciously-accepted rows leave the board and do not come back, so
  the board's length is a real backlog that a lead can burn down and be *seen* to
  burn down. The empty state stops meaning "we found nothing" and starts meaning
  "we resolved fourteen." And because `scan_all` already loops every org, the same
  index is comparable across tenants: an anonymized fleet percentile ("your
  alignment is 71; the median org running Brainiac is 58") turns a per-org report
  into a benchmark nobody else in the category can compute.
- **Root cause**: A practice divergence today is not an entity — it is the
  *output of the last sweep*. `scan_org` opens with
  `DELETE FROM practice_divergences WHERE org_id = $1` and re-inserts every
  verdict with a fresh `gen_random_uuid()`. Nothing in the schema is stable across
  runs: no `(org_id, canonical_id, practice, axis)` key, no `first_detected_at`
  vs `last_seen_at`, no state column, no observation history. Three ceilings
  follow directly, and they are the reason this screen is *shown* but never
  *decided* or *followed*:
  1. **No trend, therefore no metric.** Age, direction (widening/converging),
     time-to-standardize and any index over them are unrepresentable. The board
     can only ever be a snapshot, and a snapshot is not something an executive
     tracks.
  2. **No memory of decisions.** A ratified or deliberately-tolerated divergence
     is re-detected and re-rendered at full impact on the next sweep, forever.
     The lead's decision has no effect on the surface that asked for it — which
     is exactly why the "ratify as standard" button in the variant board is
     dashed-disabled with the note *"nothing on the server records a
     ratification."* (The server bridge `library::ratify_divergence` does exist;
     the console cannot call it because the analytics read does not select `id`.)
  3. **It actively corrupts the downstream loop.** `standard_provenance.ref_id`
     points at a `practice_divergences.id` that the next sweep deletes — every
     ratified standard's evidence becomes a dangling pointer. Worse,
     `library_sweep::blocked()` dedupes mined candidates by that same `ref_id`,
     so after the ids are re-minted it can never hit: the mining sweep proposes a
     *fresh duplicate standard candidate for the same practice on every sweep*.
     The LB2 triage queue grows as sweeps × divergences. The most valuable signal
     in the product is stored in the one shape that makes it unciteable.
- **Impact**: Converts a recurring snapshot into the only longitudinal
  organizational-alignment instrument on the market. Concretely: (a) an
  executive-trackable scalar with a quarter-over-quarter direction, which is what
  gets this screen into a leadership review instead of a platform-team tab; (b)
  board length becomes monotonically reducible — a 60-row first sweep can be
  driven to 8 and *proven*, where today re-detection guarantees it never shrinks;
  (c) duplicate mined candidates drop from O(sweeps × divergences) to exactly one
  per practice, and standard provenance stops dangling; (d) mean-time-to-
  standardize becomes a platform-team SLO. The fleet percentile is a network
  effect: it gets stronger with every tenant and cannot be replicated by anyone
  without a multi-org memory corpus.
- **Fix sketch**:
  1. **Identity (smallest first slice, provable in one test).** Add
     `divergence_key text` = `slugify(practice)` (reuse `library::slugify` — it is
     already pure and tested) plus a unique index on
     `(org_id, canonical_id, divergence_key, axis)`, and `first_detected_at` /
     `last_seen_at` / `last_model_ref`. Replace the `DELETE`+`INSERT` in
     `divergence.rs:240-268` with `INSERT ... ON CONFLICT (...) DO UPDATE SET
     summary, recommended_standard, impact, positions, last_seen_at, last_model_ref`,
     and a single `UPDATE ... SET state='resolved'` for keys the sweep no longer
     produced (soft-close: *stop deleting evidence*). The test to write first is
     the one that proves the point: run `scan_org` twice against the same fixture
     corpus and assert the row id is unchanged and `first_detected_at` is
     preserved — that assertion fails today.
  2. **Lifecycle.** `state text CHECK (state IN
     ('open','ratified','accepted_variance','resolved','regressed'))`, plus
     `decided_by`/`decided_at`/`decision_note`. `ratify_divergence` in
     `library/bridge.rs:43` also stamps `state='ratified'`; a new
     `POST /v1/library/divergences/{id}/accept-variance` records the "unless a
     specific use case is identified and approved" clause the demo
     recommendation already writes in prose. A sweep that re-detects a
     `ratified`/`accepted_variance` key sets `state='regressed'` instead of
     `open` — a *regression* is a louder finding than a new divergence and
     deserves its own lane above `high`.
  3. **Observations.** `divergence_observations (divergence_id, observed_at,
     impact, group_count, positions_hash)`, one row appended per sweep per key.
     Direction is then derivable without an LLM: `group_count` rising =
     widening, positions_hash stable + group_count falling = converging.
  4. **Expose it.** `console.rs:2982` selects `id, state, first_detected_at,
     last_seen_at, direction, age_days` (the missing `id` alone unblocks the
     dashed ratify button), and a sibling `GET
     /v1/analytics/practice-divergence/trend` returns the org series +
     `alignment_index` (share of shared practices with no open divergence,
     weighted by impact). Add it to the knowledge-health snapshot sweep in
     `sweeps.rs` so the series accretes without a new scheduler.
  5. **Render the new facts.** `divergence-scale.ts` grows a state/age/direction
     generator (it already prototypes against the bad day, which is the right
     place to design the burn-down); `StandardsVariantBoard` gains a `regressed`
     lane and per-row age/direction; `StandardsVariantMatrix` gains a sweep
     scrubber over the observation series. The fleet percentile is last and
     opt-in: an org-level `benchmark_opt_in` flag and a cross-org aggregate that
     shares only `(practice_key, count)` — never a summary, position, or team
     name — so the "zero permission leaks" promise survives the network effect.

## 2. Conformance: prove the standard was actually adopted, and make the agent the enforcement surface

- **Severity**: High
- **Category**: closed-loop-enforcement
- **File**: `crates/brainiac-pipeline/src/divergence.rs:114-238` (the sweep that
  would host the conformance pass) · `crates/brainiac-store/src/library/bridge.rs:65-143`
  (the ratification that currently ends the story) · `crates/brainiac-server/src/mcp.rs:1666-1731`
  (`standards_for` — pull-only, stack-scoped) · `crates/brainiac-pipeline/src/faithfulness.rs:35-46`
  (the reusable "judge a claim against evidence" pattern) · `console/src/divergence/variants/StandardsVariantMatrix.tsx:404-441`
  (cells that would carry compliance instead of mere presence)
- **Scenario**: Six weeks after the lead ratifies *"2 s retry cap, 3 attempts, for
  every internal service call,"* the board answers the only question that matters:
  did anything change? The matrix cell for `payments × service retry policy` is no
  longer "this team has an approach" — it is **compliant** (green, since the
  2026-08-14 sweep), while `data` is **still divergent** (magenta, 41 days after
  adoption) and `risk` is **exempt** (a recorded variance with a named approver and
  an expiry). Each standard carries an adoption curve: 5 teams at ratification →
  2 divergent today. Leadership reads one line that no other tool in this category
  can produce: *"24 standards adopted; 9 actually followed; 3 regressed."*
  Meanwhile the loop is closed on the *agent* side too: when a coding agent
  retrieves the payments team's 30-second-retry memory, the response says the
  memory sits on the non-compliant side of an adopted standard and quotes the rule
  — so the agent writes the ratified practice instead of faithfully propagating
  the drift. The divergence board stops being a report about the past and becomes
  the mechanism that ends the drift it detected, then grades its own work.
- **Root cause**: Ratification is a terminal state in the code. `bridge.rs`
  turns a divergence into a `proposed` standard and the causal chain stops there:
  nothing ever re-reads the org's memory corpus to ask whether each diverging
  group's *practice* moved. The two halves that could close the loop never meet —
  the divergence sweep clusters memories by canonical entity but knows nothing
  about `standards`; `standards_for` serves adopted rules but only when an agent
  asks, scoped by stack, with no idea which memories contradict them and no
  awareness at *retrieval* time (the hybrid retrieval path hands back a divergent
  team memory as settled truth). `library_usage_events` measures whether a rule
  was *fetched*, which is a proxy for attention, not for adoption — a standard can
  be checked a hundred times and obeyed by nobody. And the "unless a specific use
  case is identified and approved" escape hatch that the adjudicator itself writes
  into recommendations has nowhere to live as data, so every legitimate exception
  looks identical to non-compliance and the board slowly trains the lead to ignore
  it. Result: the org can adopt standards forever without converging, and nothing
  in the system would notice.
- **Impact**: Turns a detection product into an enforcement-and-verification
  product — the difference between "we have standards" and "we can prove our
  standards took." It creates the first defensible answer to *"what did
  standardization actually buy us?"*: a per-standard adoption curve, a
  time-to-convergence figure per team, and a regression alarm when a compliant
  team drifts back. It also multiplies retrieval quality, which is the stated
  mission: today an agent retrieving org memory has roughly a 1-in-N chance of
  picking the losing side of every open divergence and writing that drift into new
  code; divergence-aware retrieval collapses that to zero on any practice with an
  adopted rule, and makes Brainiac the reason the drift stops rather than the
  place it is recorded. Distinct from the queued *standards usage ranking* work:
  that counts agent tool calls, this measures the corpus.
- **Fix sketch**:
  1. **Smallest first slice — the conformance judge, one axis, no UI.** A new
     `conformance.rs` beside `divergence.rs`, reusing its exact shape: for each
     `adopted` standard that carries a `divergence` provenance row, re-gather the
     cluster with the query at `divergence.rs:122-139` (canonical entity → per-team
     positions), then one provider call per (standard, group) with a judge prompt
     modeled on `JUDGE_SYSTEM_PROMPT_V1` — *does this group's recorded practice
     comply with this statement?* → `{compliant: bool, note: string}`, temperature
     0, conservative-by-default like the divergence prompt (`unknown` when the
     group has recorded nothing, never `non_compliant` by silence — a team that
     said nothing has not violated anything). Store into
     `standard_conformance (standard_id, org_id, group_kind, group_name,
     state CHECK (state IN ('compliant','divergent','exempt','unknown')),
     memory_ids uuid[], note, model_ref, observed_at)`, append-only so the curve
     is a `SELECT`, not a recomputation. Prove it with a `_pg` test in the shape
     of `crates/brainiac-eval/tests/divergence_pg.rs`: a fixture where one team's
     memory is edited to match the standard, and the sweep must flip exactly that
     team's row to `compliant`.
  2. **The exception register.** `standard_exemptions (standard_id, group_name,
     reason, approved_by, expires_at)`; conformance reads it and reports `exempt`
     rather than `divergent`. Expiry is the honest part — an exemption that never
     expires is a silent repeal, so an expiring exemption re-opens as `divergent`
     on the next sweep.
  3. **Wire it into the existing sweep.** Run the conformance pass at the end of
     `divergence::scan_org` (same provider, same admin pool, same
     `DivergenceStats` return, extended with `conformance_checked` /
     `conformance_divergent`) so it inherits the schedule in `sweeps.rs` with no
     new scheduler and no new operator surface.
  4. **Agent-side closure — the part that causes convergence.** Add a
     `standards` block to the retrieval/MCP response: when a returned memory's id
     appears in a `standard_conformance` row with `state='divergent'`, attach
     `{slug, statement, enforcement, "this memory reflects a practice the org has
     since standardized away from"}`. This is a strict superset of the shipped
     contradiction-awareness pattern and needs no new tool — it rides the response
     an agent already reads, and it is what makes the adoption curve move rather
     than merely be observed.
  5. **Render it.** `StandardsVariantMatrix` cells switch from binary presence to
     four-state conformance (the sticky-header grid and per-team hues already
     built are exactly the right instrument for a compliance field); the board
     row's `recommend` block becomes *"adopted 2026-08-01 · 3 of 5 compliant"*
     with the divergent groups named; and the new
     `GET /v1/analytics/standards-conformance` feeds a
     `standards_adopted / standards_followed / regressed` triple into the
     knowledge-health report, where `LibraryHealth` already lands the Library's
     leadership numbers in one round trip.
