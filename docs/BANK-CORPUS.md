# The bank corpus — what a real org's knowledge actually looks like

**Why this exists.** Every surface in this console was designed against Meridian
the fintech: 9 users, 3 teams, 81 memories, 2 divergences, 0 contradictions. At
that size every list fits on a screen, so no module has ever had to answer the
question a customer will ask on day one — *what does this look like when we have
five years of it?* This document is the shape we simulate, and the reasoning
behind the shape.

**The org.** Meridian, grown up: a licensed retail + commercial bank. Same name,
same fixture lineage, twelve teams instead of three. A bank is the right stress
case because it is the org type where knowledge governance is not a nice-to-have —
regulated, audited, and organised into domains that genuinely disagree with each
other.

---

## 1. The teams

| team | domain | what it knows |
| --- | --- | --- |
| `core-banking` | core | accounts, postings, interest accrual, end-of-day batch |
| `payments` | money movement | SEPA, instant payments, SWIFT, PSP integration |
| `cards` | money movement | issuing, 3DS, scheme rules, chargebacks |
| `channels` | front | mobile app, internet banking, public API |
| `lending` | products | origination, underwriting, servicing, collections |
| `deposits` | products | savings, term deposits, rate changes |
| `risk` | control | credit scoring, IFRS9, exposure limits |
| `fincrime` | control | AML, sanctions screening, fraud monitoring, KYC |
| `compliance` | control | PSD2, GDPR, regulatory reporting, audit trails |
| `data` | platform | warehouse, feature store, streaming |
| `platform` | platform | Kubernetes, CI/CD, service mesh, DR |
| `security` | platform | IAM, secrets, pentest, key management |

---

## 2. Seven principles of a balanced corpus

The temptation when generating scale data is to make it uniform: 100 memories per
team, evenly spread. That produces a corpus no design decision can be made
against, because every real problem in this product comes from **imbalance**.

**1 · Volume follows a power law, not a headcount.**
`payments` and `core-banking` carry ~30% of the corpus between them; `security`
and `compliance` carry ~4% each. Money movement generates incidents, and
incidents generate memories. A design that only works when the teams are equal
sized is a design that has never met an org.

**2 · A few entities carry most of the cross-team gravity.**
The ledger, the customer record, the payment rail, the card scheme, the KYC
decision — five entities that a dozen teams all touch and all name differently.
This is where canonical binding earns its existence, and where the graph either
reads as a constellation or as a hairball. The long tail of entities is touched
by exactly one team and is boring by design.

**3 · Knowledge has domain-specific half-lives.**
A regulatory fact ("PSD2 SCA exemption thresholds") is true for years. An
incident pitfall ("the PSP spikes at 14:00 UTC on settlement batches") is true
for months. A runbook step is true until the next migration. The disputes bench
and the archive's as-of axis only make sense if the corpus has all three.

**4 · Visibility is a pyramid, and the top is the interesting part.**
~70% team-visible, ~28% org-visible, ~2% private. The org-visible slice is not
"the important memories" — it is *the contracts between teams*. That is why the
review gate's cross-team rule exists, and why RLS has something to hide.

**5 · The queue is never empty and never enormous.**
~80% canonical, ~8% candidate, ~5% raw, ~7% deprecated. A bank with 1,200
memories has tens in review, not hundreds — unless nobody has worked the queue
for a quarter, which is exactly the state the reviews rail was designed for.

**6 · Contradictions are rare and cluster on boundaries.**
Not scattered randomly: they sit where two teams own two halves of one truth.
`payments` vs `core-banking` on when a payment is *posted* versus *settled*.
`cards` vs `fincrime` on whether a chargeback hold outranks a fraud hold.
`lending` vs `risk` on which score is authoritative. ~40 across 1,200 memories.

**7 · Divergence is cross-cutting practice, not domain knowledge.**
Retry policy, idempotency TTL, PII in logs, deploy approval, secret rotation —
every team solved each one, none of them talked. This is the only thing in the
corpus that is *supposed* to be everywhere at once.

---

## 3. Targets

| thing | count | shaped by |
| --- | --- | --- |
| teams | 12 | §1 |
| users | 36 | 3 per team (2 members, 1 maintainer) |
| memories | **1,200+** | power law over teams (§2.1) |
| entities | ~420 raw → ~260 canonical | five hubs + a long tail (§2.2) |
| supersession chains | ~90 | gives the archive something to resurrect |
| contradictions | ~40 | boundary clusters (§2.6) |
| documents | **1,050+** | §4 |

## 4. The knowledge base

The schema allows exactly four kinds (`documents_kind_check`), which is fewer
than the taxonomy this section first proposed — `decision_record` and
`policy_page` were invented here and do not exist. The real four, and what each
carries in a bank:

| doc_kind | count | binding |
| --- | --- | --- |
| `entity_page` | ~380 | one per canonical entity — the service, the concept, the hub |
| `topic_page` | ~400 | the cross-cutting reads: a practice, a decision, a regulation |
| `runbook` | ~260 | per service operational procedure |
| `onboarding` | 12 | one per team — the "start here" |

Slugs are namespaced by domain (`payments/psp-gateway`,
`core-banking/ledger-posting`) because 1,050 flat slugs is not an index, it is a
phone book.

**A note on how these are seeded, so nobody mistakes them for the real thing.**
A page in this product is a *projection* — composed by an LLM from the canonical
memories bound to it (`compose_sweep`). Composing 1,050 pages would be 1,050
model calls, which is not a thing to do for a density test. So the simulation
seeds pages the way the composer would leave them: rows in `documents` with their
`document_sections` bindings, a `document_revisions` body, and a realistic mix of
`draft` / `published` / dirty. The bindings are real and the counts are real; the
prose is generated, not composed. Every module that lists, filters, paginates or
navigates pages is therefore under genuine load — the only thing not exercised is
the composer itself, which has its own eval profile (`--profile docs`).
