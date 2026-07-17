# load/ — the ChainSonar field test

**The question this answers:** the eval harness proves retrieval works. `uat/`
asks whether a company is better off with Brainiac than without. Neither one
puts *all three modules under simultaneous, realistic developer traffic on a
codebase nobody wrote for the test.*

> Three developers, three different jobs, one real repository, one org.
> Memory, Knowledge Base, and Library all live at once.
> **What breaks, what is missing, and how much do they actually use it?**

This is not a benchmark and there is no score. The output is a findings report:
design gaps, feature opportunities, bugs — evidence-backed, each one traceable
to a logged call or a missing one.

## Why a real codebase

`fixtures/v1` is the Meridian corpus: synthetic, curated, and *shaped to make
the pipeline look good* — every gold memory is a clean one-sentence claim,
because we wrote them that way. ChainSonar
(`C:/Users/mkdol/.personas/projects/chainsonar`) is the opposite and that is
the point:

- ~5,100 LOC of Next.js/TypeScript nobody wrote with Brainiac in mind
- a real git history with real reversals in it
- 12 files over 200 LOC — genuine refactor surface
- a real architectural tension worth researching (third-party RPC vs. direct
  chain listening), so the "new development" agent has something to actually
  think about rather than a scripted errand
- its own strong conventions ("unknowns are shown neutral — never faked"), so
  the Library has real judgment to capture rather than invented rules

## The shape

```
  chainsonar repo ──scan──▶ [8 Opus scanners] ──propose──▶ Brainiac intake
                                                                 │
                                                      the gate (a human: me)
                                                                 │
                                              memories · standards · skills · pages
                                                                 │
              ┌──────────────────────────────────────────────────┤
              ▼                        ▼                         ▼
        dev-a (Opus)            dev-b (Sonnet)            dev-c (Opus)
     direct chain listening   refactor 200+ LOC files    UI → hundreds of items
      research·design·build      modularize, quality       virtualize, paginate
              │                        │                         │
              └──────────── every call through `bx` ─────────────┘
                                       │
                          logs/brainiac-calls.jsonl (telemetry)
                          logs/activity.jsonl       (what they think they're doing)
                                       │
                                  runs/<date>/report.md
```

Each developer works in **its own git worktree** of ChainSonar, on its own
branch, with **its own scoped key**. They cannot see each other's work — which
is the whole point: Brainiac is the only channel through which one developer's
learning can reach another. If it does not travel, that is a finding.

## Design decisions

| # | Decision | Why |
|---|---|---|
| F1 | **`bx`, a CLI over REST — not MCP.** Every agent reaches Brainiac through `load/chainsonar/bx.mjs`, which mirrors the MCP tool vocabulary 1:1 and logs every call. | Two reasons, one practical and one damning. Practical: subagents share the session's MCP config, so per-agent tokens and per-agent telemetry are impossible through it. Damning: **the MCP surface only accepts env-declared tokens** (finding F-2), so a per-developer managed key cannot use it at all. `bx` gives real scoped tokens, real RLS, and complete call telemetry. The interaction *shape* is what matters, and it is identical. |
| F2 | **Telemetry is the harness's job, not the product's.** `bx` writes `{ts, agent, cmd, args, status, latency_ms, bytes, outcome}` per call. | We are measuring the product; the product measuring itself would be the thing under test grading its own homework. It also means zero instrumentation code lands in the server for a test. |
| F3 | **The scan proposes; a human gates.** Scanners get `write` + `lib:propose`, never `lib:publish`. Everything they produce lands in the review queue and the standards gate, and I triage it as maintainer. | This is the product's central claim. A harness that seeded the corpus with direct inserts would skip the exact machinery it exists to exercise — and the triage volume is itself a finding (how long does 8 agents' output take a human to clear?). |
| F4 | **Developers get `read` + `write` + `lib:read` + `lib:propose` + `kb:read`. No publish, no admin.** | Exactly what a real coding agent should hold. If a developer needs a scope it does not have to do honest work, that is a finding about the scope design, not a reason to widen the key. |
| F5 | **Isolation by worktree, not by trust.** Each developer gets `git worktree add` on its own branch. | Three agents editing one tree would produce merge noise that drowns the signal. Separate trees also make "did knowledge travel between them?" a real question with a real answer, instead of "they read each other's files". |
| F6 | **The agents are told what to do, never what Brainiac will say.** Briefs describe the job and the `bx` vocabulary; they never name a memory, a rule, or a skill to look for. | A scripted lookup proves nothing. If an agent does not reach for the org's knowledge unprompted, the product has an adoption problem, and that is worth knowing. |
| F7 | **Two blockers fixed before the run, recorded as findings.** F-1 (scopes unmintable) and F-2 (MCP rejects managed keys) both make the harness impossible. | They are findings *about the product*, discovered by trying to use it as a customer would. Fixing them is not cheating; hiding that they were there would be. |

## What we measure

Not a score. Four questions, each answered from the logs plus Brainiac's own state:

1. **Reach** — did every module get used, by whom, how often, for what? (`brainiac-calls.jsonl` grouped by agent × command)
2. **Unprompted use** — did agents call Brainiac at moments the brief did not
   name? The brief says "search before a non-trivial decision"; it never says
   *which* decision.
3. **Travel** — did anything one developer learned reach another? The only path
   is through the store, and the worktrees make that testable.
4. **Friction** — every 4xx, every empty result, every call an agent had to make
   twice. An empty search is not a failure of the agent.

## Running it

```bash
# 0. prerequisites: postgres up, `brainiac serve` + worker running
docker compose up -d
DATABASE_URL=... cargo run -p brainiac-server --bin brainiac -- serve --with-worker --mock

# 1. provision the org and mint the five keys → load/chainsonar/.keys.json (gitignored)
node load/chainsonar/setup.mjs

# 2. the scan (8 Opus subagents) — see scan.md for the brief
# 3. triage as maintainer (console at /console?m=reviews and ?m=standards)
# 4. the three developers — see agents/*.md
# 5. the report → runs/<date>/report.md
```

## Findings log

Every finding lands in `runs/<date>/report.md` under one of three headings,
and each one carries its evidence:

- **bug** — it does not do what it says. Evidence: the call and the response.
- **design gap** — it does what it says, and what it says is not enough.
  Evidence: the thing an agent could not do.
- **feature opportunity** — nobody asked for it; the run made it obvious.
  Evidence: the pattern in the logs.
