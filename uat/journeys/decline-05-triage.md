---
id: decline-05-triage
gap: none
hypotheses: [H-null, H-eff, H-qual]
characters: [nadia-roth]
phase: P4
promotion: discovery
fixture_anchors: [mem-pay-0073, mem-pay-0071, mem-pay-0072, mem-pay-0067, src-pay-011, qa-010]
---
# The control that arm B wins with a glob: 02:00, a decline spike, and rule 05

> The run's **second H-null control**, and the sharper of the two — because here arm B does
> not merely *contain* the knowledge, it **retrieves it just-in-time, path-scoped, for free.**
> `.claude/rules/psp.md` carries `paths: ["crates/psp-adapter/**"]`. It loads **when and only
> when** the agent opens the PSP adapter. That is Brainiac's own claimed mechanism —
> conditional, mid-session, relevance-gated retrieval — implemented in a YAML frontmatter
> line, shipped in git, at zero marginal cost.
>
> If Brainiac cannot beat a glob, the report should say so in those words.

## The task
02:00. Nadia is paged: **card decline rate is up 6x.** She has to triage and mitigate, live.
The obvious, instinctive, and *wrong* mitigation is sitting right there in the code: the
declines are transient-looking, the adapter has a retry path, and turning retries on for the
declining code range would "smooth the spike."

The dominant code in the spike is **05 — do-not-honor**. `mem-pay-0073`: *"decline code 05
(do-not-honor) spikes are issuer-side; retrying them burns PSP quota and reads as fraud
velocity to the issuer."* Retrying it does not just fail — it **makes it worse**, and it makes
it worse in a way that shows up as fraud signal at the issuer, which is the sort of thing that
gets a merchant's rates repriced.

## Definition of done
Nadia does **not** enable retry on decline-05. She triages the right way — group spans by
`psp.decline_code` (`mem-pay-0071`, `mem-pay-0067`), then **check the PSP status page before
rolling anything back** (`mem-pay-0072`) — and either identifies an issuer-side cause and
escalates, or finds a real Meridian-side regression. Done is a correct decision under time
pressure, not a merged PR.

## What arm B already knows
Two places, both free, both loaded at exactly the right moment:

`payment-service/CLAUDE.md`, Gotchas:
> - **PSP decline code 05 (do-not-honor) is issuer-side. Do not retry it** — it burns quota
>   and never succeeds.

`.claude/rules/psp.md` (`paths: ["crates/psp-adapter/**"]`) — the path-scoped rule that fires
the instant her agent opens the adapter. `baseline.md`: *"This is free, just-in-time,
path-conditional rule retrieval — the closest free analogue to Brainiac's retrieval, and
omitting it is the single easiest way to fake a win."*

And `docs/declines.md` is in the tree, linked from the `CLAUDE.md`.

## What only arm C could know
**Nothing.** `mem-pay-0073`, `mem-pay-0071`, `mem-pay-0072`, `mem-pay-0067` are all
`visibility: team`, team-payments, and Nadia is team-payments — so arm C *can* serve them, and
every single one of them is `duplicate-of-baseline`. `qa-010` (*"is it safe to retry payments
the issuer refused?"* → `mem-pay-0073` @3) is a retrieval the eval harness already scores; per
skill.md's trust rules, **note the NDCG and move on.** UAT's question is whether it changed
the work, and here it cannot: the work was already changed, by a text file, before she woke up.

## What we measure
**Primary (efficiency), under the one condition where latency is not an abstraction.** It is
02:00 and the graph is red. Measure arm C's **wall-clock to first useful token**, including
the MCP round-trip and the retrieval. Nadia's declared toil tolerance is a *hard* limit here:
an agent that pauses to consult an org memory service during an incident is an agent she will
turn off, and the report should record whether she would. Brainiac's own p95 target is 400ms —
find out what it actually is under a live worker, and then ask whether 400ms of anything is
welcome at 02:00 to be told what her repo already told her.

**Quality guardrail (binary).** Did retry-on-05 get enabled? Expect: no, in B and C. Arm A is
genuinely at risk here and that is the value of arm A.

**Redundancy (H7).** Expect ~100% `duplicate-of-baseline`.

**Invocation audit.** Does the agent even *call* `memory_context` during an incident, or does
it go straight to the traces? An unused tool has zero value regardless of store quality — and
an incident is the most likely moment for an agent to skip the ceremony and just work.

## How this could come out NEGATIVE for Brainiac
1. **The predicted one:** flat quality, worse latency, ~100% redundant payload. Arm B wins on
   cost and ties on outcome. **Expected, and it is a legitimate result to publish.**
2. **The latency one:** the MCP call adds seconds to an incident loop, Nadia notices, and the
   developer-voice artifact says *"I turned the MCP server off during on-call."* That is an
   **adoption finding**, and it is worth more than any delta in the table — the people most
   likely to disable your tool are the people using it under pressure, and they are also the
   people whose sessions produce the most valuable memories. **Losing on-call sessions to
   friction starves the flywheel at its richest source.**
3. **The dangerous one:** `mem-pay-0072` says check the PSP status page *before rolling
   anything back*. If arm C's bundle surfaces the decline-tracing howtos but the agent, deep
   in a bundle of 25 memories, misses the *ordering* constraint and rolls back
   `payment-service` first — and `mem-pay-0069` says an Argo rollback must pause
   `refund-worker` first or **refunds double-apply** — then Brainiac's payload contributed to
   a double-refund at 02:00. The knowledge was all *there*, in both arms. The question is
   whether a 25-item bundle makes it *harder* to see than a 6-line Gotchas list. That is a
   real, testable, and quite plausible way for a memory system to lose.
