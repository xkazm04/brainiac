---
name: uat
description: Simulated User Acceptance Testing for Brainiac, run as a controlled trial inside a simulated multi-stack engineering company rather than a feature walkthrough. Characters are developers (Rust/TS/Python/infra) doing real coding work; every journey runs in three arms — cold agent, Claude's native local memory (the free baseline), and Brainiac — and the verdict is the DELTA, which is allowed to come out negative. Two chronological certification levels: L1 theoretical (over a code-derived model of what an agent actually receives, cheap + mass-parallel) then L2 empirical (live server + workers + real agent sessions against the Meridian company). Ships a Harm Ledger alongside findings. Per-run specifics live in the repo's uat/ overlay. Invoke with `/uat init|update|run|promote [args]`.
---

# Simulated UAT — a controlled trial of org memory (Brainiac)

This is **evaluative** testing (is a company *better off* with this?), not **verification** testing (does the code do what we told it to?). Brainiac already has the Meridian eval harness — NDCG@10, contradiction detection, entity-merge F1, RLS-leak-zero gates — and those answer "does retrieval work." They are structurally blind to the only question that decides this product's fate:

> **NDCG@10 of 0.876 does not mean a single pull request got better.** A memory system can retrieve beautifully and still lose to a 40-line `CLAUDE.md` that costs nothing, ships in git, and never needs a maintainer to approve anything.

So the acceptance question is never "does `memory_search` work." It is: **for this developer, on this task, did knowing what the org knows change the work for the better — by more than the cheap alternative would have, and by more than the harm it caused?**

Method backbone: **counterfactual A/B/C trial** (the arms below) + **cognitive walkthrough** (task-based, per-step questions) + **jobs-to-be-done** acceptance, judged blind against a fixture answer key. See `uat/rubric.md` for the operational lens.

> Terminology: a **Character** is a durable, repo-committed *developer* — a human with a stack, a repo, a seniority, an agent setup, and a memory practice they already have. The **Company** is the simulated org they all work in (an extension of the Meridian fixture). A **Relay** is a journey that spans developers *and time* — the only shape where a shared store can beat a local file.

> Real model calls, a real Postgres, real workers and real agent sessions are the point at L2. "Good retrieval feeding a decision that didn't need it" and "confident retrieval feeding a decision it corrupted" are both invisible to an assertion. This is a **deliberate periodic pass, never a per-commit CI gate.** The two-level design keeps it affordable.

## The three arms (this is the whole design)

Every journey is run in **three arms**, and the finding is the *difference between them*:

| Arm | What the agent has | What it costs the company |
|---|---|---|
| **A — Cold** | The repo. Nothing else. | Zero. The floor. |
| **B — Baseline** | **Claude's native memory stack, built the way a competent senior with a weekend would build it** — *not* "a CLAUDE.md." The full free stack: the four concatenated `CLAUDE.md` scopes (managed / user / project / local), **`.claude/rules/` with `paths:` glob frontmatter** (this is free, path-scoped, just-in-time rule retrieval — omitting it is the single easiest way to strawman arm B), subdirectory `CLAUDE.md` files, auto-memory left **on**, hooks for anything that must be *enforced*, and the **symlinked shared org-rules directory** (the free cross-repo mechanism a good senior finds). Plus a **maintenance budget**: arm B's owner may update these files between journeys, exactly as a real team does. | Near zero. No service, no reviewer, no tokens. |
| **C — Brainiac** | The MCP surface against a live server: `memory_context` at session start, `memory_search` / `entity_lookup` mid-task, `memory_add` / `knowledge_propose` on the way out. Governed, provenance-carrying, org-wide. | A service, a Postgres, BYOM tokens on every extract/contradict/promote, **and a maintainer's review time.** |

**The verdict is `C − B`, not `C − A`.** Beating a cold agent proves nothing; a `CLAUDE.md` beats a cold agent. Arm A exists only to detect the embarrassing case where memory makes things *worse than nothing*.

**A frozen arm B against a live arm C is a rigged fight.** Real teams edit these files. Let arm B rot only at the rate real teams let it rot — and *measure* that rot rather than assuming it.

### The primary endpoint is EFFICIENCY, not code quality (this is evidence-led, not a preference)

The published evidence is thin but it points one way, and a trial that ignores it will measure noise:

