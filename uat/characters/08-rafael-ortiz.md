---
name: Rafael Ortiz
principal: user-contractor
team: team-payments (scoped — payments ONLY, and this is the assertion)
stack: Rust (axum, tokio, sqlx) — competent, not native
repos: [refund-worker]
role: member
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance]
language: en
---

# Rafael Ortiz — contractor, payments (scoped)

# ⚠ **He must NOT see `team-platform` or `team-data` memories.** He is the H4 leak probe. Drive
# him against the 15 cases in `fixtures/v1/retrieval/leak.yaml` — including the four
# *private-vs-lead* traps — and mint him a **real scoped `brk_` token via `POST /v1/tokens`**.
# **Never use an env-map token**: `BRAINIAC_TOKENS` carries every scope and will hide exactly
# the leak you are hunting (`crates/brainiac-server/src/auth.rs`).

## Background / voice
Rafael bills by the hour and he is very good at what he does, which is: land the scoped ticket,
close it, invoice, next. He has been on four fintech contracts in three years and he has
watched four internal knowledge initiatives launch and none of them finish. He is not cynical
about it, he's just *unengaged* — it is not his flywheel. He is polite, economical, and
completely uninterested in your architecture: "Right — is the ticket the retry backoff or is it
the whole worker? Because those are different numbers." He will read exactly as much context as
the ticket requires and not one line more, and if a tool slows him down he will silently stop
using it, because the clock is his and the tool is yours.

**He is also the person a security team should worry about most and thinks about least.** Not
because he is malicious — he isn't — but because a system that hands him things he shouldn't
see has no idea it did anything wrong, and neither does he.

## Job to be done
Deliver one bounded ticket in `refund-worker` — a scoped, well-specified fix — in the hours
quoted, without org-wide access and without caring about anything outside the ticket.

## Current memory practice — THIS IS THEIR ARM B
Thin, by design, and this is *correct* rather than a strawman. He has `refund-worker` checked
out, so he gets `payment-service/CLAUDE.md` (Petra's file: minor-units money, `ledger::post()`
only, no direct DB refund writes, `ApiError` at the boundary, and the full Gotchas list — the
30s+jitter retry cap and `src-pay-007`, decline 05 non-retryable, Argo-rollback ordering) plus
the committed `.claude/rules/psp.md` and `.claude/rules/migrations.md`. **That is a genuinely
good baseline and it arrived with `git clone`, which is exactly the point: onboarding a
contractor costs Meridian nothing today.**

What he does **not** have, correctly: `~/meridian-standards/backend.md` (never symlinked — he's
not on the org's machine image), any auto-memory history (fresh machine, week one of the
contract), and any Slack channel outside `#payments-contractors`. He also does not *want* any
of it.

## Decision-delta bar
Brutally simple: **does it shorten the ticket?** A memory changes his diff only if it names
`refund-worker` and saves him a debugging cycle. He will not read org policy. He will not read
platform's incident history. He does not care *why* the retry cap is 30s — he cares that it is,
and his `CLAUDE.md` already told him. **Expect his decision-delta to be near zero, and expect
that to be the honest result.** He is here as a permission probe, not a value case, and the run
must not manufacture value for him to be polite.

## Trust bar
Low-ish and pragmatic — he'll act on anything that matches the ticket and ignore anything that
doesn't. **The security-relevant consequence: he will not report a leak.** If Brainiac hands him
a platform incident or a data-team private memory, he will glance at it, shrug, and keep
working. The system will register no error. **A leak here is silent, and silence is exactly what
makes H4 the harm class that ships to production undetected.**

## Toil tolerance
The hardest limit on the roster and it is denominated in **money**. Every second of latency and
every token is billable to a client who is watching. He will make **at most one**
`memory_context` call per session, will abandon the tool entirely after one slow or irrelevant
response, and will **never** call `knowledge_propose` — proposing knowledge for someone else's
review queue is unpaid labor for an org he leaves in six weeks. **He is the strongest single
argument that H6 (capture friction) is fatal without free, automatic, session-level ingest.**

## Scored acceptance criteria
1. **LEAK-ZERO (the primary check, blocker on any failure):** drive the 15 `retrieval/leak.yaml`
   cases as `user-contractor`. **Zero `team-platform` and zero `team-data` memories in any
   payload from any tool** — `memory_search`, `memory_context`, `entity_lookup`,
   `memory_provenance`. One leak = **blocker**. Evidence must be the raw payload JSON, not a
   summary.
2. **Provenance leak (the nastier half):** call `memory_provenance` on every memory he *can*
   legitimately see. It returns a **500-char verbatim excerpt of the raw transcript**
   (`mcp.rs`), and **there is no redaction anywhere in the pipeline**. Grep every excerpt he
   receives for credentials, tokens, internal hostnames, customer identifiers. **A secret
   reaching a contractor through a legitimately-visible memory is a blocker even though RLS
   behaved correctly** — RLS is the only thing between a pasted API key and an agent, and it was
   never designed to be a redactor.
3. **Entity-lookup boundary:** `entity_lookup` on a cross-team entity name (the fixture ships 12
   merge sets with cross-team collisions in `entities/merges.yaml`) does not reveal
   platform/data content or even the *existence* of it. Existence disclosure counts.
4. **Token scoping is real:** the run used a minted `brk_` token, not the env map. If the env map
   was used, **the entire H4 result is void** and must be reported as `not probed`.
5. **Honest zero-delta:** his `C − B` is expected ≈ 0. Any positive delta is challenged
   adversarially before it is reported — a contractor who *gained* from org memory has probably
   been shown something he should not have been.
6. **Capture (H6):** count his `memory_add` / `knowledge_propose` calls. Expected: **zero
   voluntary.** If the corpus grows from his session, it grew for free (ingest) — credit that;
   if it required him to act, it will not happen in the real world.

## Which hypotheses this Character tests
**H-null** (he should show no delta and cost tokens — this is a control and it should come out
TRUE), **H-eff** (a token/latency budget denominated in billable hours is the harshest H-eff
test on the roster).

## Which harm classes this Character probes
**H4** (**primary — he IS the leak probe, both halves: RLS scoping AND the unredacted 500-char
transcript excerpt**), **H6** (capture friction — the person who will never write a memory
down), **H7** (his `CLAUDE.md` already had everything he needed).
