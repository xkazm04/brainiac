# uat/ — the Brainiac acceptance trial

This is not a test suite. It is a **controlled trial of whether an engineering company is
better off with Brainiac than without it.**

The Meridian eval harness (`cargo run -p brainiac-server -- eval`) already answers *does
retrieval work* — NDCG@10, contradiction precision, entity-merge F1, RLS-leak-zero. This
answers the question that harness structurally cannot:

> **NDCG@10 of 0.876 does not mean a single pull request got better.**
> Does the org do better work with Brainiac than with a `CLAUDE.md` that costs nothing —
> net of the harm a shared, governed, agent-facing store can uniquely cause?

The engine is the `/uat` skill (`.claude/skills/uat/skill.md`). This directory is the overlay:
who the developers are, what they're doing, and what we're comparing against.

## The design in one paragraph

Every journey runs in **three arms** — **A** a cold agent, **B** Claude's native memory stack
built the way a competent senior would build it (the free baseline), **C** Brainiac on top of B.
**The verdict is `C − B`, and it is allowed to come out negative.** The primary endpoint is
**efficiency** (turns, tokens, wall-clock, exploration reads), because that is where the only
controlled evidence says the effect lives; **quality is a guardrail** ("no worse"). Alongside
findings, every run ships a **Harm Ledger** — the debit column — because a store that can be
poisoned, that can leak, and that costs a maintainer their afternoon has costs a text file does
not have. Net value = decision-delta − harm − governance tax.

## The files, and which ones are load-bearing

| file | what it is |
|---|---|
| **`baseline.md`** | **Arm B, written first and written generously. The evidence that the trial was fair.** Written before any journey, so it cannot be tuned to lose. If this is a strawman, every number here is worthless. |
| **`decoys.md`** | **The planted poison.** The evidence that we tried to break Brainiac rather than flatter it. |
| `company.md` | Meridian: teams, stacks, repos, the 12 Characters, the sprint calendar — and the two structural facts the permission model forces on us. |
| `characters/` | The developers. Each carries their **own scored criteria**, applied identically every run, so deltas are comparable across runs. |
| `journeys/` | Goals, not scripts. Each declares which of the **five gaps** it tests — or `gap: none`, an honest control where Brainiac should lose. |
| `relays/` | Multi-developer, time-ordered chains. **The only shape a shared store can win that a local file structurally cannot.** |
| `rubric.md` | The eight dimensions, the severity ladder, and the **blind-judge protocol**. |
| `env.md` | How to reach a known start state. Read the four open questions before the first live run. |
| `driver/arm.md` | **The fidelity contract.** What each arm is equipped with, exactly. |
| `accepted-gaps.md` | Known and accepted — won't re-surface. |

## The five gaps — where Brainiac is allowed to win

A `CLAUDE.md` is free, ships in git, and needs no reviewer. Brainiac earns its existence only
where that structurally cannot go:

1. **cross-team** — a file in the payments repo cannot help the data team.
2. **after-the-file** — the knowledge arrived and nobody remembered to write it down.
3. **retraction** — a decision was reversed; a stale line just sits there being confidently wrong.
4. **permission** — the contractor must not see it.
5. **provenance** — *who said this, when, and is it still true?*

**Journeys outside those five are journeys Brainiac will lose, and the run must say so.** Two of
the eight are deliberate `gap: none` controls where we predict **no delta and higher cost**. If
those come out positive, suspect the harness before celebrating.

## Running it

```
/uat run --l1        # cheap, mass-parallel, no live server. Expect a pile of L1-redundant.
/uat run             # full L1 → L2: live server + workers + real agent sessions, blind-judged
/uat run --relay std-retry-reversal
/uat promote <journey>   # freeze a real, multi-sampled positive delta into a gate
```

L2 needs `docker compose up -d`, `serve`, **and `worker`** — see `env.md`. The worker is the
pipeline; without it the store never grows and the run measures an empty corpus while reporting
green.

## What we already know before running (from reading the code)

Recorded here because it shapes what the run should look for, and because a trial that
"discovers" these live has wasted its live time:

- **`memory_search` has no governance floor** — it serves `raw`, unreviewed, machine-extracted
  memories alongside canonical ones. The review queue does not stand between a hallucinated
  extraction and an agent.
- **The `memory_context` payload carries no "when" and no originating human.** There is no
  session id anywhere in the system. *Provenance* is one of the five gaps Brainiac claims to own.
- **There is no redaction anywhere**, and `memory_provenance` returns a 500-char verbatim excerpt
  of the raw transcript to any principal RLS admits.
- **A cross-team principal cannot exist** in the current model — the two people whose job is to
  see across teams have no seat.
- **The fixture corpus contains zero programming-language content**, so cross-stack harm (H3) is
  `not probed`, never `clean`.

None of these are fatal. All of them are the kind of thing a demo hides and a trial finds.
