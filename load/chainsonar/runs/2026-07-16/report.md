# ChainSonar field test — findings (2026-07-16)

Three developers, three jobs, one real ~5,100-LOC codebase, one Brainiac org.
Memory, Knowledge Base, and Library exercised at once by eight agents (four
scanners + three developers + one maintainer). This is the report the eval
harness and `uat/` structurally cannot produce: **all three modules under
simultaneous, realistic traffic on code nobody wrote for the test.**

No score. Evidence-backed findings, each tagged **bug** / **design gap** /
**feature opportunity** / **harness**, each traceable to a logged call or a
missing one. Severity is my judgement.

## The verdict in a paragraph

Brainiac's central thesis held on a real codebase: **one agent's learning
reached another through the governed store and changed the work.** dev-c scaled
the UI by rendering fewer rows instead of paginating — because the scan's memory
told it the rate limit lives in the chain fetch, not the local reads. dev-a's
ingester error-model, and two bugs it caught in its own proof, came from the
org's ratified standards. The same README-drift memory saved all three
developers from the same dead end. The gate held throughout — evidence-free
adoption refused (409 → decree), scopes enforced, 11 cross-agent proposals
deduped to 8, a scanner refusing a leading prompt. **And the cost column is
real:** onboarding surfaced two shipped bugs (unmintable scopes, MCP rejecting
device keys); the memory *contribution* path is broken for the distilled facts
agents actually send (async id ≠ memory id, 36% extraction hard-fail, no write
confirmation); a genuine coupling bug (F-9) leaves the KB empty for an org that
builds its Library before its graph; and the run's own retrieval reads were
invalidated by a harness shortcut I own (H-3). Net: the value is real and
demonstrated, and so is a specific, fixable list of what stands between the demo
and a team trusting it.

## Findings at a glance

| id | finding | kind | sev |
|----|---------|------|-----|
| B-1 | `lib:*`/`kb:*` scopes unmintable — Library reachable only by admin | bug | high · **fixed** |
| B-2 | MCP rejected managed device keys — onboarding→agent broken | bug | high · **fixed** |
| F-1 | evidence chain unusable in a session (source_id ≠ memory_id) | design gap | high · **fixed** |
| F-2 | a contribution gives no success/failure signal | design gap | high · **fixed** |
| F-3 | extraction 36% hard-fails on the distilled facts agents send | bug | high · **fixed** |
| F-9 | standards-page (L8) can't scaffold without a graph — KB stayed empty | bug | high · **fixed** |
| F-4 | no skill-authoring path; memory has no kind/tag | design gap | medium · **fixed** |
| F-5 | REST/MCP surface asymmetries (adopted-only list, uuid vs slug…) | design gap | low · **fixed** |
| F-8 | `bx` shell-quoting taxes code examples | harness | medium |
| F-6 | honesty rule held under a leading prompt | positive | — |
| F-7 | cross-agent dedup collapsed 11 proposals → 8 | positive | — |
| H-1 | a field test must own its database (a mid-run test truncated it) | harness | medium |
| H-3 | SQL-seeding bypassed embeddings — invalidated retrieval reads | harness | high |

---

## The two that blocked the run (both fixed before it ran)

Found not by review but by **trying to onboard ChainSonar as a customer would**
— provision an org, mint keys for its developers, point them at Brainiac.

### B-1 — `lib:*` / `kb:*` scopes were unmintable · **bug · high · FIXED**

