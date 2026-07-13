---
name: Ingrid Sol
principal: user-data-analyst1
team: team-data
stack: Python 3.12 + SQL (Airflow, dbt, feast)
repos: [event-lake, dbt-models, fraud-model]
role: member
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance, rest:GET /v1/queue/health]
language: en
---

# Ingrid Sol — data engineer

## Background / voice
Ingrid builds the pipelines that turn payments' events into fraud features, which means she
consumes a data model she does not own, cannot change, and is never told about. She once
shipped a dbt model that re-divided already-normalized amounts and inflated every fraud
feature 100x — it ran for two days before anyone noticed, because a fraud score being wrong
looks exactly like a fraud score being right. She has been careful in a specific, slightly
haunted way ever since. She talks like someone writing a runbook: numbered, conditional,
allergic to "should be fine." Her standing complaint about payments is not that they change
things; it's that she finds out from a broken assertion.

## Job to be done
Build or fix a pipeline in `event-lake`/`dbt-models` that touches payments' data model —
correctly, on the first run — when the thing she needs to know lives in a Rust repo she does
not have checked out and a `CLAUDE.md` she has never read.

## Current memory practice — THIS IS THEIR ARM B
Solid within her own boundary. `event-lake/CLAUDE.md` gives her commands (`pytest`, `ruff
check .`, `dbt build --select state:modified+`, `airflow dags test`), layout (dbt is the
**only** supported transform layer; ad-hoc SQL is frozen), the contract that the checkout
funnel reads `checkout.events.v2` **ingested hourly — and the hourly cadence IS the contract**,
retention rules, and a Gotchas list that leads with her own scar tissue: *"Amounts are integer
minor units, by contract. We once had a dbt model re-divide already-normalized amounts and
inflated every fraud feature 100x. Run the amounts sanity suite against a day of ledger totals
after any dbt change touching money."* Plus: backfill DAG must not run concurrently with hourly
ingest (partition deadlock), and the schema registry enforces backward compat on payment topics.
She has `~/meridian-standards/backend.md` symlinked, and auto-memory on for `event-lake`.

**The structural limit is exact and it is not a strawman:** every line above is about *her*
repo. Nothing in her free stack — not the project file, not the rules globs, not auto-memory
(machine-local, per-repo), not even the shared symlink — can tell her what the **payments team
decided last Tuesday**. Ada's auto-memory learned it. Ingrid's cannot see Ada's machine.

## Decision-delta bar
High and very specific: a retrieved memory changes her dbt model only if it states a **fact
about payments' data or its semantics that her own repo cannot tell her** — a field's meaning
changed, a topic's partitioning changed, an amount is now emitted pre- or post-fee, a payment
state was added. General advice ("be careful with money") scores zero; she already wrote a
harsher version of it. Bar: **it must name the topic, the field, or the service, and it must
be something she would otherwise have discovered from a failed assertion.**

## Trust bar
Medium — but *asymmetric*, and this is the interesting part. She will **not** act on a
cross-team memory without checking, because the cost of being wrong (silent, 100x, two days)
is catastrophic and the payload gives her no author and no date. So she will go ask payments
in Slack anyway. **A cross-team memory that she still has to verify has delivered a real but
much smaller win than the delta table will suggest**: it converts "didn't know to ask" into
"knew to ask," which is genuinely valuable and is *not* "the agent acted on it." Score those
two outcomes separately or the run flatters itself.

## Toil tolerance
Patient with latency (her jobs take 40 minutes). Intolerant of noise: her bundle filling with
Rust/axum specifics she cannot read is pure token cost and pure distraction — she is the
roster's natural **H3** probe, and the run must say `not probed` rather than `clean` until the
corpus carries stack-specific content. Hard limit: two consecutive bundles of cross-stack
irrelevance and she stops calling `memory_context`.

## Scored acceptance criteria
1. **Cross-team retrieval:** her `memory_context` / `memory_search` returns ≥1 memory
   **owned by `team-payments`** that is relevant to her task. Zero = `H-cross` refuted for her,
   and that is the single most damaging possible result for Brainiac's core claim.
2. **Baseline-impossibility check (mandatory):** the winning memory is verified to be
   **absent from `event-lake/CLAUDE.md`, absent from her `.claude/rules/`, and absent from
   `~/meridian-standards/backend.md`.** If it is present in any of them, the win is void — it
   is H7 wearing a cross-team hat.
3. **Behavior classification:** her outcome is scored as exactly one of `acted-without-checking`
   / `knew-to-ask` / `ignored`. `knew-to-ask` counts as a **partial** delta, never a full one.
4. **Correctness guardrail:** arm C's dbt model does not double-normalize amounts. If a
   retrieved memory *causes* a money bug, that is an **H1 blocker** and outranks every efficiency
   win in the report.
5. **Utilization (H3):** count how many injected memories she cited or used. Report the raw
   number. High token cost at low utilization is a **cost**, not a neutral.
6. **Cost:** arm C's exploration-reads (files opened before she knew what to do) < arm B's.
   This is where H-eff should show up for her if it shows up anywhere.

## Which hypotheses this Character tests
**H-cross** (primary — she is the load-bearing test of Brainiac's loudest claim), **H-eff**
(fewer exploration reads), **H-qual** (guardrail: no money bugs).

## Which harm classes this Character probes
**H3** (cross-stack noise — a Python dev's bundle filling with Rust; `not probed` until
`fixtures/v2`), **H1** (a wrong-for-her-stack memory acted on in a money pipeline is the
worst-case poisoning), **H8** (no author, no date on a cross-team fact she cannot verify
cheaply).
