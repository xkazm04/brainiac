---
id: promotion-queue-backlog
gap: none
hypotheses: [H-null]
characters: [petra-novak, lars-bengtsson, dana-brecht, yusuf-kaya, ada-kovac, mira-haddad, nadia-roth, tomas-reid]
phase: P2→P4
promotion: discovery
fixture_anchors: [con-005, con-006, con-007, con-008, con-009, con-010, con-011, con-012, con-003, mem-pay-0043, mem-plat-0107]
---
# The queue is the product. This relay is the one where nobody works it.

> **`gap: none` is deliberate and it is honest.** This relay tests **none of the five knowledge
> gaps**, and none of the six hypotheses is really about it either — `H-null` is listed because
> it is the closest (arm C costs more), but the truthful statement is: **this relay's target is
> the Harm Ledger — H5 (governance tax → queue abandonment), H6 (capture friction), H7
> (redundancy) — and skill.md's own framing that "the review queue is the product's heart and
> the first thing a real org abandons."** Forcing it into a hypothesis it does not fit would be
> exactly the kind of tidy dishonesty this skill exists to prevent.
>
> Every other journey in this run asks *"is the knowledge good?"* This one asks **"in month
> three, is there still a human doing this?"** — and that question decides the product's fate
> more surely than any NDCG.

## The task
Nothing here is a coding task. **That is the finding waiting to happen.**

Brainiac's auto-capture genuinely solves H6: writing a memory is not a chore, because the
session ingest does it for you. **And that is precisely what breaks it.** Automating capture
**industrializes the production of candidate memories**, and every industrialized candidate
lands in a queue that terminates in **one tired human**. The friction did not disappear. It
**moved**, from the many to the few, and it landed on the person least able to refuse it and
most able to quietly stop.

Meridian's P2 is a busy sprint. Ada ships a retry policy. Tomas reconciles a Rego. Nadia works
a 02:00 incident. Mira lands her first PR. **Four sessions, ingested, extracted, resolved,
contradiction-checked.** The pipeline does its job beautifully. And on Wednesday morning
**Petra opens a queue with dozens of items in it, and she has a sprint of her own.**

## Definition of done
There isn't one, and *that is the point of running it.* The success condition is not "the queue
is empty." It is: **the queue was emptied by a human who actually read the items, within the
SLO, and would do it again next week.**

Concretely, done means all four hold:
1. Median promotion review **< 48h** — *their own SLO* (ARCHITECTURE §7), by which they say the
   flywheel dies.
2. **Approve latency is not a rubber-stamp.** A reviewer clearing an item in 3 seconds is not
   reviewing, and a queue "drained" that way is worse than a queue abandoned, because it
   manufactures `canonical` — the highest-authority tier in the system — **without a human
   judgement behind it, while looking exactly like one that has.**
3. **Precision holds.** The contradiction queue is where reviewers get *trained to ignore the
   queue*: `fixtures/v1/contradictions/cases.yaml` ships **12 cases, of which 8 are deliberate
   negatives** — 4 `coexist` (`con-005`: 30s client timeout vs 30s retry cap — *different knobs
   that happen to share a number*; `con-009`: bge-m3 for search vs nomic-embed for clustering —
   *different scopes*) and 4 `dismissed`. **If Petra's queue is 8 false alarms for every 4 real
   supersessions, she will learn — correctly, and within two weeks — that the queue is noise,
   and she will start clearing it without reading.** That is not a hypothetical; it is the
   documented failure mode of every alerting system ever built.
4. **A retraction is possible at all.** Has any promoted memory in this run's history ever been
   *walked back*? If the answer is no, and there is no path to yes, **the store rots exactly
   like Confluence — and documentation rots silently. Nothing goes red.**

## Chain

