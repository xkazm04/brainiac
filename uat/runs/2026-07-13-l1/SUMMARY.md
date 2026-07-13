# SUMMARY — Brainiac UAT, run 2026-07-13-l1 (L1 theoretical sweep)

**Roster:** 12 developers across Rust / Go / Python / TypeScript · **Units:** 8 journeys + 3 relays,
each judged in-Character against the arm-B baseline · **Level:** L1 only (code-grounded payload
models; no live server, no real agent sessions, no blind judge) · **Provider/embedder:** N/A at L1.

> **Read this line first.** Nothing here is a measured delta. L1 can speak only to *retrievable* —
> whether the store *could* return the right thing to the right principal. Whether the retriever
> ranks it, the agent reads it, and the code gets better are L2's, and only L2's. The value of an
> L1 sweep is that it is **cheap, exhaustive, and honest about which journeys are already lost** —
> and this one paid off: it located the product's real value, priced its harms from the code, and
> found two structural defects worth fixing before a single live session runs.

## The net-value verdict

**`not-yet` — and `harmful-as-shaped` on the governance axis.**

Brainiac, as shipped, is **not yet worth adopting over a well-tended `CLAUDE.md` stack**, and on one
axis it is actively harmful. This is not a dismissal — the value is real and the trial *located* it.
It is a statement that the value is narrower than the surface, and that two shipped choices convert a
promising product into a net-negative one until they change.

## The delta table (directional — L1)

| gap | journeys | how it came out |
|---|---|---|
| **none (H-null controls)** | refund-cap-tuning · decline-05-triage · ledger-payout-recon | **All three flat/redundant, as predicted.** 100% duplicate-of-baseline. The harness is trustworthy *because* these came out the way an honest control must. |
| **cross-team** | refund-burst-features · retry-reversal-propagation | **The real win — and it is genuinely arm-B-impossible.** But partial: the *number* crosses a team boundary; the *actionable detail* (the 14:00 window, the internal/external caveat) defaults to `team`-visible and does not. Outcome is "knew-to-ask," not "acted-on." |
| **retraction** | std-retry-reversal | **Real, but erasable for free** — one `CLAUDE.md` edit in the owning repo closes it. The durable win is only on the repos the owner *cannot* edit. |
| **after-the-file** | checkout-timeout-drift | **The cleanest positive case.** A live, org-visible, decision-changing fact arm B structurally cannot hold. Gated on an unprompted retrieval the agent has no reason to make, and hands over a confident half-contract. L2 blocked on unseeded web fixtures. |
| **provenance** | v1-fallback-still-true | **L1-fail.** Brainiac cannot answer its own fifth gap: no when, no who, no still-true in the payload. `git blame` wins in two seconds. |
| **permission** | contractor-webhook-scope | **L1-fail.** The visibility model has three tiers and a contractor needs a fourth. Best case is a tie bought with a Postgres; the shipped shape loses wider. |

## Where the value actually clusters (the product's real thesis, empirically located)

**Cross-team + after-the-file, and specifically the *retraction* of a fact that has already
propagated into another team's repo.** That is the one shape where a repo-committed file structurally
cannot compete: knowledge that must reach a repo whose owner doesn't know it changed. Everything
Brainiac does *inside a single team* is redundant against that team's own `CLAUDE.md`, at a real
cost. **If this were the product — cross-boundary propagation and retraction, and little else — the
scope to cut is large and the remaining claim is defensible.** The trial's recommendation is to build
toward that and stop selling the rest.

## The two defects to fix before L2 is even worth the tokens

1. **`memory_search` has no governance floor.** It serves `raw`, unreviewed extractions alongside
   `canonical` ones. This means the entire review queue — the product's beating heart, the maintainer's
   afternoon — buys a guarantee the agent's most-used tool ignores. **Add a `min_status` floor or an
   explicit ungoverned-content warning.** Single highest-leverage change in the run. (`memories.rs:198`)

