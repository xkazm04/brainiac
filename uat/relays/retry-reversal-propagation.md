---
id: retry-reversal-propagation
gap: retraction
hypotheses: [H-retract, H-cross, H-eff]
characters: [tomas-reid, ada-kovac, petra-novak, lars-bengtsson, ingrid-sol, jonas-weber]
phase: P2→P4
promotion: discovery
fixture_anchors: [mem-plat-0107, mem-pay-0043, mem-pay-0042, mem-pay-0064, mem-plat-0121, con-003, qa-061, src-pay-007]
---
# The reversal has to reach four repos, and only one of them belongs to the team that made it

## The task
A policy the whole org depends on has been reversed, and **the reversal was made by the wrong
team to be able to announce it.** `mem-plat-0107` — *"std-retry policy: retry cap 2 seconds,
3 attempts"* — is platform's org-wide policy. It was superseded by **`mem-pay-0043`**, a
**payments** decision (30s + jitter, valid from 2026-04-01), taken in a payments incident
session (`src-pay-007`). Platform never edited `infra-live/CLAUDE.md`. Payments wrote it in
their own `CLAUDE.md` and moved on.

Three months later (today: 2026-07-13), four repos hold four different beliefs about the same
number, and every one of them is internally consistent. This relay follows the correction as
it tries to travel: **platform → payments → web + data**, across three sprint phases and a
human review gate.

**A single developer cannot run this journey. That is the entire point.** The knowledge is
produced in P2 by people who will never meet the people who need it in P3, and no file any of
them can edit is visible to all of them.

## Definition of done
By the end of P3, **Ingrid's dbt dedup window and Jonas's client timeout are both consistent
with the 30s + jitter world**, and Tomas's new Rego rule distinguishes the internal-call cap
(2s, still correct) from the external-provider cap (30s). Nobody sent a Slack message.

By the end of P4, **Lars — the maintainer who paid for it — can see that it worked.** If he
approved four promotions and cannot tell whether anything downstream changed, the flywheel
has no gauge, and he will stop turning it.

## Chain

| # | Phase | Character | Session (a real coding task) | What it PRODUCES | What the NEXT link needs from it |
|---|---|---|---|---|---|
| 1 | **P2** | **Tomas Reid** (platform) | Reconcile `policies/retry.rego` with reality: the Rego still encodes a flat 2s/3-attempt cap for everything. He splits it — internal calls stay 2s, external-provider calls get 30s + jitter — and updates the conftest suite. | A **new, org-visible clarifying memory**: *"the std-retry cap parameter is 30s + jitter for external-provider calls; the 2s cap applies only to internal service calls."* This is the reconciliation nobody has ever written down — `mem-plat-0107` and `mem-pay-0043` read as a flat contradiction (`con-003`) precisely because this distinction is missing from the corpus. | Links 4 & 5 need **the distinction, not just the number.** An agent handed only "the cap is 30s" will over-apply it to internal calls, which is the `std-retry-reversal` journey's named quality regression. This link exists to prevent the relay from *causing* that. |
| 2 | **P2** | **Ada Kovac** (payments) | Ship the `RetryPolicy` struct in `refund-worker` (the `refund-cap-tuning` task) against a live PSP latency spike. | The **operational envelope**: 30s + jitter, and the *why* — PSP settlement batches at **~14:00 UTC**, latency 8–12s. The 14:00 detail exists today **only inside `src-pay-007`'s transcript**, not in any gold memory — so this link's job is to promote it from a provenance excerpt into a first-class, retrievable memory. | Link 4 (Ingrid) needs **14:00 and the window width** to size her dedup — the number alone is useless without the time-of-day cluster. Link 5 (Jonas) needs only the number. |
| — | **BARRIER** | **Petra Novak** (payments maintainer) + **Lars Bengtsson** (platform maintainer) | **Drain + review.** `GET /v1/queue/health` must show the pipeline drained, then the promotion queue must be **actually worked**: Petra approves/rejects Ada's candidates, Lars approves/rejects Tomas's. **Time every decision.** | Two memories at **`canonical`**, org-visible, with the supersession resolved rather than left as an open contradiction. | **Everything downstream. This is the load-bearing link and it is a human.** See "If the maintainer does not review in time" below — the failure here is not that the chain stops. It is that it *keeps going, ungoverned*. |
| 3 | **P3** | **Ingrid Sol** (data) | The `refund-burst-features` task: dedupe the 14:00 refund cluster in a dbt fraud feature. | A correct dedup window, and a **data-team memory that the burst is a payments retry artifact** — closing the loop back toward payments. | Nothing downstream needs it. This is a **terminal consumer**, and that is what makes it a clean measurement point: `C − B` here is unpolluted by anything Ingrid produces. |
| 4 | **P3** | **Jonas Weber** (web) | The `checkout-timeout-drift` task: the payment-pending state in `checkout-web`. | A client timeout consistent with a 30s upstream envelope, and no naive retry on a timeout that is no longer a failure. | Terminal consumer. **The strictest test in the relay**: Jonas is in *no fixture team*, so he sees only the **org** slice. If link 1's or link 2's memory was promoted at the extractor's **default `visibility: team`** (`extract.rs:381-385`), **Jonas receives nothing and the relay silently breaks at its last link.** |
| 5 | **P4** | **Lars Bengtsson** (platform maintainer, skeptic) | Audit: *"I approved four things. Did anything change?"* Query the store as the maintainer; diff what P3 actually shipped against what P2 promoted. | The **adoption verdict**, in the voice of the person who pays the tax. | The report. Lars's scored criteria are set to *beat a text file*, and his honest answer here is the run's most valuable single artifact. |

