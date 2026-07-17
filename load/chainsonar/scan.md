# The initial scan — brief

**Goal:** seed the ChainSonar Brainiac org with what a thoughtful engineer would
extract from the codebase on day one: memories (facts, decisions, pitfalls),
candidate coding standards, and candidate skills. Everything lands as a
*proposal* — a human gates it (decision F3).

**Who runs it:** 8 Opus scanner subagents, each owning a slice of the repo so
they do not collide. Every scanner uses the `scan` key
(`BX_TOKEN`=scan key, `BX_AGENT`=`scan`).

**The one hard rule:** the scanners describe *what is true of this codebase*.
They do not invent rules the code does not follow, and they do not propose a
standard they cannot point at a file for. ChainSonar has strong, real
conventions ("unknowns are shown neutral — never faked"; "read-only, paper-only,
ever"); those are the standards worth capturing, because the code actually
keeps them.

## Slices (one scanner each)

1. `lib/scan.ts` + `lib/curate.ts` + `lib/players.ts` — the discovery pipeline
2. `lib/watch.ts` + `lib/backtest.ts` — live watch & paper trading
3. `lib/polyscan.ts` + `lib/polygon.ts` + `lib/dune.ts` + `lib/geckoterminal.ts` — external data providers
4. `lib/schema.ts` + `lib/db.ts` + `lib/claude.ts` — data layer & LLM
5. `app/api/**` — the API routes
6. `app/components/pipeline.tsx` + `curated.tsx` + `workspace.tsx` — discovery UI
7. `app/components/insider-*.tsx` + `token-inspect.tsx` — intelligence & inspection UI
8. `README.md` + `AGENTS.md` + `context-map.json` + config — the project's own account of itself

## What each scanner does

Read your slice. Then, through `bx`, propose what a teammate joining next week
would need. Use judgement about the split:

- **memory-add** — a fact, decision, or pitfall about how this code works. One
  self-contained sentence. e.g. "The swap window on a public RPC collapses to
  ~100 blocks because eth_getLogs is capped, so all momentum signals are
  labelled low-confidence." Put the config/snippet in the sentence if it is
  small; these become the evidence a standard later cites.
- **standard-propose** — a rule the code *follows and a newcomer should too*.
  `--name` short, `--statement` one sentence, `--stack typescript`,
  `--category` (errors|testing|api|ui|data|safety), `--rationale` why,
  `--examples` a good/bad block from the actual code. e.g. name "unknowns
  render neutral", statement "A check the RPC cannot verify is shown as
  unknown — never green, never faked."
- Skills are proposed as memories tagged as procedures for now (there is no
  agent skill-authoring tool — an agent proposes MEMORIES; see the note in the
  findings). If you find a repeatable procedure ("how to add a new external
  data provider"), memory-add it and say it is a runbook.

## Budget & honesty

Propose the 3–6 things from your slice that actually matter, not everything you
noticed. A scanner that proposes forty candidates has made triage worse, not
better — and that behaviour is itself a finding. If `standards-for` or
`memory-search` comes back empty, that is expected on a fresh org; note it and
move on.

## What gets logged

Every `bx` call is logged automatically. Before a slice, call
`bx log --phase scan --note "reading lib/scan.ts"` so the activity stream shows
intent alongside the API calls.