| # | Phase | Character | Session | What it PRODUCES | What the NEXT link needs from it |
|---|---|---|---|---|---|
| 1 | **P2** | **Ada, Tomas, Nadia, Mira** (4 parallel sessions) | Their real coding tasks from the other journeys. **They are not doing governance work and they do not know they are generating any.** | **Queue load.** Every session ingests → extracts → contradicts → promotes. Count the candidates. This number *is* H6-solved-into-H5: **capture friction ≈ 0, and the review load is exactly proportional to how well capture worked.** | Link 2 needs the pile. **Its size is the independent variable of the entire relay.** |
| 2 | **P2/P3** | **Petra Novak** (payments maintainer) | Work the promotion queue: `GET /v1/reviews/promotions` → approve/reject each. Work the contradiction queue: `/v1/reviews/contradictions` → resolve. **Time every single decision.** | Canonical memories — or a backlog. And a **per-item wall-clock series**, which is the primary dataset of this relay. | Link 4 (the P3 consumers) needs `canonical` rows. **Petra is a single point of failure for the entire product**, and this is the link that measures it. |
| 3 | **P2/P3** | **Lars Bengtsson** (platform maintainer, **and the skeptic**) | The same queue, platform's half. Lars's scored criteria are set to *"convince me this beats a text file."* | Either approvals, or **a documented refusal.** | **Lars is the honest test, and he is one person, not two.** `company.md` fuses the maintainer and the skeptic on purpose: *"the person who pays the governance tax is the person most likely to doubt it's worth paying."* **If Lars declines to work the queue, that is not a character quirk — it is the modal real-world outcome, arriving on schedule.** |
| 4 | **P3** | **Ingrid, Jonas, Mira** | The consumer journeys. | Their PRs — **and, as a side effect, the answer to whether links 2–3 mattered.** | Link 5 needs the outcome: did the reviewed knowledge reach anyone? |
| 5 | **P4** | **Yusuf Kaya** (security) | Audit what got promoted to `canonical` **without a real review.** Cross-check every rubber-stamped approval against the decoy from `new-joiner-inherits-p2`. | **Did an unreviewed, wrong, or credential-bearing memory acquire canonical authority?** | The Harm Ledger. |
| 6 | **P4** | **Dana Brecht** (EM — **the buyer**) | Read the analytics. Queue depth, time-to-review, hours of maintainer time consumed, memories actually used downstream. **Decide whether to keep paying.** | The **net-value verdict**, in the voice of the person who signs the invoice: `adopt` · `adopt-with-changes` · `not-yet` · `harmful-as-shaped`. | `SUMMARY.md`. **Dana's answer is the run's answer.** |

## The barrier
**Two barriers, and this relay is the only one that treats them as the object of study rather
than as plumbing.**

- **P2 → P3: queue drained + review worked.** `GET /v1/queue/health` idle **and** every
  promotion decided by a human. **Do not let the run "satisfy" this barrier by scripting the
  approvals.** A scripted approval is a fabricated maintainer, and it would make every
  downstream `C − B` in the entire trial a lie — the relay would be measuring a store that a
  perfect, tireless, instantaneous reviewer had curated, which is not a store any customer will
  ever have.
- **P3 → P4: the second wave.** P3's own sessions generate *their own* candidates. **The queue
  refills while she is still draining it.** Measure the depth at the P3→P4 barrier against the
  depth at P2→P3. **If it is monotonically increasing, the product has a structural leak and
  the trial found it in one simulated sprint.**

## If the maintainer does NOT review in time
**This relay is the one where that is not a risk to be mitigated — it is the treatment
condition.** Run it deliberately: let Lars refuse, or let Petra fall two days behind her own
48h SLO, and observe the system honestly.

- **The chain does not break cleanly. It breaks into two incompatible halves**, and which half
  a developer lands in is decided by which MCP tool their agent happened to reach for.
  `memory_context` enforces a Canonical floor (`mcp.rs:616`) and returns **nothing**;
  `memory_search` filters only `rejected` and returns **everything, raw and unreviewed**.
  **The governance step that an agent's main search tool actually enforces is: none.**
- **`status` is not in the rendered payload** (`mcp.rs:638-661`). A `raw` memory and a
  `canonical` one are **typographically identical** in the agent's context window. So the
  reviewer's work — the thing the whole tax buys — **is invisible at the point of consumption.**
  Nobody downstream can tell whether Petra reviewed it. Including Petra.
- **Rubber-stamping is indistinguishable from reviewing, from every angle except the clock.**
  This is why approve-latency is a primary metric and not a curiosity. **It is the only
  observable difference between a governed store and a store that has been *declared* governed.**
- **And the fallback, once again, is arm B** — the `CLAUDE.md` files, which are stale, which are
  wrong about std-retry, and which are still *sitting there working*, at zero cost, having asked
  nobody for anything.

**The failure mode this relay is hunting is not "Brainiac returns bad results."** It is:
**month three, the queue has 400 items, Petra has stopped opening it, `memory_search` is
serving raw extractions to every agent in the company with the full visual authority of a
governed system, and nothing anywhere has gone red.** If the run reproduces that in four
simulated phases, the verdict is **`harmful-as-shaped`**, and the run has done its job.
