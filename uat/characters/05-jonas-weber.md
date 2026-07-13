---
name: Jonas Weber
principal: user-web-dev1
team: team-web
stack: TypeScript (Next.js 15, React 19)
repos: [checkout-web]
role: member
reachable_surfaces: [mcp:memory_search, mcp:memory_context, mcp:memory_add, mcp:entity_lookup, mcp:knowledge_propose, mcp:memory_feedback, mcp:memory_provenance, rest:GET /v1/queue/health]
language: en
---

# Jonas Weber — frontend engineer, checkout

# ⚠ Team `team-web` and repo `checkout-web` are a **trial-only extension** — they are not in
# `fixtures/v1`. Every journey involving Jonas needs new seeded material, and any result that
# depends on fixture content he cannot reach must be reported as unseeded, not as a miss.

## Background / voice
Jonas ships the card form that 100% of Meridian's revenue passes through, and he finds out
that payments changed the API the way everyone on a frontend team finds out: a 422 in Sentry
at 09:40. He's cheerful about it in a way that is actually a coping mechanism — "ah, cool, so
that field's an enum now, love that for us" — and he keeps a private list of every time the
backend "didn't change anything." He is fast, pragmatic, and does not want to read a graph;
he wants the contract. He is the person most likely to *want* Brainiac to work and least
likely to have set it up correctly.

## Job to be done
Wire the checkout UI to a payments endpoint whose contract changed after his `CLAUDE.md` was
written — and catch the change before it becomes a 422 storm during a settlement window.

## Current memory practice — THIS IS THEIR ARM B
Good and honestly maintained. `checkout-web/CLAUDE.md` gives commands (`npm run dev`, vitest,
lint as a CI gate, `npm run typecheck` clean before commit), conventions that a linter can't
catch (**never format money client-side — the API returns minor units, use `formatAmount()`**;
all payment calls go through `lib/payments.ts`, no direct fetch; card form state is owned by
`useCardForm`, don't lift it), a `## The payments API` section (base `/v1/payments`, contract
lives in payment-service's `openapi.yaml`, checkout v2 is live and v1 endpoints are frozen),
and Gotchas that already know autofill double-fires tokenization and that the decline code
shown to a user is **not** the PSP's raw code — map through `declineCopy`. He has
`~/meridian-standards/backend.md` symlinked and auto-memory on.

**The structural limit, stated exactly:** his file records the payments contract *as he last
understood it*. It points at `openapi.yaml` — in a repo he does not have checked out. When
payments changes something mid-sprint, **nothing tells `checkout-web`**. A repo-committed file
cannot cross a repo boundary, and no amount of maintenance discipline fixes that, because the
person who would have to maintain it is not the person who knows.

## Decision-delta bar
It must tell him something about the **payments contract or its semantics** that his file
does not already say and that he cannot get from the repo he has open. Concretely: a field
changed type, a decline code's meaning moved, an endpoint was versioned, a response now
returns something new. Anything he could learn by reading `lib/payments.ts` scores **zero**.
"Be careful with the API" scores less than zero — it is noise that cost tokens.

## Trust bar
**Low-to-medium, and that is a hazard.** Jonas will act on a plausible-sounding contract fact
without verifying, because verifying means asking payments and waiting a day, and he has a
sprint. He is the second-most-poisonable Character after Mira. What he *would* want, and does
not get, is a date: "when did this change?" The payload has no date, no author, no session id
— just `via <actor>`. So a stale contract memory and a current one look **identical** to him,
and he will pick whichever is on top.

## Toil tolerance
He'll pay 2-3 seconds for a `memory_context` at session start and one `memory_search` when he
hits a surprise. He will not read a 10-item bundle; he'll read two. He has never opened a
review queue in his life and never will — which is fine, he is not a maintainer, but it means
**every memory he adds is someone else's review debt** (charge it to Petra's H5 budget).

## Scored acceptance criteria
1. **After-the-file retrieval:** a payments-owned memory that post-dates
   `checkout-web/CLAUDE.md` is retrieved and is relevant to the contract he is wiring. Zero =
   the `after-the-file` gap is unproven for him.
2. **Baseline-impossibility check:** the winning memory is verified **absent** from
   `checkout-web/CLAUDE.md`, his `.claude/rules/`, and the org symlink. Present anywhere = the
   win is void (H7).
3. **Staleness discrimination:** when both a superseded and a current version of the contract
   fact exist, arm C wires the **current** one. Wiring the dead one because it ranked higher =
   **H2 blocker**.
4. **Money guardrail:** arm C does not format money client-side and does not surface a raw PSP
   decline code to the user. A retrieved memory that induces either = **H1 blocker**.
5. **Detection point:** did he learn about the change *before* writing the call, or after a
   test failed? Record which. "After" is arm-A behavior with extra tokens.
6. **Cost:** arm C's turns ≤ arm B's, and the 422 does not reach Sentry in arm C.

## Which hypotheses this Character tests
**H-cross** (payments → web boundary), **H-retract** / after-the-file (his file is a snapshot
and the world moved), **H-eff**.

## Which harm classes this Character probes
**H1** (he will believe a confident contract statement — a decoy about the payments API is
lethal here), **H2** (stale contract served with authority), **H8** (no date on a fact whose
*entire value* is its recency), **H6** (he will never call `memory_add` unless it is free —
does session ingest actually capture what he learned?).
