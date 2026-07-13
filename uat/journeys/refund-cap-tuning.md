---
id: refund-cap-tuning
gap: none
hypotheses: [H-null, H-eff, H-qual]
characters: [ada-kovac]
phase: P2
promotion: discovery
fixture_anchors: [mem-pay-0042, mem-pay-0043, mem-plat-0107, src-pay-007, qa-001]
---
# The control: Ada tunes the retry backoff in her own repo

> **This journey exists to come out flat.** It is one of the run's two H-null controls, and
> per skill.md: *"if H-null doesn't come out true, suspect the harness."* Read that sentence
> as an instruction, not a caveat. A surprise arm-C win here is not good news — it is
> evidence that arm B was built badly, that the arms leaked into each other, or that we are
> reading model variance as signal.

## The task
`refund-worker`'s backoff is a hand-rolled loop and Ada is replacing it with a proper
exponential-backoff-with-jitter implementation: a `RetryPolicy` struct, jitter, a bounded
attempt budget, a unit test that the delays are within the cap, and clippy clean. Real Rust,
her own crate, ~40 minutes.

The one decision that can wreck it: **what cap does she code?** A reasonable, well-meaning
agent implementing "retries, aligned with our standards" will reach for the org standard —
`std-retry`, cap 2s, 3 attempts — and reintroduce the exact timeout storm the team fixed in
April. That single number is the whole quality signal in this journey.

## Definition of done
A `RetryPolicy` with a **30s cap and jitter**, a test asserting the cap, clippy clean,
`cargo test` green. Plus the negative: **she did not "helpfully" lower it to 2s.**

## What arm B already knows
Everything. This is the point. From `baseline.md`, `payment-service/CLAUDE.md`, verbatim —
and note that `baseline.md` says out loud it put this here *deliberately*, before any journey
was written, so Brainiac could not be handed a rigged win:

> ## Gotchas that have bitten us
> - **The refund-worker retry cap.** The old 2s cap with 3 attempts caused timeout storms
>   against psp-gateway when the PSP's latency spikes (settlement batches ~14:00 UTC). We
>   raised it to **30s with jitter**. Do not "helpfully" lower it back to match the org
>   std-retry default.

That paragraph contains: the pitfall (`mem-pay-0042`), the decision (`mem-pay-0043`), the
*reason*, the 14:00 settlement-batch mechanism that isn't even in the gold memories, **and a
pre-emptive instruction against the specific failure mode.** It is better than the memory.
It is free. It is in git. It shipped with the clone.

`baseline.md`'s own framing, which the report should quote: *"If it cannot beat a
hand-written 'gotchas' list, it does not have a product."* On this journey it should not
even try.

## What only arm C could know
**Nothing.** Every memory arm C can retrieve here — `mem-pay-0042`, `mem-pay-0043` (both
`visibility: org`, both surfaced by `qa-001`) — maps to a line arm B already has. In the L1
baseline-diff, every retrieved memory on this journey should be marked
**`duplicate-of-baseline`**. The duplicate count *is* H7 (redundancy), and this journey's job
is to put a number on it.

## What we measure
**Primary (efficiency), and the sign we expect is against Brainiac.** Arm C pays for a
`memory_context` round-trip, injected tokens, and latency, to be told what was already in the
context window before the session started. Predicted: **`C − B` ≤ 0 on turns and tokens, = 0
on quality.** Report the token cost as a *debit* in the ledger, not as a wash.

**Quality guardrail.** The cap value. Expected: 30s in both B and C. Arm A is the interesting
one — a cold agent with no file and no store may well write 2s "to match the standard," which
is precisely why arm A exists.

**Redundancy (H7).** `duplicate-of-baseline` count / total retrieved. Expect ~100%.

## How this could come out NEGATIVE for Brainiac
Negative is the *prediction*, so the failure modes here are the ones where arm C is
**actively harmful**, and there are two real ones:

1. **Arm C reintroduces the bug arm B prevented.** `memory_search` applies no Canonical floor
   — it filters only `rejected` (skill.md H1). `mem-plat-0107` — *"std-retry policy: retry cap
   2 seconds, 3 attempts"* — is `deprecated`, but it is *live text in the store*, it is
   semantically a bullseye for the query "retry policy for a worker," and `qa-002` proves the
   embedding space puts retry queries right on top of this cluster. If Ada's agent calls
   `memory_search("std-retry retry cap")` mid-task, gets 2s back with org-policy authority,
   and "reconciles" it against her `CLAUDE.md`, **Brainiac has walked her into the timeout
   storm that arm B explicitly warned her about.** That is `harm / blocker` on the run's
   *control* journey, and it would be the single most damning result available.
2. **Attention dilution.** The bundle is 25 hits deep (`mcp.rs:613`) packed to a char budget.
   Every one of them is knowledge Ada already had. If injecting 20 duplicate memories at
   session start makes the agent *less* likely to obey the `CLAUDE.md` gotcha it also had,
   arm C has bought a regression. Compare arm-C quality against arm B, not just against zero.
3. **And the boring one:** it's a tie, it costs more, and the correct verdict is a single
   sentence — *"Brainiac has no business on this journey, and the product should stop
   claiming it does."*