`auth::SCOPES` listed only `read | write | admin`. Every Library and KB
endpoint built in LB1–LB4 enforces a `lib:*` / `kb:*` scope — so
`POST /v1/tokens {scopes:["lib:read"]}` was rejected as out-of-vocabulary, and
**the only key that could reach the Library over REST was `admin`.** The entire
per-scope design ("an agent's token can read the library but never decree a
rule") collapsed into "give the agent the keys to the building."

Why every layer's own tests missed it: they mint through
`brainiac_store::tokens::create` directly, bypassing the endpoint's validation.
**Fix:** `auth::SCOPES` now lists every enforced scope. **Regression:**
`library_pg::the_token_endpoint_can_mint_every_enforced_scope` mints each scope
through the real endpoint and proves a `lib:read` key reaches the library.

### B-2 — the MCP surface rejected managed keys · **bug · high · FIXED**

`McpState::from_env` resolved the MCP token against the **env map only**. The
`brk_` device key that `/signup` mints "for the local device (the MCP agent)"
therefore failed with *"does not resolve to a principal"* — the free-tier
onboarding→agent loop was **broken end to end**. **Fix:** MCP now resolves via
`resolve_bearer` (env → `api_tokens`) and, because accepting keys without
gating would be a regression, **gates each tool by the token's scope** (the MCP
mirror of REST's `auth_of`). **Regression:**
`library_pg::mcp_managed_key_resolves_and_its_scopes_gate_the_tools`.

*(The harness routes through `bx`/REST, so B-2 did not block it — but it is the
real customer's onboarding path, so it was fixed anyway.)*

---

## What the eight agents surfaced

### F-1 — the memory→standard evidence chain is unusable in a session · **design gap · high · FIXED**

> **Fixed (2026-07-16), together with F-2.** `GET /v1/sources/{id}` now returns
> `results.memory_ids` — the ids of the memories the source produced — and MCP
> gains a `memory_status` tool reporting the same (`extracted` + memory ids,
> "not found" for foreign sources under RLS). The flow is: `memory_add` →
> poll until processed → cite the returned id as `evidence_memory_id`.
> `memory_add`'s response and tool description now say exactly that. `bx` gains
> `source-status`. Regressions:
> `library_pg::source_status_returns_the_memory_ids_it_produced` (REST, incl.
> the cross-org 404) and the `memory_status` round-trip in
> `mcp_pg::mcp_handshake_and_tools` (un-extracted → extracted → not-found).


The single most-reported finding: **three of four independent scanners hit it
without prompting.** `standard-propose --evidence <id>` wants the id of a memory
to cite. But `memory-add` returns `{source_id, job_id}` — an *async ingestion
job*, not a memory id — and passing that `source_id` to `--evidence` returns
`404 "cited evidence memory not found"`. A `memory-search` run seconds after the
add still returns `hits: []`.

So the exact flow the product invites — "memories become the evidence a standard
cites" — **cannot be completed in one working pass.** Every scanner fell back to
inlining evidence into prose and proposing standards with no provenance. Which
then meant the maintainer could only adopt them **by decree** (the gate's 409 →
`decree:true` path), because a rule with no provenance cannot be adopted plainly.
The whole "no unattributed rules" guarantee degraded to "everything is a decree"
— not because the gate failed, but because the *intake* couldn't attach
evidence.

**Opportunity:** `memory-add` should return the eventual `memory_id` (or a
handle that resolves to one), or `standard-propose --evidence` should accept a
`source_id` and resolve it. Either closes the chain.

### F-2 — a contribution gives no success/failure signal · **design gap · high · FIXED**

> **Fixed (2026-07-16)** — same change as F-1: the source-status surfaces (REST
> + the new MCP `memory_status`) distinguish *queued / retrying / processed /
> failed* and report the produced memory ids, so "landed", "still queued", and
> "extraction produced nothing" are now three visibly different outcomes
> instead of one silence.


`memory-add` returns a `job_id` and a success exit, and that is the last an
agent ever hears. Under the mock provider the memory silently never
materialized (mock extracts nothing); under real qwen it materialized ~64% of
the time (see F-3). **The agent cannot tell the difference between "landed",
"queued", and "silently dropped."** For a store whose whole value is that
contributions travel, a write with no confirmation is a trust hole. A
`memory-add` that returns `status: queued` plus a way to poll, or a synchronous
"extracted N candidates," would fix it.

### F-3 — extraction hard-fails on distilled facts · **bug · high · FIXED**

> **Fixed (2026-07-16).** A `manual` source (a `memory_add` — one pre-distilled
> statement) now takes a deterministic **verbatim** path instead of the model:
> the statement *is* the memory, so it becomes one raw memory with **zero LLM
> calls and zero parse risk**, then flows through the same firewall / dedup /
> resolve machinery as an LLM extraction. Transcripts and docs still go to the
> model. A new `brainiac-pipeline::manual` module owns the encode/decode wire
> format in one place (round-trip tested) so the MCP encoder and the pipeline
> decoder cannot drift apart. Regression: `pipeline_pg::manual_source_ingests_
> verbatim_without_the_model` proves it with a provider that **panics if
> called** — a green run is proof the model was never touched — asserting the
> statement lands byte-for-byte with the author's kind, `llm_calls = 0`,
> `parse_failures = 0`.

Real qwen-max extraction over the 14 seeded facts: **9 extracted, 5 failed
outright** — `"extractor output unparseable after 2 repairs: 'memories' was not
an array of the expected shape"` — a **36% hard-failure rate**, and every
success needed a repair pass. The cause is structural: the extractor prompt is
built for **session transcripts**, but an agent's `memory-add` sends a **single
pre-distilled fact** — the one shape the extractor is least robust on. This is
the exact input the product most invites an agent to send, and it is where
extraction is weakest. (The mock provider extracts *nothing* from any real
content, which is fine for fixture tests but means **`--mock` cannot exercise
the memory module at all** — a thing worth documenting on the flag.)

### F-4 — no skill-authoring path, and memory has no kind/tag · **design gap · medium · FIXED**

> **Fixed (2026-07-16).** Two closures:
> - **`skill_propose`** — the authoring counterpart to `standard_propose`, on
>   both MCP and REST (`POST /v1/library/skills/propose`, scope `lib:propose`).
>   An agent's skill lands as a DRAFT + an unpublished version; a named human
>   publishes it, and `skill_fetch` refuses it until they do — the same gate
>   publishing already enforced. Rate-limited per author (a new `skills.
>   proposed_by` column, migration 0032) and deduped by slug, exactly like the
>   standard channel. The store's `propose_skill` owns both guards so REST and
>   MCP cannot drift.
> - **`memory_add` kind + entities over REST** — the fields the MCP tool already
>   had. They fold into the stored `manual` source through the ONE F-3 wire-format
>   owner, so a distilled runbook records under its real kind with no extractor
>   guessing. ("Tag" as a distinct field was not added — entities ARE the tag
>   mechanism, and inventing a parallel one would be scope for its own sake.)

There is no agent tool to author a skill (LB4 gives agents `standard_propose`
but no `skill_propose`), and `memory-add` accepts only `--content` — no
`--kind` / `--tag`. So a scanner that finds a genuine runbook ("how to add a
schema column," "how to add a data provider") can only smuggle it in as a
sentence and hope the extractor guesses `howto`. Three scanners flagged this
independently. The Library has a skills half; agents have no way to feed it.

### F-5 — REST/MCP surface asymmetries · **design gap · low · FIXED**

> **Fixed (2026-07-16), the two that cost an agent a round trip; the other two
> were already right or are right as they stand:**
> - **usage by slug** — `POST /v1/library/usage` now accepts `artifact_slug` as
>   an alternative to `artifact_id`, matching MCP `skill_report_usage`. An agent
>   holding the slug `standards_for`/`skill_fetch` handed it no longer has to
>   resolve a uuid it never saw. (Exactly one of id/slug; a slug that resolves
>   to nothing is a clean `recorded:false`, not a 500.)
> - **doc search over REST** — `GET /v1/docs?q=` now runs the same lexical
>   search the MCP `doc_search` had (title/slug/body, RLS-scoped). REST was
>   list-only.
> - **list-what-was-proposed** — already served: REST `GET /v1/library/standards
>   ?lifecycle=proposed` exists, and the propose path itself dedupes and reports
>   the collision, so a proposer learns the duplicate whether or not it lists
>   first. The MCP `standards_for` stays adopted-only ON PURPOSE — a proposal
>   must never reach an agent as if it were policy.
> - **envelope shape** — left as-is by design: `memory_add`'s `{source_id,
>   job_id}` is an async ingest RECEIPT and `standard_propose`'s `{outcome,
>   standard_id, lifecycle}` is a synchronous decision; forcing one envelope
>   over two different operations would obscure that difference, not clarify it.

Real inconsistencies an agent trips on: `standards-for` over REST serves
`adopted` only, so a proposer **cannot list what has been proposed** to check
for duplicates before proposing (the dedup is server-side, but blind to the
agent); `doc-search` is MCP-only (REST `/v1/docs` is list-only); `/v1/library/
usage` takes an artifact **UUID** while MCP `skill_report_usage` takes a
**slug**; and `memory-add` → `{source_id, job_id}` vs `standard-propose` →
`{outcome, standard_id, lifecycle}` share no envelope, so each response must be
special-cased.

### F-6 — the honesty rule held under a leading prompt · **positive · notable**

The scan brief asked scanners to propose a "200-LOC file guideline." One scanner
**refused**: no doc states one and the code does not follow it (8 of 15 core
files exceed 200 lines), so proposing it would be inventing a rule — exactly
what the honesty rule forbids. It reported the *ask itself* as suspect, and
separately caught that ChainSonar's **README describes a different app**
(a nonexistent Ethereum ERC-20 desk) than the code implements. The product's
central discipline — never assert what the source does not hold — survived a
prompt actively pushing against it.

### F-7 — the dedup collapsed cross-agent duplicates · **positive · notable**

Four scanners working blind (no shared context, isolated worktrees) proposed
overlapping standards — several variants of "unknowns render neutral,"
"read-only paper-only," "throttle is a typed error." LB4's slug/statement dedup
collapsed the duplicates on the server: **11 proposals from four agents became
8 stored candidates**, with the later proposers told the idea already existed.
The mechanism designed for exactly this — many agents finding the same thing
making one candidate — worked under real concurrent multi-agent load.

### F-9 — the standards-page scaffold is unreachable without a graph · **bug · high · FIXED**

> **Fixed (2026-07-16).** The sweep's org selection was extracted from `main.rs`
> into the tested `brainiac_pipeline::compose::orgs_with_compose_work`, which now
> also `UNION`s `SELECT DISTINCT org_id FROM standards WHERE lifecycle =
> 'adopted'`. Regression:
> `standards_page_pg::a_library_first_org_is_visited_by_the_compose_sweep` seeds
> an org with three adopted rules and no graph/pages and asserts the sweep visits
> it and scaffolds its page. Clippy + tests green.


The knowledge-base module got **zero calls** across all eight agents — and it
turns out there was nothing there to call. With four adopted `typescript`
standards (threshold is three), the L8 standards-page should have scaffolded
itself. It never did. The cause: the compose sweep that runs the scaffold
selects the orgs to visit with

```sql
SELECT DISTINCT org_id FROM documents
UNION
SELECT DISTINCT org_id FROM canonical_entities
```

The ChainSonar org has no documents (nothing scaffolded yet — chicken and egg)
and no canonical entities (the seeded memories never went through entity
resolution). So **the sweep never visits the org, and the standards-page
scaffold — whose own trigger, adopted-rules ≥ 3, is satisfied — never runs.**
The Library's KB projection silently depends on the Memory graph being
populated, an unstated coupling. The org-iteration query should also
`UNION SELECT DISTINCT org_id FROM standards WHERE lifecycle = 'adopted'`
(and, for digests, any org with recently-changed memories), or the standards
page should be scaffolded on adoption rather than waiting for a sweep that may
never arrive. This is why the "standards render as a KB page" feature (a
follow-up shipped just this week) produced nothing under a realistic org that
built its Library before its graph.

*Compounding adoption gap:* even had a page existed, the developers did not
reach for `doc-search`/`doc-get` — they searched memory and standards directly.
Agents treat the KB as optional when the atoms (memories, rules) are available;
worth knowing for how the KB earns its place.

### F-8 — `bx` shell-quoting taxes code examples · **harness · medium**

`bx` is deliberately dumb, so the agent owns all shell quoting. TypeScript
snippets contain backticks and `$`, precisely what breaks a double-quoted arg,
and apostrophes in prose break single-quoted args — scanners had to reword real
code into prose for `--examples` and write contraction-free English, losing
fidelity. For a *code*-oriented tool this is a real authoring tax. A
`--examples-file` / `--content-file` flag would remove it. *(This is a flaw in
the harness's `bx`, not the product — but it distorts what agents can contribute
and is worth carrying into any real CLI.)*

---

## The gate worked exactly as designed

Two moments worth stating plainly, because they are the product's core claims
holding under live fire:

- **Evidence-free adoption was refused.** Adopting the four mined standards
  returned `409` until the maintainer signed a decree — the "no unattributed
  rules" invariant, enforced by the database, not by convention. The 409 →
  `decree:true` path is the humane version of that refusal, and it is what a
  maintainer actually did.
- **Scopes gated everything.** Developer keys carry `lib:propose` but not
  `lib:publish`: they proposed freely and could not adopt. The maintainer key
  adopted. No key could reach another org's data (all five are org-scoped in
  `api_tokens`, verified).

---

## Harness findings

- **H-1 — a field test must own its database.** Sharing `:5433/brainiac` with
  the `_pg` suite let a mid-run `cargo test` truncate the org's data (see
  README-run.md). Adopt `uat/`'s run-scoped-DB guard. **medium.**
- **H-2 — a shared scanner identity throttles proposals.** Eight scanners on one
  `scan` user hit the per-author proposal budget; the run raised
  `BRAINIAC_LIB_PROPOSE_PER_HOUR` to 50 to compensate. Distinct identities per
  agent (as the developers have) is the right shape. **low.**
- **H-3 — seeding via SQL bypasses embedding generation.** After H-1 wiped the
  corpus, I rebuilt it with a direct `INSERT INTO memories`, which does not
  create `memory_embeddings` rows — so the developers searched a corpus with no
  vectors and retrieval fell to lexical-only. It invalidated every
  retrieval-quality read of the run (see the developer friction above). The
  correct reseed path is through the ingest pipeline (which extracts *and*
  embeds), or a `reembed` pass after the insert. The lesson generalizes: **the
  harness must exercise the product's own write path, or it measures an artifact
  of its shortcut.** The two developer memories that *did* go through the real
  path were embedded correctly — the shortcut was the whole problem. **high.**

---

## Developer phase — did the knowledge travel, and did it change the work?

The question the whole exercise exists to answer. Each developer worked in an
isolated worktree on its own key; Brainiac was the only channel between them and
the scan's knowledge. **It travelled, and in two cases it changed a decision.**

### dev-c (Opus) — scale the UI · the clearest proof of value

Built `useVirtualRows` (111 LOC, dependency-free DOM windowing) across four
table surfaces — constant ~28 rendered rows at any total (89% fewer at 250 rows,
97% at 1000), build/tsc/eslint green, **zero added chain calls**.

That last clause is the point, and it came from Brainiac. dev-c searched the org
before choosing an approach and found two memories the scanners had seeded — the
providers-split fact and the throttle-is-a-typed-error rule. From them it
concluded the rate-limited path is the *pipeline's chain fetch*, **not** the
local DuckDB reads the tables consume — and therefore scaled by *rendering fewer
rows from data already in memory* rather than paginating the API, which "adds
zero upstream fetches, so the existing backpressure is untouched." The brief's
one hard constraint (don't defeat the rate-limit backpressure) was satisfied
**because the org's memory told it where the backpressure actually lives.** It
also used the README-drift memory to avoid reasoning about files that don't
exist, and fed all three back as `helpful`. Contributed back: a virtualization
standard and a memory recording the DuckDB-vs-chain distinction.

### dev-b (Sonnet) — refactor · knowledge changed the split

Split three 300+ LOC files into 200-LOC-compliant modules (build passes, no
behaviour change). Two Brainiac finds shaped the work: the DuckDB
single-connection pitfall memory made it route the new `backtest-engine.ts`
through the shared `lib/db.ts` rather than opening its own connection; and,
finding no file-splitting standard on record, it reused the codebase's own
existing `container + -shared + -subcomponent` precedent and **proposed a
standard codifying it**, citing three instances. Fed the DuckDB memory back as
`helpful`.

### dev-a (Opus) — direct chain listening · the hardest task, and the org caught its bugs

The most substantial build: a read-only, checkpointed viem log-ingester
(`lib/chain.ts` 180, `swapingest.ts` 190, `swapdecode.ts` 57, `pairmeta.ts`
137, `swaps.ts` 102, an API route, a proof harness, and a design doc) that
backfills a local DuckDB `swaps` store from public Polygon RPC in cap-sized
`eth_getLogs` chunks. **Proven live:** 97 real swaps, a 149-block local window =
3× the 50-block per-call cap, then caught up — throttles surfacing as
`paused / rate_limited` with the cursor preserved. tsc + eslint green.

It reached for six pieces of org knowledge, and each one earned its place:

- The **README-drift memory** — the third developer it saved. dev-a's brief,
  like the scanners', was written from ChainSonar's misleading README; the
  memory redirected it to the real Polygon pipeline "before I wasted time
  hunting phantom files."
- The **`throttle-is-a-typed-error` and `fail-honest-preserve-the-cursor`
  standards drove its entire error model — and caught two real bugs its own
  proof exposed** (a 402 on `getBlockNumber` and a throttle during pair
  resolution, both escaping the step). The org's ratified rules found defects
  in the developer's fresh code. That is the Library doing the job it exists
  for.
- The DuckDB-conventions memory shaped it to reuse `lib/db.ts`; the
  provider-split and Dune-budget memories confirmed the whole motivation (the
  live-watch is Dune-bound, so a local store is the credit saving).

Contributed back: three memories (the new path, an empirically-measured survey
of four public Polygon RPCs' wildly different `getLogs` caps, a BigInt typecheck
gotcha) and a standard, "Chunked checkpointed chain backfill — never read the
chain in one unbounded getLogs" (`9bcc97e5`).

### The travel, stated plainly

Four scanners captured what they learned reading ChainSonar. Three developers,
who never saw the scanners or each other, reached into that store unprompted and
pulled out exactly the constraints they needed — and in dev-c's case it decided
the architecture, in dev-b's it decided the module boundaries. The same
README-drift memory saved two different developers from the same dead end. That
is the product's entire thesis — one agent's learning reaching another through a
governed store — happening on a real codebase, measured, not asserted.

### The friction the developers hit

- **`memory-feedback --helpful true` → 422** (both dev-c and, earlier, another
  attempt): the flag is `--verdict helpful|wrong|outdated`, the brief's `bx`
  examples didn't show feedback, and the error (`missing field verdict`) is a
  raw JSON-deserialization message, not "did you mean --verdict". Discoverable
  only by grepping `bx.mjs`. **design gap · low** (partly `bx`, partly the
  server's unfriendly 422).
- **Memory retrieval returned nothing for well-formed specific queries** — the
  most-reported developer finding: dev-a and dev-c *independently* flagged that
  targeted queries ("provider rate limit swap window 100 blocks", "fetch
  batching rate limit backpressure") returned zero hits, while a bare
  "ChainSonar" surfaced everything at scores ~0.03. dev-a: "I nearly made
  decisions blind because my well-formed questions retrieved nothing."

  **This is mostly my fault, and honesty requires saying so — see H-3.** I
  seeded the twelve-memory corpus with a direct SQL `INSERT` after the
  test-suite truncation, which **bypassed embedding generation**: 12 of the 18
  memories in the org have no embedding row, so vector search was *dead* against
  the entire corpus the developers queried and only lexical BM25 ran — which is
  exactly the phrasing-sensitive, ~0.03-scoring, term-overlap behaviour both
  developers saw. The developers' *own* contributed memories (added through the
  real ingest path) got embeddings; my seeded ones did not. **The instrument was
  miscalibrated, so this run cannot be read as evidence about Brainiac's
  retrieval quality.** The residual product-true lesson is milder and still
  worth keeping: on a small corpus an agent should broaden a query that returns
  nothing before concluding the org is ignorant — and the tool could hint at
  that. **harness (H-3) · high — it invalidated the retrieval reads.**

## Metrics (from the two logged streams)

`node load/chainsonar/report.mjs` → `runs/2026-07-16/metrics.json`. Final:

- **106 Brainiac calls** across the four bx identities (the maintainer triaged
  over curl, not bx, so it is absent here — a small harness gap).
- **Reach:** scan 70 · dev-a 15 · dev-c 11 · dev-b 10. Every developer used
  Brainiac unprompted and repeatedly.
- **Module coverage:** memory 69 · library 37 · **knowledge base 0** — the KB
  was untouched, because F-9 left it empty and agents reach for atoms (memories,
  rules) over pages when the atoms are there.
- **Outcomes:** 77 ok · 23 empty (22%) · **6 error (6%)**. Every error is one of
  the findings above: 4× the F-1 evidence 404, 1× the feedback-flag 422, 1× the
  500-char 400. "Empty" is a first-class outcome, not a failure — though H-3
  inflated it by killing vector search.
- **Reads vs writes:** 31 / 75 — write-skewed only because the scan phase was
  70 proposals/adds; the developer phase alone is read-led (search before act).

All three developers left running code in their worktrees (`field/dev-{a,b,c}`),
each proven green in isolation; none was merged (the harness measures the work
and the traffic, not a release).
