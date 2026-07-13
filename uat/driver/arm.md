# The arm contract — what each arm is equipped with, exactly

> Fidelity lives here. If the arms differ in anything except the thing under test, the delta is
> an artifact of the harness, not a fact about the product.

**Invariants across all three arms.** Same task statement, verbatim. Same repo, same starting
commit, in **its own checkout** — arms must never see each other's diffs. Same model, same
effort setting. Independent subagents; **no arm may know the others exist.** Everything is
logged: turns, tokens, wall-clock, and every file the agent opened before it knew what to do
(*exploration reads* — the metric where the literature says the effect actually lives).

---

## Arm A — Cold

The repo. Nothing else.

Explicitly: **no `CLAUDE.md`** (delete it from the checkout — do not merely refrain from citing
it, the agent reads it automatically), no `.claude/rules/`, no auto-memory, no MCP server.

Arm A is not a serious competitor and is not meant to be. It exists for one purpose: to catch
the embarrassing case where **memory makes things worse than nothing at all** (`C < A`). If that
ever fires, it is the headline of the run.

## Arm B — Claude's native memory (the real baseline)

The repo **plus the full free stack**, exactly as specified in `../baseline.md`:

- the repo's committed `CLAUDE.md`,
- `.claude/rules/*.md` with `paths:` frontmatter (path-scoped, just-in-time — **do not omit
  these; they are arm B's retrieval and leaving them out is how you fake a win**),
- subdirectory `CLAUDE.md` files,
- auto-memory **on**, with whatever it has legitimately learned in prior phases *for that
  Character on that repo* (machine-local, per-repo, never shared — that limit is the
  hypothesis, not a handicap we imposed),
- the symlinked `~/meridian-standards/backend.md` → `.claude/rules/org.md`,
- hooks.

**No MCP server. No Brainiac. No network access to :8600.**

Between sprint phases arm B's owner **may** update these files (the maintenance budget). Record
what they actually did. "Nobody thought to update it" is a measured result, not an assumption.

## Arm C — Brainiac

The repo **plus** `CLAUDE.md` **plus** Brainiac.

Note what that sentence means and do not get it wrong: **arm C keeps arm B's files.** Nobody
deletes their `CLAUDE.md` to install a memory service. Arm C is arm B *plus* the MCP surface —
so the delta measures **what Brainiac adds on top of best practice**, which is the only delta a
buyer cares about. (An arm C *without* `CLAUDE.md` would measure "does Brainiac replace a
context file," a question nobody is asking and one that would flatter us.)

The agent gets the real tool surface via `mcp_call.sh`:

- `memory_context` at session start — the bundle. **6000-char budget, canonical-only, packing
  stops at the first line that would overflow** (`mcp.rs:30`, `:656-658`).
- `memory_search` mid-task — **note: no status floor. This serves `raw`, unreviewed extractions
  alongside canonical ones.** It is also the tool the within-session-decay hypothesis (`H-decay`)
  rests on, so watch whether the agent reaches for it in the middle of a long task or only at the
  start.
- `entity_lookup`, `memory_provenance`, `memory_feedback` as the agent sees fit.
- `memory_add` / `knowledge_propose` on the way out — **this is the flywheel's write side.** If
  the agent doesn't write, the next phase inherits nothing, and the relay is dead. Log whether it
  wrote **without being told to.** (If it must be told, that is H6 — capture friction — and it is
  the failure mode that kills every knowledge system in the literature.)

**Log every call, every payload, verbatim.** The payload is the evidence. A finding about arm C
without the payload that produced it is not a finding.

---

## What gets measured, mechanically (not by a model)

| metric | how |
|---|---|
| turns | driver counts agent turns |
| tokens | input + output, per session |
| wall-clock | seconds to done |
| **exploration reads** | files opened *before* the agent had a plan — the "how long was I lost" metric |
| tool calls (arm C) | which, when, with what args |
| **utilization (arm C)** | of the memories injected, how many did the agent cite or act on? *Low utilization at high token cost is a cost, not a neutral.* |
| **write-back (arm C)** | did the session produce a memory? unprompted? |

Quality is judged separately, **blind**, per `../rubric.md`. The judge never sees these numbers
and never learns which arm is which.