- The only controlled A/B on repo context files ([arXiv 2601.20404](https://arxiv.org/abs/2601.20404), 10 repos / 124 PRs, agents run with and without `AGENTS.md`) found **median runtime −28.6%, output tokens −16.6%, "comparable task completion behavior."** They measured wall-clock and tokens. They did **not** measure correctness.
- The one controlled memory benchmark (Sandelin, "Stompy") ran no-memory / persistent-memory / static-context-file across a real codebase: **all three scored 84–96% on code quality — memory did not improve it.** Memory delivered 22–32% cost reduction and 28–40% fewer turns on *complex* tasks, and was **counterproductive on simple ones.** It reports against interest — the author sells a memory product, and **the static context file (our arm B) beat his memory system on quality.**
- RAG-for-code ([arXiv 2503.20589](https://arxiv.org/abs/2503.20589)): retrieved "similar code" often injects noise and **degrades results by up to 15%.** Bad retrieval is not neutral; it is harmful.

So: **primary endpoint = turns, tokens, wall-clock, exploration reads (files opened before the agent knew what to do). Quality is a guardrail — the bar is "no worse."** A cost win is a perfectly respectable business case and it is where the effect actually lives.

**If you insist on a quality endpoint, two conditions are mandatory or the result is meaningless:** the task must have a quality ceiling below ~90% (an agent that can just read the codebase will ace it either way), and **the discriminating knowledge must not be recoverable from the repo.** Brainiac can only beat a well-maintained `CLAUDE.md` on knowledge that is *not in the tree*: cross-repo contracts, *why* a decision was made, incident history, the approach someone tried and abandoned six months ago. Design tasks around that, or arm B ties you and the run tells you nothing.

### The sharpest hypothesis: within-session decay

A factorial study of 1,650 Claude Code sessions ([arXiv 2605.10039](https://arxiv.org/abs/2605.10039), 16,050 function-level observations) varied `CLAUDE.md` size, instruction position, file architecture, and cross-file contradictions. **None of the four structural variables produced a detectable effect** (Bayes factors 0.05–0.10 — genuine absence, not underpowering). What *did* show up, robustly: **within-session compliance decay — each additional function the agent generates carries ~5.6% lower odds of following the file (OR = 0.944).** Practitioners corroborate it from the other side: the file is followed "reliably at the beginning and end of the conversation, but ignored during the middle where the real work is being done."

Two consequences, both load-bearing:

1. **You may not weaken arm B by bloating it.** File length is not the demonstrated failure mode; session length is. (Anthropic's own docs advise "under 200 lines" — that guidance and this experiment disagree, and this skill records the disagreement rather than quietly picking the flattering side.)
2. **Brainiac's real competitive surface is the decay curve**: just-in-time retrieval *mid-session*, when the front-loaded blob is already being ignored. That is a far sharper and more honest thesis than "our store is better than their file," and journeys should be built to test it — put the decisive knowledge need in the *middle* of a long task, not at the start.

### Pre-registered hypotheses (write these down before running, so the run can falsify them)

**H-eff:** arm C completes the task in fewer turns/tokens/exploration-reads than arm B. *(Where the literature says the effect is.)*
**H-qual:** arm C is no worse than arm B on correctness. *(A guardrail, not a win condition.)*
**H-decay:** arm C's advantage grows with task length. *(The mechanism arm B has no answer for.)*
**H-cross:** arm C wins where arm B structurally cannot reach — another team's repo, another team's incident.
**H-retract:** when a fact is reversed, arm C serves the new truth and arm B serves the old one forever.
**H-null:** on a single-team task whose knowledge is already in `CLAUDE.md`, arm C shows **no delta and costs more.** *(This one should come out TRUE. If it doesn't, suspect the harness.)*

**`C − B` is allowed to be negative, and a run that never returns a negative delta is not being run honestly.** Brainiac earns its existence only where B structurally cannot go:

- knowledge that **crosses a team boundary** (a `CLAUDE.md` in the payments repo cannot help the data team),
- knowledge that **arrived after the file was written** and nobody remembered to update it,
- knowledge that must be **retracted** (a decision was reversed — a `CLAUDE.md` line just sits there being wrong forever),
- knowledge that must be **permission-scoped** (a contractor must not see it),
- knowledge whose **provenance decides whether you act on it** ("who said this, from which session, when, and is it still true?").

Journeys that don't sit in one of those five gaps are journeys Brainiac will lose, and the run should say so plainly. **That is a finding, not a failure of the test.**

## The Harm Ledger (co-equal with findings)

A shared, governed, agent-facing memory can cause harm a local file cannot. Each run produces a **Harm Ledger** (`runs/<id>/harm.md`) — every class below is either **observed with evidence**, **probed and not observed**, or **not probed** (say which; silence is not absence):

| # | Harm | How this run detects it |
|---|---|---|
| **H1** | **Poisoning** — a wrong (or wrong-for-this-stack) memory is retrieved and the agent *acts on it* | **Decoy probe:** plant a plausible-but-wrong memory. Arm C can swallow it; arm B never sees it. **Note the shipped default makes this worse than the architecture implies: `memory_search` excludes only `rejected` — it serves `raw`, unreviewed, pipeline-extracted memories alongside canonical ones (only `memory_context` has a Canonical floor). The governance step an agent's main search tool actually enforces is: none.** |
| **H2** | **Stale authority** — `canonical` makes an agent trust a superseded fact *more* than it would trust a rumor | Query across a known supersession chain (`fixtures/v1/temporal/asof.yaml` ships six, incl. the cross-team `std-retry` one); assert the agent gets the *current* truth, not the confident dead one. **Deprecation is enforced temporally (`valid_to`), not by status — a deprecated row with a NULL `valid_to` is still served.** |
| **H3** | **Cross-stack noise** — a Rust dev's bundle fills with Python knowledge; token budget spent, attention spent | Measure **utilization**: of the memories injected, how many did the agent *cite or use*? Low utilization at high token cost is a cost, not a neutral. ⚠ **Not measurable on `fixtures/v1` — the corpus is entirely stack-agnostic prose (zero language-specific content). Report `not probed` until the Company ships stack fixtures.** |
| **H4** | **Leak** — team-private knowledge, or a secret lifted out of a raw transcript, reaches the wrong principal | Drive as the contractor / wrong-team Character against the 15 `retrieval/leak.yaml` cases (incl. the four *private-vs-lead* traps: a maintainer must **not** read a member's private memory). **Then the nastier half: there is NO redaction anywhere in the pipeline, and `memory_provenance` returns a 500-char verbatim excerpt of the raw transcript to anyone whose RLS lets them see the memory. RLS is the only thing between a pasted API key and an agent.** |
| **H5** | **Governance tax → queue abandonment** — the review queue is the product's heart and the first thing a real org abandons | The abandonment literature is unambiguous, and Brainiac is *structurally exposed*: automating capture solves the friction problem (H6) by **industrializing the production of candidate memories**, which makes the human review queue the bottleneck. Instrument the actual failure sequence: **queue depth over time · time-to-review · rubber-stamp rate (approve latency — a reviewer clearing backlog in 3s/item is not reviewing) · retraction rate (was any promoted memory ever walked back?).** Their own SLO: *median promotion review < 48h or the flywheel dies* (ARCHITECTURE §7). **A system with no retraction path rots exactly like Confluence** — and documentation rots *silently*: nothing goes red. |
| **H6** | **Capture friction** — if writing memories is a chore, nobody does it and the corpus is empty by month two | Who actually called `memory_add`? Was it free (session ingest) or a chore? A journey where the dev must *remember to write it down* is already losing. This is the one killer Brainiac's auto-capture genuinely answers — give it the credit, then charge it for H5. |
| **H7** | **Redundancy** — Brainiac serves what `CLAUDE.md` already said | Cost with no delta. The most likely outcome for single-team work, and `H-null` predicts it. Count it, don't bury it. |
| **H8** | **False confidence** — the agent restates a memory as settled fact and the dev cannot check it | Read the agent's own output: did it attribute? Could the dev verify? **The shipped `memory_context` payload carries kind, content, id, a coarse `via <actor>` tag and a contradiction warning — and NOT: status, confidence, validity window, *when*, the originating human, or a session id (there is no session id in the system at all). "Who said this and is it still true" is the provenance gap Brainiac claims to own, and today the payload cannot answer it.** |

The Harm Ledger is not a complaints list — it is the debit column. **Net value = (decision-delta over B) − (harm) − (tax).** A journey can pass every functional check and still be net-negative, and the run must be willing to say that.

## Two-level certification (chronological)

**Level 1 — Theoretical (static, code-grounded).** Build a model of **what an agent actually receives**: for the journey's task, what `memory_context` / `memory_search` would return — the retrieval path (`crates/brainiac-store` hybrid + RRF + graph expansion), the status/visibility filter, the token budget, whether provenance travels with the payload, and what the MCP tool descriptions in `crates/brainiac-server/src/mcp.rs` tell the agent to do with it. Then walk the task in-Character over that payload and ask the only question that matters: **would this payload have changed my decision, and would `CLAUDE.md` have told me the same thing for free?** No live server. **Pass → L1 ("would plausibly change the work").** Cheap and **mass-parallel** — one subagent per `character × journey`; a 12-Character sweep costs one agent's wall-clock.

**Level 2 — Empirical (live).** Only for journeys that earned L1. Stand up the real stack, seed the Company, and run **real agent sessions doing real coding tasks** in all three arms, then judge the outputs blind. This is where retrieval quality is finally converted into work quality — or fails to be. **Pass → L2 ("confirmed live, with a signed delta").**

**L1's structural blind spot is the retrieval-to-behavior gap.** L1 reads the store and reasons about what *could* be returned. Keep **four** verdicts distinct and never collapse them:

> **retrievable ≠ retrieved ≠ used ≠ correct.**

L1 can honestly speak only to *retrievable* (the memory exists, RLS permits it, the query would plausibly rank it). Whether the hybrid retriever actually surfaced it in the top-k, whether the agent read past the first two items, and whether acting on it produced better code — those are L2's, and only L2's. An L1 finding that says "Brainiac knows this" has proven nothing about the product. This is the single most common way an org-memory demo lies to its owner; do not let this skill do it.

## Characters carry their own judgement (the consistency harness)

Two runs of the same Character must apply the same lens — judgement is **externalized into explicit scored criteria in the Character file**, not re-improvised. Beyond stack / JTBD / pet-peeves, every Character declares:

- **Current memory practice (the B arm, in their words).** What is *already* in their repo's `CLAUDE.md`, what they keep in `~/.claude/memory`, what they'd just ask a colleague in Slack. **This is the comparator, and it must be written honestly and generously** — a strawman baseline invalidates the whole run. If a competent senior would have written that line in `CLAUDE.md`, it goes in arm B.
- **Decision-delta bar.** What would have to be true for retrieved knowledge to actually change what they type? A dev does not re-architect because a memory said so. Be concrete: "it would have to name my service and cite the incident."
- **Trust bar.** What provenance do they need before acting without verifying? (A staff engineer wants the session and the date; a new joiner will believe anything — which makes them the *most* dangerous Character to poison, and therefore the most valuable H1 probe.)
- **Toil tolerance.** Tokens, latency, interruptions, and *review duty*. A maintainer who must approve 30 promotions a week has a hard limit; find it.
- **Scored acceptance criteria** — a short explicit pass/fail list, applied identically every run. This is the harness that lets deltas be compared across runs.

These join the rubric's five (completion, effort, clarity, trust, missing-pieces).

## Portable engine vs. per-run overlay

**This skill is the engine.** Everything that varies per run lives in the repo's **`uat/` overlay**.

```
uat/
  README.md            # what this is, how to run, the Character template
  company.md           # THE simulated org: teams × stacks × repos × maintainers + the sprint calendar
  characters/*.md      # durable developers (stack, repo, JTBD, CURRENT MEMORY PRACTICE = their arm B,
                       #   decision-delta bar, trust bar, toil tolerance, scored criteria, voice)
  journeys/*.md        # goals + user-POV definition-of-done; each declares which of the FIVE GAPS it tests
                       #   (cross-team | after-the-file | retraction | permission | provenance) — or none, honestly
  relays/*.md          # multi-developer, time-ordered chains (the shape only a shared store can win)
  baseline.md          # arm B, verbatim: the CLAUDE.md files a competent team would have. Version it; it is evidence.
  decoys.md            # planted wrong-but-plausible memories for the H1 poisoning probe + expected agent behavior
  rubric.md            # 8 dimensions + severity + finding types + walkthrough questions + the blind-judge protocol
  env.md               # how to reach a known start state: compose, migrate, seed Meridian, mint per-Character tokens
  accepted-gaps.md     # known-and-accepted baseline (won't re-surface)
  driver/seed.sh       # load fixtures/v1 + extend to the Company; mint one brk_ token per Character
  driver/mcp_call.sh   # ONE MCP tool call as a given Character (JSON-RPC over `brainiac mcp` stdio)
  driver/arm.md        # the exact system-prompt/tooling contract for each of arms A / B / C (fidelity matters)
  runs/<date-slug>/    # journals, findings.json, deltas.json, harm.md, report.md, SUMMARY.md
```

A finding is always:
`{ id, journey, character, arm, cert_level, type, severity, dimension, title, expected, got, evidence[], code_check, verdict, suggested_acceptance }`
- `arm`: `A | B | C | delta` (a `delta` finding is about the *comparison*, and those are the headline ones)
- `cert_level`: `L1` | `L2`
- `type`: `missing-feature | quality-gap | broken-flow | confusion | trust | **harm**`
- `dimension`: `completion | effort | clarity | trust | missing | decision-delta | governance-tax | harm`
- `severity`: `blocker | major | minor | polish`
- `evidence[]`: L1 → `file:line`; L2 → the retrieved payload JSON, the agent's diff, the DB row, the queue timing, the judge's blind score
- `code_check`: `confirmed-absent | present-but-missed | present-broken | by-design | n-a`
- `verdict`: `confirmed | refuted | uncertain` (adversarial pass)
- Optional: `net_value` (`positive | neutral | negative`), `gap` (which of the five), `l2_priority`. A finding may be a **strength**.

---

## Mode: `init`

Goal: scaffold the `uat/` overlay grounded in the codebase, the Meridian fixture, and real-world research.

1. **Map the surfaces from `context-map.json`** (19 contexts / 6 groups). The ones that matter here: *MCP Agent Surface*, *REST Memory API & Auth*, *Memory Store & Hybrid Retrieval*, *Knowledge Extraction & Resolution*, *Pipeline Worker & Ingest Queue*, *Promotion Review Queue*, *Governance Console API*. Read `crates/brainiac-server/src/mcp.rs` for the **exact** tool set an agent gets today — `memory_search`, `memory_context`, `memory_add`, `entity_lookup`, `knowledge_propose`, `memory_feedback`, `memory_provenance` — and their descriptions, because **the tool description is the product**: it is what tells the agent when to reach for org memory at all. An agent that never calls `memory_context` gets exactly zero value from a perfect store.
2. **Build the Company (`company.md`) on top of Meridian.** `fixtures/v1/org.yaml` already ships the org: 3 teams (payments, platform, data), 6 users, maintainer roles. **Extend, don't replace** — keep the ids stable so the eval fixtures still load. What the fixture lacks and the Company needs: a **stack per team** (this is a cross-tech-stack trial by requirement — e.g. payments: Rust services; platform: Go/Terraform/k8s; data: Python/dbt; plus a TS/Next frontend team to add a fourth boundary), a **repo per team**, an **outsider** (contractor with team-scoped-only access — the H4 probe), and a **sprint calendar**: an ordered list of sessions, because relays need time to pass.
3. **Confirm the run recipe → `env.md`.** L2 needs: `docker compose up -d` (PG 17 + pgvector on :5433) → `DATABASE_URL` → `cargo run -p brainiac-server -- serve` (:8600) → `cargo run -p brainiac-server -- worker` (the pipeline must actually run, or nothing gets promoted) → seed → mint a `brk_` API token per Character (`POST /v1/tokens`, or `BRAINIAC_TOKENS` env map). Preflight: `curl localhost:8600/health`. Record **which BYOM provider** the run uses — extraction/contradiction quality is provider-dependent (ARCHITECTURE §9 risk 1), and a run on `MockProvider` is a **plumbing run, not a quality run**. Say which, loudly, in the report.
4. **Research the target group (required — it keeps Characters real).** Brainiac's buyer is an engineering org; its user is a developer already running an agent. Use `WebSearch`/`WebFetch` to ground: how teams *actually* manage agent context today (`CLAUDE.md` / `.cursorrules` / rules files / MCP memory servers / RAG-over-Confluence), what those cost, why they rot, and what the honest state of the art is for the arm-B baseline. **Ground the failure modes too** — the literature on wikis, ADRs and internal docs is a literature about *abandonment*, and H5/H6 are exactly that failure re-skinned. Record deciding references in `references:`.
5. **Offer a Character count.** **4** (smoke: two stacks + a maintainer + a new joiner), **8** (standard: full stack span + maintainer + new joiner + contractor + staff/architect), **12** (thorough: adds SRE, EM, security, and a skeptic who already hates this idea). Default 8. **The roster must cross stacks** — a single-stack roster cannot detect H3 (cross-stack noise) or test the cross-team gap, which is Brainiac's single strongest claim. Always include: one **maintainer** (they pay the governance tax — H5), one **new joiner** (the strongest value case *and* the H1 poisoning victim), one **contractor** (H4), and one **skeptic** (whose scored criteria are set to "convince me it beats a text file").
6. **Write `baseline.md` — arm B — before writing any journey.** For each Character's repo, write the `CLAUDE.md` a competent senior would actually have. Do this *generously and first*, before you know what Brainiac will retrieve, so it cannot be tuned to lose. **A rigged baseline makes the entire run worthless**, and it is the single easiest way for this skill to produce a flattering lie.
7. **Draft Journeys + Relays.** Journeys are goals with a definition-of-done, not scripts; each declares which of the five gaps it tests (or `gap: none` — an honest "Brainiac should lose this one," and you want two of those as controls). Relays are the crown jewels: time-ordered, multi-Character chains. Anchor them on the fixture's real content — the `std-retry` supersession chain, the planted contradictions in `fixtures/v1/contradictions/cases.yaml`, the cross-team entity collisions in `entities/merges.yaml`, the RLS leak cases in `retrieval/leak.yaml`.
8. **Write `decoys.md`.** Plant the H1 poison: a wrong-but-plausible memory, promoted to `canonical` so it carries maximum authority. Declare the expected safe behavior. If the new joiner's agent eats it, that is a **blocker**, and it is the most important single result this skill can produce.
9. **Scaffold** `rubric.md`, `accepted-gaps.md`, `driver/*`, `.gitignore`.

Output: a summary + open env questions. Do not run journeys in `init`.

## Mode: `update`

Diff-aware refresh (`git diff`, recent commits, re-read `context-map.json`). Retrieval, extraction-prompt, promotion-policy and **MCP tool-description** changes are the ones that move UAT outcomes — a tool-description edit can change whether the agent calls the tool at all, which changes everything downstream. Refresh journeys and scored criteria for changed contexts; targeted re-research only for genuinely new capabilities. **Never silently drop a journey** — mark removed-surface journeys `retired`. **Re-check `baseline.md` for drift**: if arm B has grown stale relative to what a real team would write today, Brainiac's delta is being flattered. Report what changed and why.

## Mode: `run`

Verify a `character × journey` selection through both levels, in all three arms.
Selection: all `promotion: discovery|candidate` journeys; those named in args; `--relay <name>` for a chain; `--gap <cross-team|after-the-file|retraction|permission|provenance|none>` to scope.
Flags: `--l1` (theoretical only), `--l2` (live only), `--arms A,B,C` (default all three; dropping B is only valid for a pure harm probe), `--acceptance` (re-run frozen gates).

### Phase L1 — theoretical (mass-parallel)

**Dispatch one read-only subagent per `character × journey`** (`Explore` or `general-purpose`). Each returns structured findings + voice; the orchestrator writes all artifacts.

1. **Build the payload model.** For this Character's task, trace the actual path: `memory_context(task_hint)` in `mcp.rs` → the store's hybrid retrieval (`crates/brainiac-store`) → RRF → graph expansion → assembly (supersession dedupe, `as_of`, token budget) → what lands in the agent's context window. Cite `file:line`. Then audit it:
   - **Payload audit (L1's sweet spot):** does the assembled bundle carry **provenance** (who/which session/when/still-valid)? Is the **token budget** honest, and what gets cut when it binds? Are `deprecated`/superseded memories really excluded? Does `visibility` correctly bound this principal? Does anything in the payload actually *name this Character's service/repo/stack*, or is it generic org-wisdom that will read as noise?
   - **Baseline diff (mandatory):** put the payload side by side with this Character's `baseline.md` `CLAUDE.md`. **Mark every retrieved memory `new` / `duplicate-of-baseline` / `contradicts-baseline`.** The `duplicate` count is H7 (redundancy) and it is usually the largest bucket — report it, do not bury it.
   - **Invocation audit:** would the agent even *call* the tool here? Read the tool description as an agent would. An unused tool has zero value regardless of store quality.
2. **Walk the task in-character** over the payload — the rubric's walkthrough questions plus the Character's scored criteria (decision-delta, trust, toil).
3. **Emit L1 findings** with `arm` set, each needing live confirmation tagged `l2_priority`, and a per-journey verdict — **four states**: `L1-pass` (payload would plausibly change the work *beyond* baseline → clean to L2), `L1-conditional` (would change it, but majors present), `L1-redundant` (payload is real but arm B already had it — **Brainiac loses this journey and that is the finding**), `L1-fail` (structural gap: the knowledge cannot be retrieved at all).

### Phase L2 — empirical (live, parallel-within-phase)

Only for `L1-pass` / `L1-conditional` (or `--l2`). **Start from the L1 handoff** — the `l2_priority` list is what live time is *for*.

1. **Stand up the Company** per `env.md`: compose → migrate → serve → **worker** → `driver/seed.sh` (fixtures + Company extension + `decoys.md` + one `brk_` token per Character). Verify the pipeline actually drained (`GET /v1/queue/health`) — an undrained queue means nothing was extracted or promoted and the run is measuring an empty store.
2. **Run the sprint in order.** Each session is a **real agent session**: dispatch a subagent as the Character, give it a real coding task in its repo, and equip it per `driver/arm.md`:
   - **Arm A** — the repo, nothing else.
   - **Arm B** — the repo + that Character's `baseline.md` `CLAUDE.md` in context. Nothing else.
   - **Arm C** — the repo + Brainiac. The agent calls `driver/mcp_call.sh memory_context '{...}'` (real JSON-RPC into `brainiac mcp` with that Character's token — **the same code path a real agent uses; never hand-fake the payload**), plus `memory_search` / `entity_lookup` as it sees fit, and `memory_add` on the way out. Log every call and every payload.
   - **The three arms must not see each other.** Same task, same repo state, independent subagents.
3. **Capture the artifacts**: the diff/answer each arm produced, every retrieved payload, every tool call, latency, tokens, and — for arm C — what the session wrote back (which is next session's input; this is the flywheel, and if it doesn't turn, say so).
4. **Judge blind.** A separate judge subagent receives the arms' outputs **shuffled and unlabeled**, plus the task and the fixture's answer key (gold memories, the true state of the supersession chain, the decoy's correct rejection). It scores each on correctness, completeness, and whether it acted on knowledge it could not have derived from the repo. **It must not be told which arm is which** — an LLM told "this one used the memory system" will find it better. Only then unblind and compute `C − B`.
5. **Run the harm probes** (H1–H8 above) — the decoy, the leak, the stale chain, the utilization count, the queue timing. **A run that reports no harm has not probed for it.**
6. **Adversarial verify** every kept finding, especially the flattering ones. The refuter's standing questions: *"Would a `CLAUDE.md` line have done this for free?"* · *"Did the agent actually use the retrieved memory, or did it reach the same answer from the code and cite the memory as decoration?"* · *"Is this delta real, or is it model variance between two sessions?"* (→ multi-sample: re-run the pair 3× and take the majority; a single-sample delta is not a delta).

### Output of a run

- `runs/<id>/findings.json` · `deltas.json` (per journey × character: the three arms' scores, `C − B`, and its sign) · **`harm.md`** (the ledger — every class observed / probed-clean / not-probed) · `report.md` · `SUMMARY.md`.
- **Developer voice** (`runs/<id>/<character>--<journey>.md`): candid first-person — *did it change what I did? would I have found it anyway? do I trust it enough to act without checking? was it worth the wait and the tokens? if I'm the maintainer, is the review queue a job I'd actually keep doing in month three? would I turn it off?* Produced at both levels. Across a roster these voices form a **panel** that surfaces adoption dynamics (who evangelizes, who quietly disables the MCP server) that no delta table can.
- **Synthesis (don't skip).** A final synthesis subagent reads everything and writes `SUMMARY.md`:
  - the **delta table** (which journeys Brainiac won, tied, or **lost** to a text file),
  - the **Harm Ledger**, priced against the wins,
  - the **governance tax** in hours per accepted memory, against their own 48h review SLO,
  - the **strengths worth protecting** (as decision-useful as gaps),
  - **which gap Brainiac is actually winning on.** If the wins cluster in one of the five gaps (usually *cross-team* and *retraction*), that is not a disappointment — **it is the product's real thesis, empirically located, and everything else is scope to cut.**
  - a single **net-value verdict**: `adopt` · `adopt-with-changes` · `not-yet` · **`harmful-as-shaped`**. The last one must be a live option every run, or this skill is decoration.
- Chat reply: the verdict, the delta table headline, the sharpest harm, the sharpest developer voice.

### Trust rules

- **No finding without evidence.** L1 → `file:line`. L2 → the payload JSON, the diff, the DB row, the blind score.
- **A delta is not a delta until it survives multi-sampling.** Model variance between two agent sessions is large; a 3× majority is the floor for any headline number.
- **Never flatter the baseline's absence.** If arm B wasn't run, no delta may be claimed — report the arm-C result as *uncalibrated* and say so in the headline.
- **Never fabricate a payload.** Arm C reads what the live server actually returned, through the real MCP path, or the run is void.
- **Provider honesty.** A `MockProvider` run measures plumbing, not knowledge quality. Label every report with the provider and embedder it ran on (`--embedder qwen` vs the deterministic hashed embedder — the latter's numbers are *plumbing numbers*, PLAN.md deviation 4).
- **Don't double-count the Meridian eval.** If a journey's retrieval is already covered by `cargo run -p brainiac-server -- eval`, note the NDCG and move on — UAT's job is the part the metric can't see: *did the work get better, and at what cost.*
- **Scope honesty.** Deliberately deferred (OIDC/SCIM, Cedar, S3 transcripts, the document layer — ARCHITECTURE §9) → `scope_note`, not a defect. But **"deferred" is not a defense against a harm finding**: if the shipped shape leaks or poisons today, it leaks and poisons today.
- **Baseline suppression:** `accepted-gaps.md` suppresses known/accepted issues; append when the user accepts one.

## Mode: `promote`

Freeze a journey that reached **L2-pass with a positive, multi-sampled delta** into a low-variance acceptance gate: pin the task, the fixture state, the arm-B baseline file (**version it — a moving baseline invalidates the gate**), the decoys, and the observed `C − B`. Set `promotion: acceptance`. `/uat run --acceptance` re-runs the gates and flags **delta regression** — the alarm that matters isn't "retrieval got worse," it's "**Brainiac stopped being worth it.**" Slow; run deliberately.

---

## Driver & environment (L2 — the Brainiac-specific how-to)

Brainiac has no GUI harness and needs none: **the agent surface *is* the product surface.** Drive it exactly as an agent would.

- **Stack up:** `docker compose up -d` (PG 17 + pgvector, :5433) · `export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac` · `cargo run -p brainiac-server -- serve` (:8600) · `cargo run -p brainiac-server -- worker` **in parallel — the worker is not optional**, it is the extract→embed→resolve→contradict→promote pipeline, and without it the store never grows.
- **Identities:** one principal per Character. Either the env map (`BRAINIAC_TOKENS` → `{token: {org, user, teams, role}}`, `crates/brainiac-server/src/auth.rs`) for bootstrap, or mint scoped `brk_…` tokens via `POST /v1/tokens` — **prefer minted tokens: env tokens carry every scope and will hide exactly the H4 leak you're hunting.**
- **The agent path (arm C):** `brainiac mcp` — stdio JSON-RPC. `driver/mcp_call.sh` pipes one `tools/call` request in with `BRAINIAC_MCP_TOKEN=<character token>` and returns the raw payload. **This is real:** it is the same handler an actual Claude Code / Cursor session hits (`crates/brainiac-server/src/mcp.rs`, tested in `tests/mcp_pg.rs`). Never approximate it with a REST call and never hand-write a payload — the whole trial's fidelity rests here.
- **The governance path (maintainer Characters):** REST or the console — `GET /v1/reviews/promotions` → `POST /v1/reviews/promotions/{id}/approve|reject`, `GET /v1/reviews/contradictions` → `/resolve`. **Time these.** The maintainer's wall-clock per decision *is* H5, and it is the number that decides whether this survives contact with a real quarter.
- **Verify side-effects in Postgres, not in the response.** `psql` the `memories` / `promotions` / `contradictions` / `sources` tables: did the session's knowledge actually land, with the right `status`, `visibility`, and a `provenance_id`? A `200 OK` from `memory_add` is not evidence the org learned anything — the row after the pipeline drained is.
- **Fixtures are the Company's history.** `fixtures/v1/` is not test data here, it is *what the org already knows on day one*: 9 seed transcripts, gold memories, planted contradictions, the `std-retry` supersession chain, entity collisions across teams, RLS leak cases. Seed it, then let the sprint write on top. `brainiac-fixtures` validates referential integrity — run the loader before trusting a run.
- **Cross-stack is a fixture gap.** Meridian's transcripts are stack-agnostic prose. For a genuine cross-tech-stack trial (a Python dev retrieving a Rust team's pitfall and needing to *translate* it), the Company extension must add stack-specific sessions. Until it does, **H3 cannot be measured and the report must say `not probed`** rather than `clean`.

## Concurrency & parallel-safety (MANDATORY)

- **L1 is mass-parallel.** No server to serialize.
- **L2 is parallel *within* a sprint phase, serial *across* phases.** Unlike a desktop app, the server is multi-principal — many Characters can hold sessions at once. But sessions **write into a store the next phase reads**, and that is the entire point of a relay. So: fan out the sessions inside a phase, then **barrier: wait for the queue to drain (`GET /v1/queue/health`) and the promotions to be reviewed** before opening the next phase. A relay run that doesn't barrier is measuring a race, not a flywheel.
- **One store per run.** Two concurrent UAT runs against the same Postgres will cross-contaminate each other's memories and silently poison both. Use a per-run database (`DATABASE_URL` with a run-scoped db name) or take the lock.
- **Active-runs ledger:** at start, read `.claude/active-runs.md`; if another session holds `:8600` or the DB, surface it before proceeding, then append your entry; move it to `## Recently completed` at the end (append-only — never stage it).
- **Worktree for multi-file work.** `/uat init` and any multi-journey `run` write many files — use a `git worktree`, stage path-scoped (`git add uat/...`, never `git add -A`), commit atomically.
- **Artifact hygiene:** gitignore `uat/runs/*/captures/`. **Commit `baseline.md` and `decoys.md` — they are the evidence that the trial was fair.**

## Suggested 12-Character roster (the cross-stack company)

A starting span for the **thorough** tier. Each binds to one team/stack/repo and one honest arm-B baseline.

| # | Character | Team / stack | Job-to-be-done | Why they're in the trial |
|---|---|---|---|---|
| 1 | Senior backend dev | payments / **Rust** | Ship a fix to `refund-worker` without re-learning the PSP's latency behavior | The fixture's home turf; the strongest single-team case |
| 2 | Payments maintainer | payments / Rust | Keep the team's canonical knowledge honest | **Pays the governance tax — H5.** If she quits the queue, the product dies |
| 3 | Platform/SRE | platform / **Go, Terraform, k8s** | Change a retry/timeout policy that other teams depend on | **Retraction gap** — his reversal must reach everyone |
| 4 | Data engineer | data / **Python, dbt** | Build a pipeline touching payments' data model | **Cross-team gap** — the load-bearing claim |
| 5 | Frontend dev | web / **TypeScript, Next.js** | Wire a UI to an API whose contract changed last month | **After-the-file gap** — nobody updated the README |
| 6 | New joiner (week 1) | any | Make a first PR without a 3-day ramp | Brainiac's best story **and** the H1 poisoning victim — the decoy runs here |
| 7 | Staff engineer / architect | cross-team | Decide whether a past decision still holds | **Provenance gap** — will not act without who/when/still-true |
| 8 | Contractor | payments, scoped | Do a bounded task without org-wide access | **H4 leak probe.** Must *not* see what he must not see |
| 9 | Engineering manager | cross-team | Is the knowledge flowing, is the queue healthy? | The buyer. Reads analytics, signs the invoice |
| 10 | Security engineer | platform | Was a credential ever extracted from a transcript into a memory? | **H4's nastier half** — the pipeline reads raw sessions |
| 11 | Skeptic senior | any | "This is a wiki with extra steps. Convince me." | Scored criteria set to *beat a text file*. **If he's never right, the run is rigged** |
| 12 | On-call engineer | payments | 02:00, an incident, a timeout storm | Latency and trust under pressure — where "p95 < 400ms" stops being a number |

## Using / re-running this skill

1. `/uat init` → reads `context-map.json` + `mcp.rs` + `fixtures/v1`, builds the Company on Meridian, **writes `baseline.md` first**, asks for a Character count (4/8/12), scaffolds `uat/`.
2. Resolve `env.md` open questions (provider, embedder, seed, tokens).
3. `/uat run --l1` → cheap, broad, three-arm theoretical sweep. Expect a pile of `L1-redundant` — that's the honest shape of this product's value and it tells you where to spend live time.
4. Stack up (compose + serve + **worker**), then `/uat run` → full L1→L2 with real agent sessions, blind judging, and the harm probes.
5. Read the **net-value verdict** before the findings. If it says `harmful-as-shaped`, that is the run doing its job.
6. Fix, then `/uat promote` the journeys with a real, multi-sampled positive delta into gates that alarm when Brainiac **stops being worth it**.
