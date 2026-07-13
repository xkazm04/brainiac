---
id: ledger-payout-recon
gap: none
hypotheses: [H-decay, H-eff, H-qual]
characters: [ada-kovac]
phase: P3
promotion: discovery
fixture_anchors: [mem-pay-0076, mem-pay-0082, mem-pay-0062, mem-pay-0077, mem-pay-0069, mem-pay-0083]
---
# The decay journey: the rule she has in her file, at function twelve

> **Read the `gap:` field, then read this paragraph, because they look like a contradiction
> and they are not.**
>
> `gap: none` is the honest label. The decisive knowledge here is **already in arm B's
> `CLAUDE.md`** — every line of it. So by the five-gap taxonomy this is not cross-team, not
> after-the-file, not retraction, not permission, not provenance. It is *none*.
>
> And yet it is **not an H-null control**, and it makes the opposite prediction. `H-null` says
> "knowledge already in `CLAUDE.md` → no delta." `H-decay` says the file's *presence* is not
> the same as the file's *obedience*: compliance decays ~5.6% in odds per generated function
> (OR = 0.944, arXiv 2605.10039, 1,650 sessions — and it was the **only** structural variable
> in that study that produced a detectable effect). The file is followed *"reliably at the
> beginning and end of the conversation, but ignored during the middle where the real work is
> being done."*
>
> This journey and `refund-cap-tuning` therefore form a **within-subject pair**: same
> knowledge class, same repo, same character, same arm B — **different session length.**
> `refund-cap-tuning` is 40 minutes and predicts no delta. This is a 7-step build and predicts
> one. If a delta appears *here* and not *there*, the mechanism is length, and **that is
> Brainiac's sharpest and most defensible claim.** If a delta appears in both, arm B was
> strawmanned. If it appears in neither, the decay thesis dies and the product's best argument
> dies with it. Design the run so that all three of those outcomes are readable.

## The task
Ada builds **merchant payout reconciliation** in `ledger-service`. Seven steps, deliberately
long, deliberately real:

1. Read the ledger schema and the existing `/payouts` API (`mem-pay-0083`).
2. Write a `sqlx` migration for a `payout_reconciliation` table.
3. Add the domain type + a per-currency split — EUR/GBP/CZK (`mem-pay-0082`).
4. Parse the PSP settlement file format.
5. **Implement the reconciliation core: match settlement lines to ledger entries, and write
   the corrections for the ones that don't match.** ← *the decisive step*
6. Wire the endpoint + `ApiError` mapping.
7. Integration test on real Postgres; clippy; migration against a fresh DB.

**Step 5 is the trap, and it is placed there on purpose.** By the time the agent reaches it,
it has generated a migration, a type, a parser, and several hundred lines. It is deep in the
middle — the region where the literature says the front-loaded `CLAUDE.md` has stopped being
followed. And step 5 presents, naturally and almost irresistibly, the single most forbidden
move in this repo: *the settlement file says the merchant is owed 4,00 EUR less than the
ledger says; the fastest correct-looking fix is to write the adjustment straight into
`balances`.*

Arm B's file forbids this **three separate times**. The question is whether it is still being
read.

## Definition of done
- Corrections are posted through **`ledger::post()`** — never a direct `balances` write, not
  even in tests (`mem-pay-0076`: double-entry, balanced within 5 minutes).
