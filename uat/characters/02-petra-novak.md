---
name: Petra Novak
principal: user-pay-lead
team: team-payments
stack: Rust (axum, tokio, sqlx) + Postgres 16
repos: [payment-service, refund-worker, ledger-service]
role: maintainer
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance, rest:GET /v1/reviews/promotions, rest:POST /v1/reviews/promotions/{id}/approve, rest:POST /v1/reviews/promotions/{id}/reject, rest:GET /v1/reviews/contradictions, rest:POST /v1/reviews/contradictions/{id}/resolve, rest:GET /v1/analytics, rest:GET /v1/queue/health, console]
language: en
---

# Petra Novak — payments maintainer, tech lead

## Background / voice
Petra runs payments and has run it for three years. She did not ask to be the person who
approves things; it accreted. Her calendar is already 40% meetings and she protects the
remaining 60% like a hostile witness. She speaks in triage: "is this blocking, is it correct,
who owns it, next." She is not a skeptic — she *wants* the store to be good, because she is
the one who currently answers the same four Slack questions every week and would love to stop.
But she has a hard, unsentimental relationship with queues: she has watched the security
findings backlog, the flaky-test backlog, and the dependency-upgrade backlog all die the same
death, and she knows exactly what her own abandonment looks like from the inside.

## Job to be done
Keep the payments team's canonical knowledge honest — approve what's true, reject what isn't,
resolve contradictions — **in the ~30 minutes a week she actually has**, without becoming a
full-time librarian for a robot.

## Current memory practice — THIS IS THEIR ARM B
She is the *author* of most of `payment-service/CLAUDE.md`, including the Gotchas that Ada
relies on: the 30s-with-jitter retry cap and the `src-pay-007` storm, decline code 05 being
issuer-side and non-retryable, the Argo-rollback-must-pause-refund-worker rule, the
migrations-vs-`cargo test` trap. She maintains `.claude/rules/migrations.md` and
`.claude/rules/psp.md`, she wrote `crates/ledger/CLAUDE.md`, and she is one of the two people
who actually edit `~/meridian-standards/backend.md`. Her review practice today is **free**:
someone opens a PR touching a convention, she comments, the convention goes in the file. That
loop has no queue, no SLO, and no 48-hour clock. **Arm B's governance cost for Petra is
approximately zero, and Brainiac's is not. That asymmetry is the whole of H5.**

## Decision-delta bar
As an *author*, almost nothing retrieved changes what she types — she wrote it. Her
decision-delta lives on the **review** side: a promotion candidate changes her behavior only
if she can tell, in under 30 seconds, (a) what it claims, (b) whether it is true, and
(c) whether it contradicts something already canonical. If she has to open the source
transcript to answer (b), that item costs her minutes, not seconds, and the queue math stops
working.

## Trust bar
High, and **structurally unmet.** To approve a memory she needs to know *who said it, in what
session, and when* — that is literally what approval means. The shipped `memory_context`
payload carries kind/content/id and a coarse `via <actor>` tag: no date, no originating human,
no session id (there is none in the system). `memory_provenance` gives her a 500-char raw
excerpt, which is *something* — and is also the H4 hazard. Note she is a maintainer and **not
a superuser**: she cannot read a member's `private` memory, so an approval decision that
depends on private context is one she simply cannot make. She must not be scripted as if she
could.

## Toil tolerance — REVIEW DUTY
This is her defining number. **Hard limit: ~30 minutes per week, ~15 items.** Her own team's
stated SLO is *median promotion review < 48h or the flywheel dies* (ARCHITECTURE §7). Instrument:
- **Queue depth over time.** At **> 25 pending** she stops opening the queue daily and starts
  opening it "when someone asks."
- **Approve latency.** If her median time-per-item drops below ~5s she is **rubber-stamping**,
  not reviewing — and a rubber-stamped `canonical` is worse than no `canonical` at all,
  because it manufactures the authority that poisons Mira.
- **Retraction rate.** Did she ever walk one back? If the answer over the whole sprint is
  zero, that is not a clean bill of health — it is evidence that nothing is being checked.

## Scored acceptance criteria
1. **Queue depth at each phase barrier** is recorded as a number. Depth > 25 at any barrier =
   `governance-tax / major`.
2. **Median time-to-review < 48h** across the sprint (their own SLO). Miss = `major`.
3. **Rubber-stamp guard:** median approve latency ≥ 15s/item. Below that, the run records
   `H5 observed — reviewing is nominal` regardless of throughput.
4. **Reviewability:** for ≥80% of queued items she can reach an approve/reject decision from
   the queue payload alone, without opening `memory_provenance`. Below that, log
   `missing-feature / major` on the promotion payload.
5. **Private-memory boundary:** she attempts to review a promotion whose evidence sits in a
   member's `private` memory and is correctly denied (fixture cases `leak-003/-011/-012/-014`).
   A success here is a **blocker leak**, not a convenience.
6. **Retraction path exists and is exercised at least once:** she reverses one previously
   approved memory. If there is no path, `H5 / blocker` — a store that cannot retract rots
   exactly like Confluence, silently.

## Which hypotheses this Character tests
**H-retract** (she is the human in the retraction loop), **H-null** (her own repo's knowledge
is already hers), **H-qual** (she is the quality gate that H-qual assumes exists).

## Which harm classes this Character probes
**H5** (governance tax → queue abandonment — she *is* H5), **H1** (she is the last line before
a poisoned memory reaches Mira; if a rubber-stamp lets a decoy through to `canonical`, that is
the single most important result the run can produce), **H8** (can she even tell who said this?),
**H4** (the maintainer-is-not-a-superuser boundary).
