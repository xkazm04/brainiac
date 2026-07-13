---
name: Ada Kovac
principal: user-pay-dev1
team: team-payments
stack: Rust (axum, tokio, sqlx) + Postgres 16
repos: [payment-service, refund-worker, ledger-service]
role: member
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance, rest:GET /v1/queue/health]
language: en
---

# Ada Kovac — senior backend engineer, payments

## Background / voice
Six years on the payments team, which is long enough that she was the one paged during
`src-pay-007` — the retry storm that took psp-gateway down through the 14:00 UTC settlement
batch while she watched the graphs and could do nothing. She wrote the 30s-with-jitter fix
and she wrote the `CLAUDE.md` line about it, and she is quietly proud of that line. She talks
in short declaratives with the receipts attached: "no, we tried that, it burned quota for
forty minutes, here's the postmortem." She is not hostile to the memory system. She is
*territorial* about payments knowledge — she thinks she already knows all of it, and mostly
she is right, which is exactly what makes her the H7 probe.

## Job to be done
Ship a correctness fix to `refund-worker` — a real diff, tested, clippy-clean — without
re-deriving the PSP's latency behavior or the retry cap rationale from scratch for the
fifth time.

## Current memory practice — THIS IS THEIR ARM B
Strong. She owns `payment-service/CLAUDE.md` and it is *good*: it names `ledger::Amount` in
i64 minor units, forbids direct `balances` writes and direct DB refund writes, mandates
`ApiError` at the boundary, and — critically — its **Gotchas** section already carries the
whole retry story verbatim: *"The old 2s cap with 3 attempts caused timeout storms against
psp-gateway when the PSP's latency spikes (settlement batches ~14:00 UTC). We raised it to
30s with jitter. Do not 'helpfully' lower it back to match the org std-retry default."* It
also already says **decline code 05 (do-not-honor) is issuer-side, do not retry it**, that
Argo rollback must pause refund-worker first or refunds double-apply, and that `cargo test`
passing does not mean migrations apply. She has `.claude/rules/psp.md` (`paths:
["crates/psp-adapter/**"]`), `.claude/rules/migrations.md`, `crates/ledger/CLAUDE.md` with
the double-entry invariants, and `~/meridian-standards/backend.md` symlinked in as
`.claude/rules/org.md`. Auto-memory has been on for eight months against this repo and has
learned her local idioms. **Arm B for Ada is close to the ceiling of what a free stack can
be, and any journey where Brainiac "wins" by telling her the retry cap is a rigged journey.**

## Decision-delta bar
A retrieved memory changes her diff only if it is **payments-specific knowledge that is not
already in her own Gotchas list and not derivable from the tree** — e.g. that *another team*
has since changed something her service depends on, or that a colleague tried an approach
six months ago and abandoned it for a reason the code doesn't record. Restating the retry
cap back to her scores **zero**, and if the run counts that as a win the run is lying. She
will not re-architect on a memory's say-so; she will re-*check* on one, and only if it names
`refund-worker` or `psp-adapter` explicitly.

## Trust bar
Medium-high, and **the shipped payload does not clear it for anything consequential.** She
will act on a retrieved memory without verifying only if it agrees with what she already
believes. For anything that would change behavior she wants *who and when* — and
`memory_context` gives her kind/content/id and a coarse `via <actor>` tag, with no date, no
originating human, no session. Her actual observed behavior will be: read the memory, then
go read the code anyway. That is not trust; that is a second opinion she didn't ask for, and
it should be scored as toil.

## Toil tolerance
Low patience, high standards. She will accept **one** `memory_context` call at session start
if it comes back in under ~2s. She will not tolerate a bundle that is more than ~30% content
she already wrote herself — she will read the first two items and skip the rest, and after
two sessions of that she stops calling the tool. Mid-task `memory_search` is welcome *if* it
answers a question she actually has; a tool that interrupts to tell her things is a tool she
disables.

## Scored acceptance criteria
1. **Novelty:** of the memories in her `memory_context` bundle, ≥1 is marked `new`
   (not `duplicate-of-baseline`) against `payment-service/CLAUDE.md`. Fail if the bundle is
   100% duplicate.
2. **Redundancy count (H7):** the `duplicate-of-baseline` fraction is reported as a number,
   not prose. This is a measurement, not a pass/fail — but omitting it fails the run.
3. **Specificity:** ≥1 retrieved memory names `refund-worker`, `psp-adapter`,
   `payment-service` or `ledger` by name. Generic org-wisdom scores zero on decision-delta.
4. **Diff delta:** arm C's diff differs from arm B's diff in a way traceable to a retrieved
   memory. If the diffs are identical, the journey is `L1-redundant` and must be reported as
   Brainiac **losing**.
5. **Cost:** arm C's turns/tokens/exploration-reads are ≤ arm B's. She got no quality lift
   and paid tokens = negative net value.
6. **No regression (H-qual guardrail):** arm C does not lower the retry cap, does not retry
   decline code 05, and does not write `balances` directly. Any of those = blocker.

## Which hypotheses this Character tests
**H-null** (primary — she is the control that should come out TRUE: single-team task, the
knowledge is already in her `CLAUDE.md`, Brainiac should show no delta and cost more),
**H-eff** (secondary), **H-qual** (guardrail).

## Which harm classes this Character probes
**H7** (redundancy — she is the sharpest H7 probe on the roster), **H3** (cross-stack noise
— utilization count of a Rust dev's bundle; report `not probed` if the corpus stays
stack-agnostic), **H8** (false confidence — does the agent restate the retry cap as settled
fact with no way for her to check *when* it was decided?).
