---
id: refund-burst-features
gap: cross-team
hypotheses: [H-cross, H-eff, H-qual]
characters: [ingrid-sol]
phase: P3
promotion: discovery
fixture_anchors: [mem-pay-0042, mem-pay-0043, mem-data-0091, mem-data-0090, mem-data-0104, qa-062]
---
# The 14:00 spike in the refund events is not fraud

## The task
Ingrid is building a fraud feature in `event-lake`: **`refund_attempts_per_merchant_1h`**,
a dbt model over refund events feeding the feature store. Real work — a dbt model, a
schema pin, a backfill DAG run, and the amounts sanity suite (`mem-data-0104`) after.

While profiling a day of data she finds a hard problem: refund attempts spike ~4x in a
narrow window around **14:00 UTC**, clustered per merchant, with repeated attempts on the
same refund id seconds apart. Read naively, that is a textbook velocity-fraud signal, and
the feature she is about to ship will score it as one. She has to decide: **is this real
merchant behavior, a duplicate-event bug in her pipeline, or an artifact of how payments
behaves?** Every one of those has a different fix, and only one of them is hers.

## Definition of done
The model treats the 14:00 cluster as a **retry artifact, not a fraud signal** — it
deduplicates by refund id within the retry window rather than counting attempts, and the
window she picks reflects the cap that is actually in force **today**: 30s + jitter
(`mem-pay-0043`), not the 2s cap the burst was originally shaped by (`mem-plat-0107`,
superseded 2026-04-01).

Done, from Ingrid's POV: she does not open a Slack thread with payments and wait a day, and
she does not ship a fraud feature that fires on payments' own retry storm.

## What arm B already knows
`event-lake/CLAUDE.md` (`baseline.md`) is a good file. It knows the *shape* of the last
cross-team money bug:

> - **Amounts are integer minor units, by contract.** We once had a dbt model re-divide
>   already-normalized amounts and inflated every fraud feature 100x.

That is a genuinely well-tended baseline, and it will make arm B *look* alert to
payments-side semantics. But read what it actually contains: **it knows about amounts. It
knows nothing about retries.** There is no line in `event-lake/CLAUDE.md`, in
`.claude/rules/`, or in `~/meridian-standards/backend.md` about `refund-worker`, the PSP,
the 2s cap, the 30s cap, or the 14:00 settlement batch. Payments learned that in
`src-pay-007`, wrote it in *their* `CLAUDE.md`, and a repo-committed file **cannot cross a
repo boundary**.

The honest arm-B outcome is not "Ingrid gets it wrong." It is "Ingrid spends half a day and
a Slack interrupt on Ada's calendar to get it right." That interrupt is real cost, and it is
the cost Brainiac claims to remove.

## What only arm C could know
`mem-pay-0042` — *"refund-worker's default 2s retry cap causes timeout storms against
psp-gateway under PSP latency spikes"* — is **`visibility: org`**, and `qa-062` is this
journey almost verbatim: *"why do our checkout payment retries sometimes hammer the PSP?"*,
`asking_as: {team: team-data}`, gold `mem-pay-0042` @3. It is the corpus's single clearest
cross-team retrieval, and it is retrievable by a data principal *by design*.

The 14:00 settlement-batch detail is in the transcript (`src-pay-007`: *"At 14:00 the PSP
publishes settlement batches and their latency spikes to 8–12s"*) but **not** in either gold
memory — so arm C reaches it only via `memory_provenance` on `mem-pay-0042`, whose 500-char
source excerpt would carry it. Whether the agent thinks to make that second call is an
invocation question, not a retrieval one. Do not credit arm C with the 14:00 number unless
the agent actually fetched it.

## What we measure
**Primary (efficiency).** Turns, tokens, and — the number that matters here — **the Slack
interrupt.** Score arm B's true cost as *turns + one blocking question to another team*.
If the arm-B agent correctly says "I cannot determine this from this repo; ask payments,"
that is arm B behaving **well**, and its cost is a day of latency and an hour of Ada's, not
a wrong answer. Price it that way or the comparison is dishonest.

**Quality guardrail.** Does the feature dedupe or does it fire? Does the window match the
*current* cap?

**Cross-team utilization (H3-adjacent).** Of the memories arm C injects into a data
principal's bundle, how many are payments/platform memories she never uses? Count them.
Cross-team retrieval and cross-team *noise* are the same mechanism pointed at different
targets.

## How this could come out NEGATIVE for Brainiac
1. **The org slice is a sliver, and Brainiac only wins on the sliver.** `mem-pay-0042` and
   `mem-pay-0043` are `visibility: org` — lucky. The knowledge Ingrid *also* needs is not:
   `mem-pay-0073` (decline-05 is issuer-side, retrying burns quota — the other half of why
   the burst looks the way it does) is **`visibility: team`**, and `leak-005`/`leak-013`
   confirm the model actively forbids a data principal from reading payments' team slice.
   The extractor **defaults new memories to `visibility: team`** (`extract.rs:381-385`), so
   the corpus skews team-private by construction and every *future* payments learning will
   default to invisible to Ingrid. **Brainiac's cross-team win is confined to whatever
   someone remembered to mark `org` — which is exactly the discipline problem arm B has.**
   Say this plainly in the finding; it may be the most important sentence in the run.
2. **A Slack message costs zero tokens and comes with a human who can be asked a follow-up.**
   Ada would have answered in four minutes. If arm C's delta is "saved four minutes of Ada,"
   that does not pay for a Postgres, a worker, a BYOM bill, and Petra's review time.
3. **The answer may be in the data.** If the duplicate refund ids are visibly identical rows
   seconds apart, a competent analyst dedupes on sight without knowing *why*, and arm A ties.
   The knowledge only becomes load-bearing for the *window width* — check whether the width
   actually changes the feature's output before calling this a quality win.
4. **She may retrieve the pre-April world.** If the agent surfaces `mem-pay-0042` (the 2s
   pitfall, which has no `valid_to` and is still canonical) without `mem-pay-0043` (the 30s
   decision), Ingrid tunes her dedup window to 2s and it is **wrong for every event since
   April**. Arm C would have caused that. `mem-pay-0042` is a live memory describing a dead
   configuration, and nothing in the payload says so.