2. **The tool descriptions throw away Brainiac's one structural advantage.** `memory_context` says
   *"Call once when starting work"* — front-load-then-decay, which is arm B's exact failure mode plus a
   bill. The only mechanism where Brainiac beats a text file — just-in-time retrieval against the
   within-session decay curve — requires a *mid-session* call the surface never prompts. **Rewrite the
   descriptions to induce mid-task retrieval at decision points.** A prompt change, not an architecture
   change. (`mcp.rs:308-309`)

## The Harm Ledger, priced against the wins (full detail in `harm.md`)

The debit column is heavier than the delta column. **H7 redundancy** is the dominant outcome for
single-team work (100% on the controls). **H5 governance-tax-to-abandonment** is structurally
over-determined: capture is unthrottled, review is human-rate-limited, no dwell-time is recorded so
rubber-stamping is invisible, and the buyer (Dana) can neither see the store — RLS truncates her
analytics to near-zero — nor clear the queue (she's a maintainer of no team). **H4 leak** has a real
verbatim-transcript exposure with no redaction anywhere. **H8 false-confidence** is structural: the
payload cannot say who/when/still-true. Against these, the wins in the cross-team column are real but
narrow, and on today's code they do not outweigh the tax.

## The governance tax, in the maintainers' own terms

Modeled P2 load is ~40–70 human decisions; Petra's budget is ~15/week and Lars's is 20 minutes.
The queue crosses both disengagement thresholds *within the first sprint*, before the P3 consumers
ever read what P2 was supposed to certify. Their own SLO (median review < 48h) is unmeetable at that
inflow with two people. Lars — the skeptic who is also the platform maintainer — will not announce he
has stopped; he will simply stop opening the queue, and per the model nobody notices for a month.
This is the `harmful-as-shaped` path, and the shipped code cooperates with it at every step.

## Strengths worth protecting (do not "fix" these)

- **Nothing auto-promotes to canonical, ever** — poison needs a human hand.
- **`memory_context` fails safe (empty) under abandonment** — the Canonical floor is doing real work.
- **RLS is correct; the 15-case leak gate is green,** including the maintainer-vs-private traps.
- **Temporal supersession works** — the dead side of every closed chain is excluded.

## Which segments Brainiac is winning vs losing

- **Winning (conditionally):** the cross-team consumer who doesn't own the knowledge and can't see the
  other repo — Ingrid (data), Jonas (web). This is the whole product.
- **Losing:** every single-team author (redundant), the on-call engineer (latency + poison risk under
  pressure), the contractor (no fourth tier), the staff engineer and the EM (no cross-team seat).
- **The buyer is losing hardest,** which is the commercial problem: Dana funds it, cannot see it,
  cannot fix it, and the one metric she asks for — *is anyone still using this in month three* — the
  console structurally cannot show her.

## Handoff to L2 (in priority order)

1. **P0 — the poisoning behavior** (`new-joiner-inherits-p2`): does the `contradict` worker open the
   D1 row so `⚠ CONTRADICTED` fires? Drive Mira against both the fired and not-fired states. Drive an
   agent as a `memory_search` consumer against the raw D2 decoy.
2. **P0 — the queue economics** (`promotion-queue-backlog`): **do not script the approvals.** Time
   real maintainer decisions across P2→P4; record depth at each barrier and per-item wall-clock;
   assert monotonic growth and sub-5s medians. Call `/v1/analytics` as `user-em` and record the
   empty-store-next-to-backlog contradiction.
3. **P0 — the decay thesis** (`ledger-payout-recon`): count spontaneous mid-session `memory_search`
   calls across 3× sessions. If zero, the sharpest thesis is refuted by the surface, and the fix is a
   description change.
4. **P1 — the leak** (`contractor-webhook-scope`): mint Rafael a scoped `brk_` token (never the env
   map), plant a credential in a transcript, drain, then read the `memory_provenance` excerpt back.
5. **Prerequisite — `fixtures/v2`:** the web team, `checkout-web`, and any stack-specific corpus.
   Without it, `checkout-timeout-drift`, the web link of `retry-reversal-propagation`, and all of H3
   cannot run at L2.

## The panel verdict (the one sentence the 12 voices add up to)

*"It told me what my own repo already told me, slower and with a bill — except for the one time it
told me something my repo couldn't, and even then it wouldn't tell me who said it or whether it was
still true, so I went and asked a human anyway."*
