---
id: checkout-timeout-drift
gap: after-the-file
hypotheses: [H-cross, H-eff, H-qual]
characters: [jonas-weber]
phase: P3
promotion: discovery
fixture_anchors: [mem-pay-0064, mem-pay-0063, mem-pay-0022, mem-pay-0071, mem-pay-0079, con-001, tmp-008]
---
# checkout-web still thinks the PSP answers in ten seconds

## The task
Jonas is building the **payment-pending state** in `checkout-web`: the user hits Pay, the
request goes to `payment-service` via `lib/payments.ts`, and the UI has to decide how long to
wait, what to show while waiting, and what to do when the wait ends without an answer.

He writes the obvious thing. `lib/payments.ts` has an existing `PAYMENT_FETCH_TIMEOUT_MS` of
15s — comfortably above the PSP's 10s client timeout — so a slow payment surfaces as a clean
"we couldn't reach the provider" error and the user retries. He extends that pattern, adds a
spinner, adds an abort, and ships.

**The 10s number is dead.** `mem-pay-0063` (*"psp-gateway client timeout is 10 seconds"*) has
`valid_to: 2026-05-01`, `status: deprecated`, `superseded_by: mem-pay-0064` — *"psp-gateway
client timeout raised to 30 seconds after the PSP incident review."* Since May, a payment
that takes 22 seconds is a payment that is **still going to succeed**. Jonas's 15s client
abort now fires *on live, in-flight, about-to-succeed payments*, shows the user an error, and
invites them to pay again. He is building a **double-charge funnel**, and every file he can
read tells him he is right.

## Definition of done
The pending state tolerates the **real** upstream envelope (30s + margin), does not abort
in-flight payments, and — critically — **does not offer a naive retry** on a timeout it can no
longer distinguish from success. Ideally it reconciles against payment state rather than
guessing (`mem-pay-0068`: `payment-service` is the system of record).

Done from Jonas's POV: he ships the pending state without ever thinking to ask payments a
question, because he did not know there was one.

## What arm B already knows
`checkout-web/CLAUDE.md` (`baseline.md`) is *accurate about everything it covers*:

> ## The payments API
> - Base: `/v1/payments`. Contract lives in payment-service's `openapi.yaml`.
> - Checkout v2 is the live flow. v1 endpoints are frozen.

Both statements are still true (`mem-pay-0022`). The file is not stale, it is **incomplete** —
and it is incomplete in the specific way `baseline.md` predicted when it wrote this repo's
entry: *"This file records the payments API contract as Jonas last understood it. When
payments changes something mid-sprint, nothing tells `checkout-web`. A repo-committed file
cannot cross a repo boundary."*

Note what makes this the *after-the-file* gap rather than the *retraction* gap: **no line in
Jonas's file is wrong.** There is nothing to retract. The knowledge simply **arrived after the
file was written, in another team's repo, and no mechanism existed to carry it across.** No
amount of arm-B diligence closes this, because Jonas has no way to know he should be
diligent about it. He would have to re-audit another team's timeouts on a schedule, forever,
for a change he has no reason to suspect. **Nobody does that. That is the gap.**

The symlinked `~/meridian-standards/backend.md` carries org *conventions* (ArgoCD, OTel,
Vault, minor units) — not another team's operational parameters. It never mentioned 10s and
it never mentioned 30s.

## What only arm C could know
`mem-pay-0064` is **`visibility: org`**. A `memory_context("payment request timeout pending
state")` from `checkout-web` should surface *"psp-gateway client timeout raised to 30 seconds
after the PSP incident review"* — a fact that lives in **payments' incident review**, in
**payments' repo's** blast radius, and in **no file Jonas will ever open.** `tmp-008` asserts
it at rank 1 as-of 2026-06-01; today is 2026-07-13.

This is the cleanest positive case in the whole run: **org-visible, temporally correct,
decision-changing, cross-repo, and genuinely unreachable by arm B.** If Brainiac does not win
this journey, it does not win anything.

## What we measure
**Primary (quality — and this is the one journey where quality is the endpoint, not the
guardrail).** The two conditions from skill.md are met and should be checked explicitly before
the run: (a) the task has a **quality ceiling well below 90%** — an agent reading only
`checkout-web` will confidently ship 15s, because 15s is *correct* given everything in the
tree; and (b) **the discriminating knowledge is not recoverable from the repo** — the number
lives in another team's memory, and `checkout-web` does not vendor `payment-service`'s config.

Binary: **what timeout shipped, and does a timeout offer a retry?**

**Secondary (efficiency).** Turns. Expect arm B to be *fast and wrong* again — the same trap
as `std-retry-reversal`. **A repo whose files are internally consistent produces confident,
efficient, incorrect work, and the efficiency table will reward that.** Never report the
efficiency delta on an after-the-file journey without the quality column beside it.

## How this could come out NEGATIVE for Brainiac
1. **The web team does not exist in the fixture.** `company.md` is blunt: `checkout-web` and
   `user-web-dev1` are a **trial-only extension**; there is no web team in `org.yaml`, no
   `checkout-web` repo, and **zero TypeScript anywhere in `fixtures/v1`.** So: Jonas's
   principal must be minted with **org-only visibility** (he is in no fixture team), which is
   *exactly right* for this journey — but it also means **the L2 run needs new material that
   does not exist yet.** L1 on this journey is real and citable. **L2 is blocked on `fixtures/v2`
   and the report must say so rather than quietly running a hand-made payload.**
2. **Half the answer is invisible to him — and it's the dangerous half.** `mem-pay-0071` (the
   adapter normalizes decline codes into `psp.decline_code`) and `mem-pay-0079` (autofill
   double-fires tokenization) are **`visibility: team`, team-payments.** An org-only principal
   **cannot see them.** So arm C hands Jonas the timeout number and *not* the decline-mapping
   contract his `declineCopy` depends on. **A partially-informed agent that now believes it
   has the payments contract is more dangerous than one that knows it doesn't** — it will stop
   asking. Watch for arm C's agent asserting confidence it has not earned.
3. **`openapi.yaml` may already say it.** `checkout-web/CLAUDE.md` points at
   `payment-service`'s `openapi.yaml` as the contract. If that spec is vendored, generated, or
   fetched in CI **and it carries the timeout**, then arm A finds the 30s by reading a
   generated file, arm C's win evaporates, and the correct finding is: *"the cross-repo gap was
   closable by publishing a contract artifact — which the team should do anyway, and which
   costs no Postgres."* **Check the repo for a vendored spec before claiming this win.** It is
   the single most likely way this journey's headline result turns out to be an artifact of a
   missing build step.
4. **Nobody asks the question.** Jonas has no reason to call `memory_context` about a timeout
   he does not know changed. If the agent only queries org memory for things it already
   suspects, **Brainiac's after-the-file gap is unreachable in practice** — you cannot retrieve
   the answer to a question you do not know to ask. The only thing that saves arm C here is an
   **unprompted session-start `memory_context(task_hint)`** broad enough to pull the timeout in
   from a task hint about a pending state. **That is a retrieval-recall problem, not a
   knowledge problem, and it is the thing to watch.**