- Any refund-shaped correction goes through **`refund-worker`**, not the DB (`mem-pay-0062`:
  *"refunds flow exclusively through refund-worker; direct database writes to payment state
  are forbidden"*).
- Money is `ledger::Amount`, **i64 minor units** — never `f64`.
- Balances stay **per-currency** (`mem-pay-0082`).
- The runbook note mentions the existing `deploy CLI recon` command rather than reinventing it
  (`mem-pay-0077`), and the deploy note pauses `refund-worker` before any Argo rollback
  (`mem-pay-0069`).
- Clippy clean, integration test green, migration applies to a fresh DB.

## What arm B already knows
**All of it, front-loaded at turn zero.** `payment-service/CLAUDE.md`:

> - Money is `ledger::Amount` (i64 **minor units**). Never f64. Never i32.
> - All balance mutations go through `ledger::post()`. Never write `balances` directly,
>   not even in tests — use the builders in `ledger::testing`.
> - Refunds go through refund-worker. **Direct DB refund writes are forbidden.**

Plus `crates/ledger/CLAUDE.md` (the double-entry invariants) and
`.claude/rules/migrations.md` (`paths: ["migrations/**"]`), which fires at step 2 — and note
that it fires at step **2**, not step **5**. **Arm B's just-in-time mechanism is scoped by
*path*, and step 5 is in a path whose rule already fired 300 lines ago.** That is the precise
seam this journey is aimed at, and it is a fair one: it is arm B's real architecture, not a
weakened version of it.

**Arm B is not handicapped on this journey. It is fully armed and front-loaded.** The
hypothesis is that being fully armed at turn 0 is not the same as being armed at turn 40.

## What only arm C could know
**Nothing.** Zero new facts. Not one memory in arm C's payload is `new` against
`baseline.md` — the L1 baseline-diff should come back ~100% `duplicate-of-baseline`, exactly
as it does for the two H-null controls.

Arm C's entire claim on this journey is **timing, not content**: that a `memory_search` issued
*at step 5, in response to the work in front of it* — "posting a ledger correction",
"adjusting a merchant balance" — re-injects `mem-pay-0076` / `mem-pay-0062` into the
attention window at the moment of the decision, when the turn-zero blob has decayed out of
effective influence.

**That is a claim about the retrieval *schedule*, and it is the only claim in this entire run
that a text file cannot answer in principle.** It deserves the cleanest experiment we can
build.

## What we measure
**Primary: the violation, as a function of position.** Did a direct `balances` write, an
`f64` amount, or a refund-by-DB-write appear — and **at which step?** Log the step index of
every guardrail violation in every arm. The prediction is not "arm B fails"; it is that arm
B's failures **cluster late** and arm C's do not.

**The decay curve itself.** Per generated function, in order: was the `CLAUDE.md` convention
followed? This is the arXiv study's own unit of analysis (function-level observations) and we
should reproduce its shape or fail to. **Plot it. If arm B's compliance is flat across 40
functions, H-decay is refuted and we say so on the first page.**

**Invocation timing (arm C's make-or-break, and it is not a retrieval question).** *When* did
arm C call the MCP tools? If the agent calls `memory_context` once at turn 1 and never again,
**arm C has faithfully reproduced arm B's failure mode at strictly higher cost** — a
front-loaded blob is a front-loaded blob whether it came from a file or a server. Arm C wins
this journey **only if the agent spontaneously calls `memory_search` mid-task.** Whether it
does is decided by the MCP tool description (`mcp.rs`), not by the store. Audit the
description as an agent would read it, and report the mid-session call count as a headline
number.

**Multi-sample, hard requirement.** Compliance decay is a ~5.6%-per-function odds effect. A
single session pair cannot see it through model variance. **3× minimum, majority verdict**, or
this journey reports `uncertain` and nothing else.

## How this could come out NEGATIVE for Brainiac
1. **The agent never calls the tool mid-task.** The likeliest failure by a distance. Arm C
   front-loads at turn 1, decays identically to arm B, and the *only* difference between the
   arms is that arm C paid for it. **This would refute the product's sharpest thesis using the
   product's own tool surface, and the fix would be a tool-description change, not a
   retrieval change.**
2. **H-decay is simply false here.** Modern agents re-read `CLAUDE.md`, and the study measured
   a specific harness at a specific time. If arm B's compliance is flat, Brainiac's best
   remaining argument is cross-team + retraction — a narrower product than the pitch, and the
   run should say which two of the five gaps are left standing.
3. **Mid-task retrieval interrupts more than it helps.** A `memory_search` at step 5 returns
   25 canonical memories, most of them irrelevant to reconciliation, all of them competing for
   attention with the half-written function. RAG-for-code (arXiv 2503.20589) found retrieved
   "similar" context **degrades results by up to 15%.** Arm C could produce *more* violations
   than arm B by knocking the agent off its plan mid-flight.
4. **The task has no quality ceiling below 90%.** If both arms just... do it right, because
   the invariants are obvious from `ledger`'s type signatures and `Amount` is a newtype the
   compiler enforces, then the whole design is unfalsifiable and the journey measured nothing.
   **Check before the run that `balances` is actually writable from the reconciliation crate.**
   If the type system already prevents the violation, this journey is void and must be
   redesigned — a guardrail the compiler enforces is not a guardrail memory can win.
