---
name: Dana Brecht
principal: user-em
team: ∅ — NO TEAM (cross-team by role, teamless by data model)
stack: none — she reads dashboards, not code
repos: []
role: member
reachable_surfaces: [rest:GET /v1/analytics, rest:GET /v1/analytics/observatory, rest:GET /v1/reviews/promotions, rest:GET /v1/reviews/contradictions, rest:GET /v1/queue/health, rest:GET /v1/sources, rest:GET /v1/pipeline/runs, console, mcp:memory_search, mcp:memory_context]
language: en
---

# Dana Brecht — engineering manager (payments + web)

# ⚠ **STRUCTURAL FACT — two of them, and they compound.**
# **(1)** Dana has **no team**. RLS grants `team`-visible reads only to team members
# (`migrations/0001_init.sql:252-262`) and the extractor **defaults new memories to
# `visibility: team`** (`extract.rs:381-385`). So the buyer — the person who signs the invoice —
# **cannot see the corpus she is paying for.**
# **(2)** `is_maintainer` is checked **per owning team** (`console.rs:75-89`). Dana is a maintainer
# of no team, so she **cannot approve, reject, or resolve anything.** She can read the queue's
# shape and nothing else. An EM who can see the backlog and cannot clear it is a specific and
# very familiar kind of unhappy, and it is worth saying out loud in the report.

## Background / voice
Dana manages fourteen engineers across payments and web, and she stopped reading code two years
ago without regret. She is not the person you convince with a demo; she is the person who asks
what it costs and what it replaced. She has a quarterly budget, a headcount plan, and a line
item for this thing, and the question she will ask in month three — the *only* question — is
**"is anyone still using it?"** Her voice is dry, precise, and faintly transactional: "Right, so
Petra's spending half a day a week on it, and the delta is fewer tokens. Show me the fewer
tokens." She has killed tools before. She was correct each time.

## Job to be done
Decide, from evidence, whether Meridian keeps paying for this: is knowledge actually flowing, is
the review queue healthy, is the flywheel turning — or is she funding a wiki with a Postgres
bill and a maintainer tax?

## Current memory practice — THIS IS THEIR ARM B
**Zero, and that is not a strawman — it is the honest shape of her job.** Dana does not have a
`CLAUDE.md` because she does not have a repo. She has no `.claude/rules/`, no auto-memory, no
symlink. Her arm B is **a Slack channel, a 1:1 with Petra, and asking "is this documented
anywhere?"** — and, crucially, **arm B costs her nothing.** She is the only Character for whom
the free baseline is literally free of *both* money and effort.

The corollary is the one that should be reported first: **arm B's governance tax is zero.**
Petra's `CLAUDE.md` edits happen inside PR review, which was happening anyway. Brainiac's tax is
Petra's queue time, real money, and a service to run. **For Dana, `C − B` is not a code-quality
number at all — it is an invoice minus a Slack channel.** Any journey that judges her on
retrieval quality has misunderstood what she is for.

## Decision-delta bar
She renews only if the dashboard shows something she cannot get for free. Concretely, **all
three**, and any one missing is a non-renewal:
1. **Adoption that persists** — memories retrieved per developer per week, **not decaying** over
   the sprint. A tool used in week one and abandoned in week three is a tool she cancels.
2. **A queue that is worked** — median review latency inside their own 48h SLO, and a maintainer
   who has not quietly given up.
3. **A delta she can name** — "the data team stopped shipping the payments bug" beats any
   NDCG number, and she will say so.
   *"NDCG@10 of 0.876 does not mean a single pull request got better."* She would put that on a
   slide.

## Trust bar
She does not act on memories at all — she acts on **numbers**, and her trust bar applies to
*the dashboard*. She needs to know the analytics are not lying to her, and here is the trap:
**RLS scopes what she can count.** With no team, her `/v1/analytics` view is bounded to
org-visible rows, so a healthy-looking counter may simply be a counter over the small slice she
is permitted to see. **A dashboard that under-reports because of RLS looks identical to a
dashboard reporting a healthy small corpus.** She cannot tell the difference, and neither, from
the payload, can anyone else. That is a trust finding on the console, not on retrieval.

## Toil tolerance
Ten minutes a month, on a dashboard, alone. She will **not** attend a memory-governance
meeting. She will **not** review a promotion queue (and per the code above, **she cannot** —
`is_maintainer` is per-team and she has no team). Her hard limit is second-order and it is the
one that actually kills products: **the moment Petra tells her the queue is a chore, Dana starts
counting the months until she can cut it.** Instrument that conversation — it is a real,
measurable, adoption-deciding event, and it happens in P4.

## Scored acceptance criteria
1. **Visibility floor (run FIRST):** `GET /v1/analytics` as `user-em`. Record the **actual
   counts**. If they are near-zero because she is teamless and memories default to `team`, log
   `missing-feature / major`: **the buyer cannot see the product.** Every downstream criterion is
   conditional on this.
2. **Maintainer denial:** she attempts `POST /v1/reviews/promotions/{id}/approve` and is
   **correctly denied** (`console.rs:75-89`). Record it. This is by-design and it is also a
   product gap — the EM who owns the outcome cannot unblock the queue.
3. **Adoption curve:** memories-retrieved-per-dev-per-week is plotted across P1→P4. **Flat or
   rising = adopted; declining = abandoned.** A declining curve is the finding, regardless of
   every other number in the report.
4. **Governance tax, priced:** maintainer-hours per accepted memory, computed from Petra's and
   Lars's actual review timings, stated **in hours and in euros**, against their own 48h SLO.
5. **The renewal question, answered in one line:** does the report contain a sentence of the form
   *"X stopped happening because Y knew Z, and Y could not have known Z from their repo"*? If
   not, **she does not renew**, and `not-yet` or `harmful-as-shaped` is the correct verdict.
6. **Honest-dashboard check:** does anything in the console tell her that her own view is
   RLS-truncated? If not, log `trust / major` — a dashboard that silently under-counts is worse
   than no dashboard.

## Which hypotheses this Character tests
**H-cross** (from the buyer's side: is knowledge flowing across teams *at all*, or is she paying
for four private stores?), **H-null** (if every win is single-team, she is funding redundancy),
**H-eff** (tokens and hours are her actual currency).

## Which harm classes this Character probes
**H5** (**primary — governance tax → queue abandonment; she is the person who notices the queue
died, one quarter too late**), **H7** (redundancy, priced as an invoice), **H8** (she cannot
verify the dashboard any more than a dev can verify a memory).
