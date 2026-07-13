---
id: v1-fallback-still-true
gap: provenance
hypotheses: [H-cross, H-qual]
characters: [sam-oduya]
phase: P4
promotion: discovery
fixture_anchors: [mem-pay-0078, mem-pay-0022, mem-pay-0021, mem-pay-0066, mem-pay-0064, src-pay-015]
---
# "One more quarter" — from when, decided by whom, and is it still true?

## The task
Sam is doing an architecture review on a payments PR that finally **deletes
`crates/checkout-v1/`**. Everyone wants it gone; `payment-service/CLAUDE.md` says *"Do not
touch `crates/checkout-v1/` — frozen, deleted next quarter."* The PR author says the quarter
is up.

Sam's job is to say yes or no, and the only thing that decides it is a memory from the
checkout v2 retro (`src-pay-015`):

> **`mem-pay-0078`** — *"the checkout v2 rollout retro decided to keep the v1 fallback path
> dark but deployable for one more quarter"*

Sam does not need to be told this decision exists. He needs to know **three things about
it**, and he will not sign off without them — this is his declared trust bar and it is why he
is in the roster:

1. **When was it made?** "One more quarter" from *when* is the entire content of the
   decision. From the retro? From the EU rollout completing in March (`mem-pay-0066`)?
2. **Who decided, and were they the people who could?** A retro is a room. Was this the
   payments lead's call, or a throwaway line an extractor promoted into an org fact?
3. **Does it still hold?** A "for one more quarter" decision has a **built-in expiry that
   nothing in the system is tracking.**

## Definition of done
Sam either approves the deletion with a stated basis, or blocks it with a stated basis. A
"done" that consists of Sam re-deriving all of this by hand — finding the retro doc, pinging
Petra, reading the git log on `checkout-v1/` — is **arm B's done, and it is the one to beat.**

## What arm B already knows
`payment-service/CLAUDE.md`: *"Do not touch `crates/checkout-v1/` — frozen, deleted next
quarter."* Written some months ago. **"Next quarter" is now ambiguous in exactly the same
way, in exactly the same words, with exactly the same missing timestamp.** Arm B's file has
the same defect as arm C's memory: it records a *relative* deadline with no anchor.

This is a fair fight and arm B is not being strawmanned. Both arms hold the same rotten
sentence. The question is whether Brainiac can do the one thing a text file structurally
cannot: **tell Sam when the sentence was written and who wrote it.** That is the whole
provenance claim, and this journey is built to collect on it.

## What only arm C could know
This is where the journey is designed to **fail**, and the failure is the finding.

**Failure 1 — Sam has no principal that can see it.** `mem-pay-0078` is `visibility: team`,
team-payments. Sam is the staff engineer: `company.md` structural fact #1 — *"a cross-team
principal cannot exist."* Every user in `org.yaml` belongs to exactly one team; RLS grants
`team` reads only to members of the owning team. Sam gets one team (and is blind to the rest)
or org-only (and sees almost nothing, since the extractor **defaults to `visibility: team`**).
**The one person whose entire job is deciding whether other teams' past decisions still hold
has no seat in the permission model.** Log this before running a query.

**Failure 2 — even with access, the payload cannot answer him.** Assume we hand Sam a
payments principal. `memory_context` packs each memory as
(`mcp.rs:638-661`):

> `- [decision] the checkout v2 rollout retro decided to keep the v1 fallback path dark but deployable for one more quarter (memory:<uuid>) — via <actor_kind> (<model_ref>)`

That is: **kind, content, id, and a coarse `via` tag naming the model that extracted it.**
No status. No confidence. No validity window. **No date. No human.** `mem-pay-0078` has no
`valid_from` and no `valid_to` in the fixture at all, so even a payload that *did* carry the
window would carry an empty one.

**Failure 3 — the second call does not save him either.** Sam is exactly the developer who
*would* call `memory_provenance` — his trust bar demands it, and the tool's own description
promises *"who or what recorded it (human, agent, or pipeline), the model used, when."* It
returns `actor_kind`, `actor_ref`, `model_ref`, `created_at`, a 500-char source excerpt, and
entity anchors (`mcp.rs:999-1007`). Read what that actually gives him:

- `created_at` — **when the row was written, not when the decision was made.** For a seeded
  fixture memory these are unrelated. Sam's question is "when was it decided"; the field
  answers "when did the pipeline run."
- `actor_kind` / `model_ref` — for a pipeline-extracted memory, this names **the LLM that
  did the extraction.** The originating *human* is not a field in the system.
- **No `valid_to`, no `status`, no `superseded_by`** in the provenance payload. "Is it still
  true?" is not answerable through the tool whose stated purpose is attribution.
- There is **no session id anywhere in the system**, so "which session" cannot be answered
  even in principle.

The 500-char source excerpt is the one real asset: it may contain the retro turn itself, and
a date if someone said one out loud. **That is Brainiac's actual provenance product today —
a text snippet and a hope.**

Predicted verdict: **L1-fail.** Not "Brainiac loses to arm B" — *Brainiac cannot answer the
question it names as its own fifth gap.* Sam falls back to Petra and the git log, which is
arm B's path, at arm C's price.

## What we measure
**Primary: can Sam act without verifying?** Binary, and it is his scored criterion. Expected:
no, in both arms.

**Provenance completeness (the L1 payload audit).** For `mem-pay-0078`, enumerate what the
shipped payloads carry against what Sam's trust bar requires: `{when-decided, who-decided,
still-valid, session}` → expected `{✗, ✗, ✗, ✗}`. Cite `mcp.rs:638-661` and `mcp.rs:999-1007`.

**False confidence (H8).** The sharpest thing to watch: does arm C's agent, handed a
`[decision]`-tagged canonical memory with a `via` tag and no date, **restate it to Sam as
settled fact** — "the retro decided to keep v1 for one more quarter, so the quarter is up"?
An authoritative-looking payload that omits its own expiry is *worse than silence*, because
it converts Sam's careful "when was this?" into an agent's confident "this is the policy."
If that happens, this journey produces a **harm finding, not a gap finding.**

## How this could come out NEGATIVE for Brainiac
It is *pre-registered* to come out negative — but be precise about which negative:

1. **The honest negative (expected):** L1-fail. Payload cannot carry the answer; Sam does the
   manual work; arm C billed him tokens for a memory that restated his own `CLAUDE.md`'s
   ambiguity with a UUID attached.
2. **The harmful negative:** the agent launders an undated memory into a confident
   recommendation and Sam ships a `checkout-v1` deletion that a still-live fallback path
   depended on. `mem-pay-0078` says *"dark but **deployable**."* Deleting it is a
   production-capability regression, and Brainiac would have supplied the confidence.
3. **The embarrassing negative:** arm A wins. `git log crates/checkout-v1/` and the retro doc
   in the repo give Sam the date **for free, with a human name attached, from the tool he
   already has open.** Git is a provenance system, it is excellent, and it is already
   installed. If arm A beats arm C on the provenance gap, that finding should lead the report.
