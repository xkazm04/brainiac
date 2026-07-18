---
name: perfect
description: Session-after-session product perfection loop. Opus (the strongest generally-available model) directs — it walks the repo's context map context-by-context, proposes 5 challenged, high-value directions per context (features, design elevations, significant optimizations), gates them with the user until 10 are accepted, then orchestrates builder subagents (model sized to the work — Sonnet for S/M briefs, Opus for L or high-risk ones) per context in isolated worktrees while making every review/merge decision itself. Loop control-state lives in a linked Obsidian vault; the knowledge it produces and consumes lives in Brainiac org memory. Invoke with `/perfect [init|propose|build|status|reflect] [context-name]`.
---

# Perfect — the direction-and-delivery loop (Brainiac)

> One strong model, used as *judgment* — seeing what would make a product excellent, challenging its own ideas, reviewing diffs ruthlessly — is worth more than any number of models used as *execution*. A cheaper strong model is great at execution inside a well-scoped brief. `/perfect` wires the two together in a permanent loop: **Opus directs, Sonnet builds, the vault remembers, Brainiac learns.** Each session moves the product measurably closer to the best UX, architecture, and feature quality it can have; no session ever starts from zero. (Opus is the director because it is the strongest generally-available model — the seat follows capability, not a name; if a stronger tier returns to the plan, it takes the seat.)

**The product**: Brainiac — an org-level memory/knowledge server for coding agents. Rust workspace (`crates/*`: axum REST + MCP stdio server, knowledge pipeline, BYOM model gateway, Postgres/pgvector store via sqlx, Meridian eval harness) plus a Next.js 15 governance console (`console/`: React 19, Tailwind 4, Recharts, openapi-typescript client).

## Roles — Director and Builders

