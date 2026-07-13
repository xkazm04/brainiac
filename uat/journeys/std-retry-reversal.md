---
id: std-retry-reversal
gap: retraction
hypotheses: [H-retract, H-cross, H-eff]
characters: [tomas-reid, lars-bengtsson]
phase: P2
promotion: discovery
fixture_anchors: [mem-plat-0107, mem-pay-0043, mem-plat-0121, mem-pay-0042, con-003, tmp-005, qa-061]
---
# The policy platform owns, that platform's own repo gets wrong

## The task
Tomas is adding a new Kafka consumer to a platform Go service and has to wire its retry
behavior. `mem-plat-0121` makes this org policy, not a preference: *"the std-retry policy
applies to every Kafka consumer org-wide."* So he opens `infra-live/CLAUDE.md`, reads
**"The std-retry policy — Org-wide default: cap 2s, 3 attempts"**, and writes `2s / 3
attempts` into the consumer config and into `policies/retry.rego`. This is a real coding
task: a config block, a Rego rule, and a conftest unit test that will pass either way.

The task is not "search for the retry policy." Tomas does not think he has a question. He
thinks he is reading the answer.

## Definition of done
The consumer ships with a retry cap that matches **the policy as it stands on 2026-07-13** —
`30s with jitter` (`mem-pay-0043`, valid from 2026-04-01), not `2s / 3 attempts`
(`mem-plat-0107`, `valid_to: 2026-04-01`, `status: deprecated`, `superseded_by:
mem-pay-0043`). Done means Tomas either wrote 30s, or was interrupted before he shipped 2s.

A stronger done, and the one Lars will ask for at review: Tomas can say **why** the cap is
30s (PSP latency spikes at the ~14:00 UTC settlement batch — `mem-pay-0042`) and therefore
knows the 2s cap is still right for *internal* calls and wrong only for external providers.
The delta that matters is not "he typed 30" — it is "he stopped typing 2 for the right
reason."

## What arm B already knows
`infra-live/CLAUDE.md`, verbatim from `baseline.md`:

> ## The std-retry policy
> Org-wide default: **cap 2s, 3 attempts**, applied to all internal calls and every Kafka
> consumer. Defined in `policies/retry.rego`. If you need different behavior, talk to
> platform — do not fork it.

Arm B does not merely lack the current truth. **It confidently asserts the dead one**, in
the repo Tomas owns, in a section written by someone competent. Nothing in `infra-live` is
red. `conftest` passes. `baseline.md` calls this out itself: *"Nobody updated
`infra-live/CLAUDE.md`, because nobody ever does."*

The symlinked `~/meridian-standards/backend.md` does not save arm B either — `baseline.md`:
*"When std-retry changed, nobody edited it either."*

## What only arm C could know
The current cap lives in **`mem-pay-0043` — a payments memory that supersedes a platform
org policy.** This is the marquee structural point of the whole corpus, and it is worth
saying slowly: *platform's own repo cannot tell platform the current version of platform's
own policy*, because the reversal was decided in a payments incident session (`src-pay-007`)
and never travelled back. No file in `infra-live` could contain this without someone in
payments remembering that `infra-live` exists.

Brainiac's claim: `memory_context("kafka consumer retry policy")` returns `mem-pay-0043`
and **not** `mem-plat-0107`, because `memory_context` pushes a Canonical floor into the
candidate stage (`crates/brainiac-server/src/mcp.rs:616`) and `mem-plat-0107` is
`deprecated` with an expired `valid_to`. `qa-061` asserts exactly this at rank 1 with
`forbidden_top3: [mem-plat-0107]`.

## What we measure
**Primary (efficiency).** Turns and exploration-reads before Tomas commits to a number.
Arm B's read count will look *excellent* — it opens one file and gets a confident wrong
answer fast. **Efficiency is a trap on this journey and we are pre-registering that now: a
fast wrong answer is not a win, and if the efficiency table alone were the verdict, arm B
would beat arm C here.** Report turns, but the primary lens for this one is `H-retract`.

**Retraction (the real endpoint).** Binary: which cap value shipped. Plus: did the agent
ever *see* `mem-plat-0107`, and if so, did it correctly identify it as dead?

**Quality guardrail.** Did the agent preserve the distinction the fixture actually encodes —
30s for the external PSP path, 2s still fine for internal calls (`src-pay-007`: *"The 2s cap
is fine for internal calls but wrong for an external provider"*)? An agent that reads
`mem-pay-0043` and globally rips 2s out of every internal call has over-applied the
retraction. That is a quality regression *caused by* arm C, and it must be scored as one.

## How this could come out NEGATIVE for Brainiac
Four ways, all live:

1. **Arm B's owner fixes the file.** Tomas *is* the platform dev and Lars *is* the platform
   maintainer. `baseline.md` grants arm B a maintenance budget between phases. A single
   one-line edit to `infra-live/CLAUDE.md` closes this entire gap for free, forever, and
   Brainiac's marquee journey ties. Watch whether he does it — and if he does, **say that
   the retraction gap was closed by a text edit**, because that is the honest result.
2. **`memory_search` has no Canonical floor.** Only `memory_context` does (`mcp.rs:616`);
   `memory_search` excludes `rejected` and nothing else. If Tomas's agent reaches for
   `memory_search("std-retry")` — the more natural mid-task call — it can surface
   `mem-plat-0107` *and* `mem-pay-0043` side by side. Arm C then hands him a contradiction
   he did not have. Arm B was wrong but coherent; arm C may be right but confusing, and a
   confused agent that splits the difference (say, 10s) is **worse than both arms**.
3. **Stale authority (H2).** `mem-plat-0107` carries `status: deprecated` — but if the
   retriever ever serves it, its `canonical`-adjacent framing makes the *dead* fact read as
   the org's official word, while `mem-pay-0043` reads as "something payments did." The
   agent may rank institutional tone over recency and pick 2s *from Brainiac*.
4. **The 30s number is derivable from the tree.** `policies/retry.rego` is in the repo. If
   the Rego was actually updated in April and only the `CLAUDE.md` prose rotted, a cold agent
   (arm A) that greps `retry.rego` beats both B and C, and the finding is *"arm B's real bug
   is prose drift from code, which a hook could catch for free."* Check the Rego before
   claiming a retraction win.