## The barrier
**Between P2 and P3: queue drained + review worked.** Both, and in that order. The queue is
drained when `GET /v1/queue/health` reports the extract→embed→resolve→contradict→promote
pipeline idle. **Draining is not reviewing.** The barrier is only satisfied when Petra and
Lars have made a real decision on every promotion — and "real" is measured, not asserted:
**approve latency under ~3s per item is a rubber-stamp, not a review**, and it must be logged
as one.

## If the maintainer does NOT review in time
This is the abandonment failure mode, and Brainiac's shipped code makes it **worse than a
clean break** — which is the finding, and it is worth stating precisely:

- **`memory_context` has a Canonical floor** pushed into the SQL candidate stage
  (`mcp.rs:616`). An unreviewed memory is **`raw`**, and therefore **invisible** to the
  session-start bundle. Ingrid and Jonas open P3 and their context bundle contains **nothing
  from P2.** The relay's chain is severed, silently, with no error and nothing red.
- **`memory_search` has no such floor** — it excludes only `rejected`. So the *same* unreviewed
  memory **is** served to any agent that reaches for search mid-task.
- Therefore an unworked queue does not stop the knowledge. **It converts Brainiac from a
  governed store into an ungoverned one, non-deterministically, depending on which tool the
  agent happened to call.** Ingrid's agent calls `memory_search` and gets the raw, unreviewed,
  possibly-wrong reconciliation. Jonas's agent relies on `memory_context` and gets nothing. Two
  developers, same store, same moment, incompatible worlds.
- And the **fallback is arm B**, which still says **2s**, in `infra-live/CLAUDE.md`, with total
  confidence. **The cost of an unworked review queue is not "no benefit." It is a silent
  reversion to the stale truth, now with a memory system on the invoice.**

Instrument it: **queue depth over time, time-to-review, approve latency, and — the one nobody
measures — whether any promoted memory was ever *retracted*.** Their own SLO is median
promotion review **< 48h or the flywheel dies** (ARCHITECTURE §7). Find out what it actually
is when the maintainer is Lars and Lars thinks this is a wiki with extra steps.