- **Director (the main session — Opus, the strongest generally-available model).** Owns everything that is judgment: opportunity-scoring contexts, drafting directions, adversarially challenging them before the user ever sees them, running the acceptance gate, writing builder briefs, answering builders' product questions mid-flight, reviewing every diff, deciding merge/redo/drop, running the repo gates, committing, and writing the vault. The Director **never delegates a decision** to a builder and never rubber-stamps a builder's diff.
- **Builders (subagents, one per context; model sized to the brief — see the model-selection rule in Phase B).** Sonnet executes S/M-sized briefs; Opus takes L-sized or high-risk ones. Each receives a tight brief (direction specs + acceptance criteria + the context's `filePaths` scope + repo-convention digest) and implements in its **own worktree**. Builders return a structured report; when they hit a genuine product ambiguity they **return the question instead of guessing** — the Director answers via `SendMessage` and the builder continues.
- **Scouts (Explore subagents, cheap).** Produce the per-context current-state brief the Director synthesizes directions from. Never used for judgment.

## The Obsidian vault — durable loop state

Resolve the vault root (first hit wins), then use `$VAULT/Perfect/`:

```bash
for v in "C:/Users/mkdol/Documents/Obsidian/brainiac" "C:/Users/kazda/Documents/Obsidian/brainiac"; do
  [ -d "$v" ] && VAULT="$v" && break
done
# First run: if an Obsidian root exists (C:/Users/*/Documents/Obsidian) but no brainiac vault, CREATE the folder there.
# Portable fallback: no Obsidian root at all → use <repo>/.perfect/ (same schema — still an Obsidian-openable folder).
```

```
Perfect/
  Perfect.md               # HOME / Map-of-Content — always reflects current truth:
                           #   mission, the scored context QUEUE with the CURSOR,
                           #   the ACCEPTED POOL (n/10), shipped ledger headline, link to last session
  config.md                # per-repo overlay: gates to run, worktree recipe, wave size,
                           #   direction sizing rules, cooldown, ## User taste, + ## Skill improvement log
  contexts/<name>.md       # one per context-map context (long-lived, updated in place)
  directions/<slug>.md     # one per direction (long-lived; the atom of the whole loop)
  sessions/<YYYY-MM-DD[-n]>.md  # immutable run records, each ends with a `next:` pointer
```

**Context note** (`contexts/<name>.md`):
```markdown
---
name: <context-map name>        type: perfect/context
group: <group>                  category: ui|api|lib|data|config|test
opportunity: <0-10>             # value reach × headroom × strategic fit (Director's judgment)
last_proposed: <YYYY-MM-DD|never>   cooldown_until: <date|—>
directions: ["[[<slug>]]", …]
---
## Current state   (scout brief digest + file:line evidence — refreshed each proposal pass)
## Direction history   (proposed / accepted / REJECTED-and-why — rejections are memory too)
## Shipped   (direction → commit SHA → observed effect)
```

**Direction note** (`directions/<slug>.md`):
```markdown
---
slug: <kebab, stable>           type: perfect/direction
context: "[[<context-name>]]"   lens: feature|ux|optimization|robustness|wildcard
status: proposed | accepted | building | shipped | failed | dropped | rejected
size: S|M|L                     # must fit ONE builder session (≲15 files, no cross-context schema break)
proposed: <date>  accepted: <date|—>  shipped: <date|—>  commit: <sha|—>
---
## What & why   (the user value, one paragraph, no fluff)
## Evidence   (file:line of the gap/opportunity in today's code)
## Acceptance criteria   (3-6 checkable bullets — the builder's contract AND the review checklist)
## Risks / non-goals
## Build record   (builder report digest, review verdict, gate results — filled during build)
```

**Session note**: phases run, contexts covered, accept/reject tallies, build outcomes with SHAs, deltas, and **`next: <the exact resumption instruction for the following session>`**.

Vault hygiene: slugs are stable; **update notes, never duplicate**. Subagents may fail to write files in some harnesses — after any parallel phase the Director MUST `ls` the target dir and **backfill missing notes from the agents' returned content** before trusting "written".

## Brainiac — the knowledge plane (org memory)

Two memory systems, two layers, no overlap:

- **Obsidian vault = how the loop RUNS** (control plane): the scored queue, the cursor, the accepted pool, per-direction build status, worktree bookkeeping. High-churn orchestration state; never governed, never shared.
- **Brainiac = what the loop KNOWS** (knowledge plane): the org's durable, governed product knowledge — decisions, pitfalls, patterns. A direction that ships is a decision; a builder that hits a landmine found a pitfall. Both outlive the loop and belong to the whole org.

The loop READS Brainiac when proposing and WRITES it when shipping. Credentials come from the repo's `.env` (written by the `brainiac-onboard` skill): `BRAINIAC_API_URL` + `BRAINIAC_API_TOKEN`. Read the token WITHOUT printing it (`$(grep '^BRAINIAC_API_TOKEN=' .env | tail -1 | cut -d= -f2- | tr -d '\r')`); if `.env` has no Brainiac key, this whole plane is a no-op — the loop still runs on the vault alone, so a 000/connection failure is skipped, never fatal.

- **Report usage — once per round, at Phase 0.** The loop IS a use of this skill:
  ```bash
  curl -fsS -X POST "$API/v1/library/usage" -H "authorization: Bearer $TOK" -H 'content-type: application/json' \
    -d '{"artifact_kind":"skill","artifact_slug":"perfect","event":"apply"}' || true
  ```
  Best-effort telemetry — needs a `lib:read`-scoped key; a plain onboarding key (`read,write`) gets 403, which is fine to swallow. Never let a usage 403 stop the round.
- **Read at Propose (Phase P, before drafting):** search org memory for the cursor context's prior art, so the loop never re-proposes a shipped decision or a rejected idea, and builds on known pitfalls. `POST $API/v1/memories/search {query: "<context concern>", k: 5}` (bearer token). Fold hits into the challenge checks alongside the vault's own history.
- **Write at Ship (Phase B merge / Phase W):** each shipped direction becomes a decision memory; each pitfall a builder reported becomes a pitfall memory. `POST $API/v1/memories {content: "<one self-contained statement>", kind: "decision"|"pitfall"}`. One statement per fact, not a transcript — it enters the governed review pipeline. Stamp is automatic (the key is project-scoped).

## The loop — a vault-driven state machine

Every invocation starts the same way; the vault decides which phase runs.

### Phase 0 — Recall & register
1. Read `Perfect.md` (+ last session's `next:` pointer). If missing → run **init** (below).
2. Read `context-map.json`; diff against `contexts/*` — new contexts get notes + a queue slot, removed ones get archived (`status: retired` in frontmatter).
3. Scan the memory directory (MEMORY.md) for signals that veto or steer directions (e.g. "feature X was cut — don't re-suggest"; the disputes consolidation commit is an example of deliberate cuts).
4. **Report this run** to Brainiac as a skill-usage signal (best-effort — see the knowledge-plane section). Then announce the resumption point in one sentence and go where the state machine points: pool < 10 → **Propose**; pool ≥ 10 (or user said `build`) → **Build**.

### Init (first run only)
1. Scaffold the vault tree + `config.md` (record the gates:
   - **Rust**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`. Pg-backed integration tests (`*_pg.rs`) **silently skip without `DATABASE_URL`** — a green run without it proves nothing about them. Full gate: `docker compose up -d postgres` then run tests with `DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac`.
   - **Console** (when `console/` touched): `npm run typecheck` + `npm test` in `console/`.
   - **API contract** (when handlers/routes touched): regenerate `openapi.json` per the repo's OpenAPI derivation, then `npm run gen:api` in `console/` and commit both artifacts.
   - Clippy calibration: gate on *no NEW warnings in files this diff touched*, compared against master for the same files.
   Also record: wave size = 3; cooldown = 2 rounds.)
2. Score every context 0-10 for **opportunity** = user-facing reach × headroom (distance from "perfect", judged from context-map metadata, `docs/ARCHITECTURE.md`, `docs/PLAN.md`, `docs/EVAL.md`, and memory) × strategic fit (active arcs in memory). Write the ranked **queue** into `Perfect.md` with the cursor at the top. Don't deep-read code yet — scoring is refined per-context at proposal time.
3. Write session note; proceed straight into Propose.

### Phase P — Propose (context by context, until the pool holds 10)
Loop while `pool < 10` and the user hasn't said stop:

1. **Cursor** = highest-opportunity context not on cooldown. **Prefetch**: before presenting context *k*, launch the scout for context *k+1* in the background.
2. **Scout** (Explore, "very thorough", read-only): given the context's `filePaths`, `apiRoutes`, `db_tables` → return a current-state brief: what exists, what's rough, dead ends, UX seams, perf smells, with `file:line` evidence.
3. **Draft 5 directions** — one per lens by default: **feature** (new user value), **ux** (design/flow elevation), **optimization** (perf/cost/significant simplification), **robustness** (failure modes, observability, architecture), **wildcard** (the non-obvious idea a great PM would pitch). Each sized to ONE builder session; a bigger vision ships as its phase-1 slice.
   **Weight the slate by `config.md → ## User taste`** — the lens spread is a starting point, not a quota. Default depth is the *engine*, not the chrome: for any context with backend/algorithmic substance (pipeline, retrieval, extraction, gateway, store), most directions should be architecture-level (data model, retrieval quality, ranking, extraction accuracy, RLS/permission correctness, cost structure, eval coverage); UI surfacing appears at most once-twice unless the user steers otherwise. Scout prompts must match this depth (trace the full pipeline, not just the components).
4. **Challenge before presenting** (the Director argues against itself; a direction that fails any check is replaced, not presented):
   - Does it already exist in code? (scout evidence, not assumption)
   - Was it already proposed/rejected/shipped? (check `contexts/<name>.md` history + `.claude` memory + **Brainiac org memory** — `search {query: "<the context's concern>", k: 5}`; a shipped decision or a known pitfall there vetoes the direction or reshapes it)
   - Does it conflict with an active arc or a "removed, don't re-suggest" memory?
   - Is the value claim concrete — can I name the user moment it improves?
   - Can one builder session genuinely ship it behind the acceptance criteria?
5. **Present** the 5 in chat — numbered, each: title · lens · size · one-paragraph why · evidence · acceptance criteria. Then gate with **AskUserQuestion (multiSelect)** — the tool caps options at 4 per question, so use TWO questions in one call: Q1 = directions 1–3, Q2 = directions 4–5 (labels = `N · short title`, description = one-line value claim + size). The user can annotate via "Other" (e.g. `edit 2: …`, `stop`); selecting nothing in both = none accepted.
6. Record outcomes in the vault (rejected ones too, with the user's implied reason — rejections steer future proposals). Accepted → `directions/<slug>.md` with `status: accepted`, pool counter++, context gets `cooldown_until`. Update `Perfect.md` after every context, not at session end — a killed session must lose nothing.
7. **A `none` gate that carries a steer** (the user says what they wanted instead) is a re-scout order, not a rejection of the context: promote the steer to `config.md → ## User taste` if it generalizes, re-scout at the steered depth/angle, and re-propose the SAME context once before advancing the cursor. Never re-present any rejected direction.

### Phase B — Build (one builder per context, the Director decides everything)
1. **Wave plan**: group the pool's accepted directions by context → one builder per context, ≤ `config.wave_size` (default 3) concurrent, and **≤ 3 directions per builder brief** (a 4-direction brief exceeds one agent-session budget — split a bigger context into two sequential builders). Present the wave plan in one screen; on user go (or when invoked as `/perfect build`), execute.
2. **Worktree per builder** — prepared by the Director, NOT via Agent-tool isolation (those worktrees lack `node_modules`):
   ```bash
   git worktree add .claude/worktrees/perfect-<ctx> -b worktree-perfect-<ctx>
   # Console builders need node_modules — junction, NOT copy:
   cmd //c mklink //J ".claude\\worktrees\\perfect-<ctx>\\console\\node_modules" "..\\..\\..\\..\\console\\node_modules"
   # Rust builders share the main target dir to avoid a cold full rebuild per worktree:
   #   run cargo with CARGO_TARGET_DIR=<main repo>/target
   # Pg tests: docker compose (main repo) provides postgres on :5433 — builders reuse it via DATABASE_URL.
   ```
3. **Model selection (per brief, Director's judgment):** default from the SIZE of the largest direction in the brief — all S/M → `model: "sonnet"`; any L → `model: "opus"`. Escalate an S/M brief to Opus when the Director judges the risk profile warrants it: RLS/permission-touching query paths, concurrency/locking semantics, schema migrations with subtle invariants, or work that must integrate against signatures that moved on master mid-wave. Never de-escalate an L brief. Record the chosen model in the direction notes' build record; if a Sonnet builder's diff fails review on capability grounds (not spec ambiguity), redo that direction with Opus and log it in the skill-improvement log — repeated capability failures recalibrate the default.
4. **Brief** each builder (see template below); launch with the selected model, `subagent_type: "general-purpose"`, all briefs in one message so they run concurrently.
5. **Mid-flight decisions**: a builder returning `DECISION NEEDED: …` gets an answer from the Director via `SendMessage` — product calls, trade-offs, and scope cuts are the Director's alone. A builder that stops without its final report gets one `SendMessage` nudge.
   **Builder-death recovery (session limits WILL kill builders):** the instant a builder dies, `git add -A && git commit --no-verify` a `wip(…)` snapshot **inside its worktree** (isolated tree — add-all is safe there; never-lose-work beats commit hygiene). Then the Director either finishes the work inline (review the WIP diff, complete gaps, split into per-direction commits along file boundaries — same-file hunks may share a commit if the message says so) or re-briefs a fresh builder after the limit resets with "continue from the WIP commit".
6. **Review — the Director earns its title here.** Per builder branch: `git diff master...worktree-perfect-<ctx>` and review against each direction's acceptance criteria, repo conventions (workspace crate boundaries, sqlx query style, error handling, console component patterns, OpenAPI/types sync), and taste. Verdict per direction: **merge** / **redo with notes** (SendMessage, builder fixes in place) / **drop** (`status: failed`, reason recorded). Never merge on "tests pass" alone — read the diff.
   **Docs-vs-code check:** when a diff documents a behavior (contract text, formula, doc comment, OpenAPI description), grep for the code that implements it before merging — a contract describing behavior the code doesn't have is worse than nothing.
   **Silent-skip check (Brainiac-specific):** pg-backed tests skip without `DATABASE_URL` — before trusting a builder's "tests pass", confirm the pg tests actually RAN (look for the test names in output, not just exit 0).
7. **Merge serially**: per direction, `git merge --squash` (or cherry-pick) → ONE atomic commit on master, message `feat(<context>): <direction title>` + `Co-Authored-By` footer. Stage per-file, verify `git diff --cached --stat` matches intent (foreign pre-staged files → `git restore --staged` them). Run the config gates on master after each merge; a red gate is fixed inline before the next merge. Run `cargo fmt --all` before committing Rust changes (repo history shows fmt-sweep commits — don't create the need for another).
8. **Contract-sync in the same turn**: changes to handlers/routes regenerate `openapi.json` + `console/src/lib/api-schema.d.ts` (`npm run gen:api`); changes to file ownership update `context-map.json`; architecture-level changes update `docs/ARCHITECTURE.md`.
9. **Cleanup**: per worktree — `cmd //c rmdir` the node_modules **junction FIRST** (if created), then `git worktree remove`, then delete the branch once its commits are on master.

### Phase W — Wrap (every session, even interrupted ones)
1. Update every touched vault note; write the session note with the **`next:` pointer** (e.g. `next: propose — cursor at memory-store-hybrid-retrieval, pool 7/10` or `next: build wave 2 — pipeline-worker + console-shell remain`).
2. `Perfect.md` headline refreshed: pool count, queue cursor, shipped-total, last-session link.
3. **Reflect on the skill itself**: 2-4 bullets in `config.md → ## Skill improvement log` — what dragged, what the user overrode, what the next round should change. This log is the input for the between-rounds skill revision.
4. **Write shipped knowledge to Brainiac** (knowledge plane): for each direction that reached `shipped` this session, `memory_add` a one-line decision statement (kind `decision`); for each landmine a builder surfaced, a `pitfall`. One self-contained fact each, not a transcript — it enters the governed review pipeline and the next round's Propose phase will read it back. Best-effort; a connection failure is skipped, the session still wraps.

## Direction quality bar (what earns a slot in the 5)

- **Value-first**: names the user moment it improves; "nice refactor" is not a direction unless it unlocks something.
- **Evidence-backed**: cites today's code (`file:line`), not vibes.
- **One-session-shippable**: ≲15 files, no cross-context schema breaks; else slice it.
- **Novel to the vault**: not shipped, not pending, not previously rejected (unless the world changed — say so).
- **Lens-diverse**: default one per lens; substituting a second entry in one lens requires the Director to say why.

## Builder brief template

```
You are a builder for the `<context>` context of Brainiac — an org-level memory/knowledge
server for coding agents. Rust workspace (axum REST :8600 + MCP stdio, knowledge pipeline,
BYOM gateway, Postgres/pgvector via sqlx) + Next.js 15 governance console (React 19, Tailwind 4,
Recharts) in console/.
Work ONLY in this worktree: <abs path>. Your scope is this context's files:
<filePaths from context-map.json>. Touching other contexts requires DECISION NEEDED.

Implement these accepted directions, one atomic commit each, message `feat(<context>): <title>`:
<per direction: What & why · Acceptance criteria · Evidence file:line · Risks/non-goals>

COMMIT EACH DIRECTION THE MOMENT IT IS DONE AND VERIFIED — never batch commits
for the end of the session. An interrupted session must lose at most the
direction in progress, not everything.

Repo law (non-negotiable):
- Rust: cargo fmt --all before every commit; no new clippy warnings in files you touch
  (run with CARGO_TARGET_DIR=<main repo>/target to reuse the build cache).
- Schema changes go through migrations/ (sqlx migrate) — never mutate schema inline.
- Pg-backed integration tests (*_pg.rs) need DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac
  (docker compose postgres in the main repo). They SILENTLY SKIP without it — a green run
  without DATABASE_URL does not count as verification. Say explicitly whether they ran.
- API changes: keep openapi.json in sync (it is derived from the handlers) and regenerate
  console types with `npm run gen:api` in console/; commit both artifacts.
- Console: typecheck with `npm run typecheck`, test with `npm test` (vitest) in console/;
  follow existing component patterns in console/src; Tailwind 4 utility style, Recharts for charts.
- RLS/permissions are a core product invariant — any query path that touches org data must
  respect the principal's token scope; when in doubt, DECISION NEEDED.
- If you change which files a context owns, update context-map.json to match.
- Verify before claiming done: the gates above plus driving the actual flow when the stack is
  up (docker compose + cargo run + console dev server); report what you COULD NOT verify honestly.

If a product decision is ambiguous, STOP that direction and return `DECISION NEEDED: <question>`
with your recommendation — never guess. Final report format:
per direction → status (done|blocked|decision-needed), commits, files, verification evidence, open risks.
```

## Modes

- **`/perfect`** — resume the loop wherever the vault says it stopped (the default; covers init on first run).
- **`/perfect propose [context]`** — force a proposal pass (optionally jump the cursor to a named context).
- **`/perfect build`** — build now with the current pool even if < 10.
- **`/perfect status`** — read-only: queue, cursor, pool, in-flight builds, shipped ledger, last session. No agents.
- **`/perfect reflect`** — read `config.md → Skill improvement log` + last sessions and propose concrete edits to THIS skill file.

## Guardrails

- **Never stash, never `git add -A` on master** — per-file staging, staged-count check before every commit; other sessions' work is sacred (worktree WIP snapshots are the one exception, isolated trees only).
- **Cost discipline**: scouts are Explore-tier; builder models are sized to the brief (Sonnet for S/M, Opus only for L or Director-judged high-risk work — see Phase B step 3); the Director never re-runs a scout whose brief is < 1 round old (it's in the context note).
- **Honest ledger**: a direction only reaches `shipped` with gates green AND the Director having read the diff; anything else is `failed` with a reason. No silent drops — every accepted direction's fate is recorded.
- **Interruptibility is a feature**: write the vault incrementally (after every context in P, after every merge in B) so a killed session resumes losslessly.
- **The user is the product owner**: the gate is theirs; the Director challenges but never overrides a rejection, and repeated rejections of a lens/context recalibrate the queue scores.
