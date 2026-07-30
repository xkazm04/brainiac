---
name: architect
description: Heavy structural scan of the Brainiac workspace — surfaces weak patterns to upgrade and strong patterns to codify, producing ADR-style decisions with a durable cross-session backlog. The highest-risk / highest-payoff scan in the suite; walks context-map.json context by context.
category: Development
memory: vault
contexts: tracked
---
# Architect (Brainiac)

Heavy-hitter scan for **structural patterns** — both weak ones to upgrade and
strong ones to codify. For rare, deliberate, high-effort sessions where the payoff
is a class of bugs eliminated, a crate boundary restored, or a convention promoted
from "tribal knowledge" to "clippy/CI-enforced rule."

This is the highest-risk, highest-payoff skill in the suite. It pairs with
`/explorer` (per-area paper cuts), `/tiger` (the LLM call sites), `/code-review`
and `/simplify` (per-diff) — those handle the small and the medium; `/architect`
handles the large.

**Anchor:** `context-map.json` at the repo root (25 contexts, 258 files, `groups:
[]` — the map is flat here; `category` is the only taxonomy axis) plus the
crate-layering rules in `.claude/CLAUDE.md`.

**Deliverable contract — every finding ends as an artifact.** An architect run
that produces observations without enforcement artifacts is a failed run. Each
finding that survives triage must terminate as exactly one of: **(a)** an
ADR-style vault note (Phase 7b/8a), **(b)** a clippy lint / CI gate / structural
test proposal (Phase 7B), or **(c)** a scoped rollout plan with per-commit steps
(the ADR's Rollout section). Only `drop` verdicts are artifact-free, and they are
recorded with a reason. Reaching Phase 12 with a surviving finding that has none
of the three means the run is not done.

## Interaction conventions

Built for parallel CLI control — every user prompt is single-keystroke answerable.

- **Every prompt is a numbered menu.** Numeric input picks the option; **Enter**
  triggers the default; option `1. other → …` is the deviation lane (free text).
- **Every phase output ends with a `Next?` block** of 2–5 numbered next-step
  actions. Replying with a digit advances the run.
- Multi-finding triages use `<id>=<verdict-number>` (e.g. `1=2 2=1 3=3`);
  `all=<n>` and `ask` shortcuts are always accepted.
- Long free-text answers are still accepted everywhere.

## Input

### Q1 — Mode

```
Mode? (Enter = scan)
  1. scan      — pick a theme, parallel-agent sweep        ← default
  2. area      — bound the sweep to one crate / context group
  3. resume    — drain the backlog (skip scanning)
```

`resume` skips the rest of Input — jump straight to Phase 9.

### Q2a — Theme (scan mode)

```
Theme? (Enter = pick for me)
  1. other → describe (free-form theme; angles auto-picked in Phase 3a)
  2. rls-and-tenancy        — scoped_tx discipline, SET LOCAL, brainiac_app role
  3. crate-layering         — core/store/pipeline/gateway/eval/publish/server boundaries
  4. error-handling         — anyhow + .context, unwrap_used, tracing/Sentry surfaces
  5. sql-and-migrations     — sqlx query shape, append-only numbered migrations, indexes
  6. async-workers          — pipeline job queue, tick loops, cancellation, backpressure
  7. provider-gateway       — ChatProvider spectrum, resilience, telemetry, embedders
  8. api-type-sync          — utoipa → openapi.json → console api-schema.d.ts
  9. eval-gates             — fixture profiles, baselines, hard gates, drift
  10. console-architecture  — App Router, server-only data access, generated types
  11. testing-strategy      — pg-backed self-skip tests, mock-server coverage, fixtures
  12. pick for me   ← default (uses Architect/coverage.md staleness)
```

Theme is required for scan mode — `pick for me` is fine, but a one-word vague
free-form theme yields shallow findings; if option 1's input is too thin, re-ask.

### Q2b — Area (area mode)

`context-map.json` has **no groups** (`"groups": []`), so area mode resolves
against the **category** axis and the **crate/dir** axis instead:

```
Area? (Enter = pick for me)
  1. other → type a hint (path fragment, keyword, or exact context name)
  2. core         — crates/brainiac-core       (shared-core, library-core)
  3. store        — crates/brainiac-store      (api-keys, org-directory, projects, document-wiki)
  4. pipeline     — crates/brainiac-pipeline   (extraction-pipeline, sweep-pipeline, standards-governance)
  5. gateway      — crates/brainiac-gateway    (llm-gateway)
  6. server       — crates/brainiac-server     (server-core, auth-middleware, onboarding)
  7. eval+fixtures— crates/brainiac-eval, fixtures/   (eval-harness)
  8. publish      — crates/brainiac-publish    (kb-publishing)
  9. console      — console/                   (console-shell, analytics-dashboard, library-landing, …)
  10. data+deploy — migrations/, Dockerfile, docker-compose*  (database-schema, deploy-config)
  11. pick for me   ← default
```

Resolve the chosen area to a concrete file list by filtering `context-map.json`'s
`contexts[].file_paths` by prefix; name the contexts covered so scope is explicit.
Option 1's free text falls through to the resolver (exact `name`, path fragment
against `file_paths`, or keyword against `keywords`). Scan is bounded to that area
but still cross-cutting within it.

If the user's first message is ambiguous (e.g. just `/architect`), present Q1; if
they typed `resume`, skip to Phase 9.

---

## Constants

- **Codebase reference files:**
  - `context-map.json` (repo root) — machine-readable context→file map (25
    contexts; fields: `name`, `category`, `description`, `entry_points`,
    `file_paths`, `keywords`, `api_surface`, `db_tables`, `cross_refs`,
    `tech_stack`). **The authority** for what a context owns. If a decision moves
    files between contexts, update this file in the same change (`.claude/CLAUDE.md`).
  - `.claude/CLAUDE.md` — project rules: crate layering, errors, type-crossing via
    utoipa, quality standards (the 5 gates), design standards (RLS, core-vs-store,
    append-only migrations, KB-as-projection, console rules).
  - `docs/ARCHITECTURE.md`, `docs/PLAN.md` (§ Crate map), `docs/EVAL.md` (§3.2
    regression gates), `README.md` — the hand-written architecture narrative.
  - `crates/brainiac-store/src/lib.rs` header — the RLS contract in prose.
  - Domain plans: `docs/KB-PLAN.md`, `docs/LIBRARY-PLAN.md`, `docs/PROJECT-PLAN.md`.
- **Vault root** (resolved at Phase 0), then `$VAULT/Architect/`:
  - `Architect/scans/` — one note per scan run, the synthesis output
  - `Architect/decisions/` — one ADR per accepted decision
  - `Architect/backlog.md` — durable queue of accepted decisions with status
  - `Architect/strong-patterns.md` — load-bearing patterns kept for codification
  - `Architect/weak-patterns.md` — anti-patterns, with affected files
  - `Architect/coverage.md` — themes/areas previously scanned, staleness
  - `Patterns/architect-preferences.md` — distilled rules across runs
  - `Lessons/{date}-architect.md` — append-only self-reflection
- **Categories of finding** — `weak-pattern | strong-pattern | tech-swap |
  structural-bug-class | convention-gap`
- **Risk** — 1 (low, isolated) … 5 (production-critical surface: anything touching
  RLS, migrations, or the eval hard gates starts at 4)
- **Effort** — `s | m | l | xl`
- **Reach** — concrete: "{N} files / {M} call sites / {K} contexts" — never vague.
- **Payoff** — 1 (incremental) … 5 (eliminates a recurring bug class or unblocks a
  major future)

---

## Coordination — concurrent sessions

Before materially editing the working tree (which `/architect` always does in
Phase 7), read `.claude/active-runs.md` (create it with `## Active` /
`## Recently completed` if missing) and append one entry declaring scope. If a
`started` entry <2h old overlaps your area, surface the conflict before
proceeding. Overlap on `.claude/active-runs.md` itself is not a conflict.

**Ledger edits are a single bash invocation** (read + append/move in one command)
— parallel sessions rewrite the file between an Edit-tool read and its write.
Always edit the ledger in the **main checkout**, even when code work happens in a
worktree.

**Declared paths for `/architect`:**
- Vault: `Architect/scans/<run>.md`, `Architect/decisions/<adr>.md`,
  `Architect/backlog.md`, `Architect/strong-patterns.md`,
  `Architect/weak-patterns.md`, `Lessons/{date}-architect.md`
- Working tree (varies by area): typically a subset of `crates/brainiac-*/src/`,
  `console/src/`, `migrations/`, `docs/`
- Always: `.claude/active-runs.md`

**At session end**: move your entry to `## Recently completed` with
`completed (commit: <sha>)` or `aborted (<reason>)`. Trim entries >14 days.

---

## Phase 0: Resolve vault path

Same resolution the repo's `/perfect` skill uses, so all skills share one vault:

```bash
for v in "C:/Users/mkdol/Documents/Obsidian/brainiac" "C:/Users/kazda/Documents/Obsidian/brainiac"; do
  [ -d "$v" ] && VAULT="$v" && break
done
# First run: if an Obsidian root exists (C:/Users/*/Documents/Obsidian) but no
# brainiac vault, CREATE the folder there.
# Portable fallback: no Obsidian root at all → use <repo>/.architect/
#   (same schema — still an Obsidian-openable folder; gitignore it).
```

Record `$VAULT` for the rest of the run.

### Bootstrap (one-time per vault)

If any of these are missing, create them:

- `$VAULT/Architect/`, `$VAULT/Architect/scans/`, `$VAULT/Architect/decisions/`
- `$VAULT/Architect/backlog.md`:
  ```markdown
  # Architect Backlog

  Durable queue of architectural decisions. Sorted manually by priority.
  Status values: `proposed | approved | in-progress | shipped | abandoned | blocked`.

  ## Pending
  _No pending decisions._

  ## Shipped
  _None yet._

  ## Abandoned / Blocked
  _None yet._
  ```
- `$VAULT/Architect/strong-patterns.md`:
  ```markdown
  # Strong Patterns

  Load-bearing patterns identified by `/architect`. Promote-worthy: ideally these
  graduate into clippy lints, CI gates, structural tests, or CLAUDE.md conventions.

  ## Patterns

  _No patterns yet._
  ```
- `$VAULT/Architect/weak-patterns.md`:
  ```markdown
  # Weak Patterns

  Anti-patterns identified by `/architect`, with reach data. Each entry should
  eventually convert into a backlog decision (or be explicitly accepted as
  "tolerable for now" with a reason).

  ## Patterns

  _No patterns yet._
  ```
- `$VAULT/Architect/coverage.md`:
  ```markdown
  # Architect Coverage

  Heatmap of themes and areas scanned, with last-scan date.

  ## Themes
  _No themes scanned._

  ## Areas
  _No areas scanned._
  ```
- `$VAULT/Patterns/architect-preferences.md`:
  ```markdown
  # Architect Preferences (distilled from /architect runs)

  > Rules upgraded from `Lessons/` after 3+ observations. Loaded by Phase 1.

  _No patterns yet. Will be populated as runs accumulate._
  ```

`Lessons/` is shared with the other skills — don't recreate.

---

## Phase 1: Load context & memory

### 1a. Required-file check

`context-map.json` and `.claude/CLAUDE.md` must exist. If `context-map.json` is
missing, stop and tell the user to re-run the Vibeman/Personas context scan — do
not improvise a taxonomy.

### 1b. Read in order

1. `.claude/CLAUDE.md` — **most important** for architect: crate layering, RLS,
   migrations, type-crossing, the quality gates. Read in full.
2. `context-map.json` — the context taxonomy and file lists (use the `index`/
   `description` fields for a fast overview; don't dump all 258 paths).
3. `docs/ARCHITECTURE.md` + `docs/PLAN.md` (§ Crate map) + `docs/EVAL.md` §3.2.
4. `$VAULT/Architect/strong-patterns.md` — what's already load-bearing.
5. `$VAULT/Architect/weak-patterns.md` — what's already on the radar.
6. `$VAULT/Architect/backlog.md` — what's pending or in-progress.
7. `$VAULT/Architect/coverage.md` — staleness signals.
8. `$VAULT/Patterns/architect-preferences.md` — finding shapes the user rejects.
9. The 3 most recent `$VAULT/Lessons/*-architect.md`.

Optional prior art: this repo is wired to Brainiac org memory itself. Before
proposing, you may `POST $BRAINIAC_API_URL/v1/memories/search {"query": …,
"k": 5}` with the bearer token from `.env` (never print it) to see whether a
decision was already made and why. 0 hits is normal on a young org — treat it as
a bonus source, never a blocker.

### 1c. Snapshot freshness

Compare `context-map.json`'s `generated_at` to today and to `git rev-list --count
HEAD`. Warn (don't block) if it is >30 days old or the repo has advanced far since.

### 1d. Aging strong-patterns review

Parse `$VAULT/Architect/strong-patterns.md`. For each entry compute age = today −
`Identified`. If `Codification status: noted` AND age > 60 days AND no
`Last reviewed` within 30 days → mark **aging**. Surface in Phase 5 alongside new
findings. Gentle pressure, not nagging — a pattern may stay `noted` if the user
snoozes. If status is already `lint-added`, `docs-written`, or `test-guard-added`,
it is codified; don't flag it.

---

## Phase 2: Mode dispatch

Scan mode → Phase 3. Area mode → Phase 3 with the area scope override applied to
all sub-agent prompts. Resume mode → Phase 9.

---

## Phase 3: Parallel scan (scan + area modes)

Spawn **3–5 `Explore` sub-agents in parallel**, each looking at the theme/area
from a different angle.

### 3a. Pick the angles

Default angles:
1. **Usage map** — where does this concept appear? Count call sites, group by
   crate/context. Identify shape variation.
2. **Type/contract** — are types consistent? Are crate boundaries respected? Any
   leaky abstractions (SQL in `core`, business math in `store`, hand-written
   request shapes in the console)?
3. **Failure mode** — what happens when this fails? `anyhow` context quality,
   recovery, observability (`tracing` → Sentry), silent `Ok(())`.
4. **Performance surface** — hot paths, N+1 queries, per-row `embed()` calls,
   sync work in an async worker, unbounded prompt/context growth.
5. **Test coverage** — is this tested at the right layer? `core` unit tests,
   pg-backed `store` tests (do they self-skip and therefore prove nothing in CI?),
   mock-server gateway tests, eval fixtures.

Theme → angle mapping:
- `rls-and-tenancy` → 1, 2, 3 + "unscoped-query audit: every `sqlx` call site vs
  `scoped_tx`".
- `crate-layering` → 1, 2 + "dependency direction audit (who depends on whom in
  Cargo.toml vs the declared layering)".
- `error-handling` → 1, 2, 3, 5.
- `sql-and-migrations` → 2, 4 + "migration history vs live schema drift" +
  "index coverage for the hot retrieval queries".
- `async-workers` → 1, 3, 4 + "cancellation & idempotency of job-queue ticks".
- `provider-gateway` → 1, 2, 3 + "resilience/telemetry uniformity across the 5
  providers" (coordinate with `/tiger`, which owns the call-site lens).
- `api-type-sync` → 2 + "utoipa annotation coverage vs openapi.json" +
  "console types generated vs hand-written".
- `eval-gates` → 5 deeply + "baseline provenance" + "hard-gate coverage".
- `console-architecture` → 1, 2, 4 + "server-only boundary" + "data-access shape".
- `testing-strategy` → 5 deeply + "fixture duplication" + "what CI actually
  exercises without DATABASE_URL".

If `area` mode, every angle is bounded to that area's `file_paths`.

### 3b. Sub-agent prompt template

Self-contained; use `Explore` (read-only).

```
You are scanning the Brainiac codebase for {angle name}.

Repo shape: Rust workspace, 8 crates under crates/brainiac-* (core = pure domain
math, no DB; store = Postgres data plane + RLS; pipeline = async workers;
gateway = BYOM chat/embed providers; eval = fixture profiles + regression gates;
fixtures = the synthetic "Meridian" org; publish = KB export; server = axum HTTP
+ MCP + clap CLI). Next.js 15 console in console/. Postgres 16 + pgvector.

Theme: {theme}
{If area mode:} Scope: only files under {area paths from context-map.json}
Background: {1 paragraph from .claude/CLAUDE.md / docs/ARCHITECTURE.md relevant to the theme}

Specific questions:
1. {question 1 tailored to angle}
2. {question 2}
3. {question 3}

Report format (Markdown):
- Files inspected: {list, capped at top 30 by relevance}
- Observed shapes: {distinct patterns found, with file:line examples for each}
- Inconsistencies: {where shapes diverge — call out specific files}
- Outliers: {any single file doing it differently from the rest}
- Smell strength: 1-5 (1 = healthy, 5 = active drag on the codebase)
- Cross-references: {where this angle interacts with other crates/contexts}

Budget: 30-60 minutes of equivalent work. Don't enumerate every match — sample
strategically and report shape, not exhaustive detail. Read-only: do not edit.
```

Run all sub-agents **in parallel** (single message, multiple `Agent` calls).

### 3c. Synthesize

Merge into a single pattern model. Look for:
- **Convergence** — multiple angles flagging the same crate/context → high confidence.
- **Conflict** — one angle calls something a strength, another a weakness →
  usually context-dependent; investigate.
- **Surprise** — something none of the angles expected → likely the most valuable
  finding of the run.
- **Reach quantification** — every weakness gets a concrete count: "31 call sites
  across 4 crates / 3 contexts."

If reports are thin (smell strengths all 1–2), the area is healthy in this theme.
**Say so explicitly** and offer a different theme or a downgrade to "passive scan,
no findings to action." Don't manufacture findings to fill a quota.

### 3d. Output structure

- 0–8 **weak-pattern** findings with reach, risk, effort, payoff.
- 0–4 **strong-pattern** findings worth codifying.
- 0–2 **tech-swap** proposals — only when smell strength is ≥4 AND the swap
  unlocks payoff a refactor can't. (A swap touching `sqlx`, `axum`, or pgvector is
  a months-scale project; say so honestly.)
- 0–3 **structural-bug-class** findings (recurring bugs whose root is structural
  — e.g. "every new store module re-implements the scope stamp instead of taking
  `scoped_tx`; fix the missing helper, not the 9 call sites").

Cap total findings at **8**; rank by `(reach × payoff) / (risk × effort)` and drop
the bottom.

---

## Phase 4: Surface against existing memory

Cross-check every finding against `strong-patterns.md` (flag conflicts
explicitly), `backlog.md` (merge duplicates: "previously proposed in
[[backlog#decision-N]], re-confirming with new reach data"), and
`weak-patterns.md` (update reach/risk instead of creating a new entry). This
prevents "we keep finding the same thing every quarter and never doing it."

---

## Phase 5: Present findings

### Summary table

```
#   Type                   Sev    R   E    Reach                                Title
─   ────────────────────   ────   ─   ──   ───────────────────────────────────  ──────────────────────────────
1   weak-pattern           {sev}  {R} {E}  {N call sites / M crates / K contexts} {title}
2   structural-bug-class   ...
3   tech-swap              ...
4   strong-pattern          —     —   —    {concrete count}                      {title} — codify
```

(Shape only. Every row must carry a measured reach and `file:line` evidence from
this run — never a placeholder or a remembered finding.)

R = risk (1-5), E = effort (s/m/l/xl). Strong patterns have no risk/effort.

### Per-finding detail

For weak-pattern / structural-bug-class / tech-swap:

```
[N] {title}
    Type:        {weak-pattern | structural-bug-class | tech-swap}
    Reach:       {concrete count}
    Risk:        {1-5} — {what could break, recovery path}
    Effort:      {s/m/l/xl} — {scan/migrate/test ratio}
    Payoff:      {1-5} — {what this unlocks, what bug class it eliminates}
    Contexts:    {context-map.json names touched}

    Current shape:
      {2-3 sentences, with 2-3 file:line examples showing the variation}

    Proposed shape:
      {2-3 sentences, with one canonical example file:line where it's already
       done right (or a sketch)}

    Migration plan (sketch):
      {3-7 numbered steps, each independently shippable. Note breaking vs
       additive. Ballpark commit count. Flag any migration/RLS/baseline step.}

    Risks:
      - {risk 1, with mitigation}
      - {risk 2}

    Already-on-radar: {link to weak-patterns.md entry or backlog item if any}
```

For strong-pattern:

```
[N] {title}
    Type:           strong-pattern
    Reach:          {concrete count}
    Why it works:   {2-3 sentences}
    Codification:   {clippy lint? CI gate? structural test? .claude/CLAUDE.md note? docs/ARCHITECTURE.md section?}
    Risk to losing: {concrete bug shape if it drifts}
```

### Aging strong patterns (from Phase 1d)

```
Aging strong patterns (noted but not codified):

[A1] {title}  — noted {date} ({N} days ago)  → [[Architect/strong-patterns#{title}]]
```

---

## Phase 6: Triage

```
For each finding, pick a verdict:
  1. execute now    — implement this one in this session
  2. queue          — accept as backlog decision; defer       ← default
  3. drop           — not worth pursuing
  4. rework         — true gap, wrong proposed shape

Reply with `<finding>=<verdict>`, space-separated. Examples:
  "1=2 2=1 3=3 4=4"   →  1 queued, 2 executed, 3 dropped, 4 reworked
  "all=2"             →  queue everything
  "ask"               →  guided walkthrough item-by-item
  Enter               →  same as "all=2"   ← default
```

- **execute now** → Phase 7. Only one per session is recommended; if the user
  picks more, warn ("doing N structural changes in one session is high-risk —
  pick the highest priority and queue the rest?") and allow override.
- **queue** → Phase 8 (ADR + backlog).
- **drop** → record in the scan note as `decided: dropped` with reason;
  pattern-track in Lessons (Phase 10).
- **rework** → ask "what shape would actually fit?", capture, re-present. If no
  clear redo, queue as `proposed (needs reshape)`.

Strong-pattern triage:
```
  1. codify   — proceed to Phase 7B; ship clippy lint / docs / test guard
  2. note     — record in strong-patterns.md but defer codification   ← default
  3. drop     — not actually load-bearing; do NOT persist
```

Aging strong-pattern triage:
```
  1. codify    — same Phase 7B path (aging is meant to push toward this)   ← default
  2. snooze    — bump Last reviewed to today; won't re-surface for 30 days
  3. drop      — no longer load-bearing; remove from strong-patterns.md
```

---

## Phase 7: Execute (one decision, this session)

### 7a. Isolation: worktree by default

Structural rollouts are almost always multi-file, so the default is a dedicated
worktree under the repo's existing `.claude/worktrees/`:

```
Isolation for this decision:

  1. worktree .claude/worktrees/architect-{slug}   ← default (mandatory for multi-file rollouts)
  2. commit on current checkout                    ← only if the rollout touches a single file

Pick 1 or 2 (Enter = 1).
```

```bash
git worktree add .claude/worktrees/architect-{slug} -b worktree-architect-{slug}
cd .claude/worktrees/architect-{slug}
```
Work and commit inside the worktree; vault writes and `.claude/active-runs.md`
edits still target the main checkout. After the branch is merged into `master`
and confirmed in `git log master`: `git worktree remove
.claude/worktrees/architect-{slug}` then `git branch -D
worktree-architect-{slug}` (Phase 12 ritual).

Note: a worktree needs its own `.env` (or exported `DATABASE_URL`) for pg-backed
tests, and `console/node_modules` is not shared — run `npm ci` there if the
decision touches the console.

Option 2 is legitimate only for genuinely single-file changes. If the user insists
on the main checkout for a multi-file rollout, warn once, then honor it.

### 7b. Write the ADR first

Before any code change, write `$VAULT/Architect/decisions/{YYYY-MM-DD}-{slug}.md`:

```markdown
---
date: 2026-07-26
slug: {slug}
status: in-progress
type: weak-pattern | structural-bug-class | tech-swap
reach: "{concrete count}"
contexts: [{context-map.json names}]
risk: {1-5}
effort: {s/m/l/xl}
payoff: {1-5}
branch: worktree-architect-{slug} | "(committed on main checkout)"
related_scan: [[Architect/scans/{date}-{theme}]]
---

# {Title}

## Context
{Codebase reality today, with file:line examples. ~1 paragraph.}

## Decision
{What we're going to do. Be specific about scope and which crates it touches.}

## Consequences
### Positive
- {what we gain}
### Negative / risks
- {what we lose or risk}
### Mitigations
- {pre-flight checks, rollback plan}

## Rollout
{Numbered atomic commits. Each independently shippable.}
1. {step 1} — {validation: cargo fmt / clippy -D warnings / cargo test / eval profile / tsc / vitest}
2. ...

## Acceptance criteria
- {observable criterion 1}
- ...

## Regression checklist
- [ ] RLS: no new query path outside `scoped_tx`; pg tests ran with DATABASE_URL set
- [ ] Migrations: append-only, next number, no shipped migration edited
- [ ] Eval: hard gates (RLS leaks / superseded-in-top-3 = 0) still 0; no baseline rewritten
- [ ] OpenAPI: `openapi.json` regenerated + `npm run gen:api` if types crossed the boundary
- [ ] context-map.json updated if file→context ownership changed
- [ ] {area-specific item} — verified by: {how}
```

### 7c. Pre-flight checks

In a fresh worktree the tree starts clean — skip to step 3. On the main checkout,
**do NOT require a clean working tree.** Concurrent sessions share it.

1. **Inspect:** `git status --short`. Read every modified/untracked path.
2. **Classify** each path as: *in-flight by someone else* (leave strictly alone),
   *pre-existing in your touch zone* (surface to the user:
   ```
   File X already has uncommitted changes. This decision wants to edit it.
     1. commit those changes first (you do it; I'll re-inspect)
     2. let me commit on top of them   ← default
     3. abort — pick a different decision
   ```
   ), or *yours from this session*.
3. **Capture validation baselines** (record in the ADR under `## Pre-flight
   baseline` — subsequent commits compare against these, not against zero):
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   docker compose up -d   # Postgres+pgvector on :5433
   export DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac
   cargo test --workspace
   cd console && npx tsc --noEmit && npx vitest run && cd ..
   ```
   If `DATABASE_URL` is unset the pg tests **self-skip** — a green run then proves
   nothing about the data plane. Record whether the DB was up.

**Forbidden during pre-flight and every later phase:** `git stash` (never, not
even `--keep-index`), `git reset --hard|--merge`, `git restore` / `git checkout
--` on working-tree paths you didn't author, `git clean -f[dx]`, and
`git add -A` / `git add .` / `git add -u`.

If a working-tree path conflicts with your edits and the user won't commit it
first, abort the decision and queue it back with `blocked: working-tree-conflict`.

### 7d. Atomic commits per rollout step

For each step in the ADR's Rollout:

1. Apply the changes.
2. Run that step's validation.
3. **Compare to baseline** — clippy must stay at zero warnings (`-D warnings` is
   the gate), fmt clean, tests at the baseline pass rate, tsc/vitest clean.
4. If validation regresses → fix inline. Do NOT stack failing commits. No
   `--no-verify`, no `--amend`.
5. Stage **only the paths this step touched** — `git add path/one path/two`.
6. **Verify the staged index:** `git diff --cached --stat`. If more files are
   staged than you added, `git restore --staged <path>` each unrelated file.
7. Commit with an `architect: <step title>` subject (or a conventional
   `refactor(<crate>): …` matching `git log`), Co-Authored-By footer, body
   referencing the ADR by wikilink.
8. Record the SHA in the ADR's Rollout section as you go.

### 7e. Final regression sweep

1. Run the full gate set (the `.claude/CLAUDE.md` § Quality standards list):
   `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D
   warnings`; `cargo test --workspace` **with Postgres up**; the relevant eval
   profile against its committed baseline
   (`cargo run -p brainiac-server -- eval --fixtures fixtures/v1 --profile <p>
   --baseline results/<p>-baseline.json --out results/ci-run.json`); console
   `npx tsc --noEmit && npx vitest run && npm run build`.
2. Walk the ADR's regression checklist; verify each item by actually running the
   path where possible (`cargo run -p brainiac-server -- serve`, the console at
   :3100).
3. **If any checklist item is unverified, do not mark the ADR `shipped`.** Mark
   `in-progress` with a "needs verification" note and queue the verification.

**Never** run `--write-baseline` as part of an architect rollout. A baseline
change is its own deliberate, separately committed diff with a reason
(`docs/EVAL.md` §3.2).

### 7f. Update ADR status

All steps committed + checklist green → frontmatter `status: shipped`, add
`commits: [<sha>, …]`, move the backlog entry from Pending to Shipped. Partial →
stays `in-progress` with the remaining steps recorded.

### 7g. Boundary rules — non-negotiable

- **RLS**: every runtime query runs through a transaction stamped with the
  caller's `app.org_id`/`app.user_id` (`SET LOCAL`) under the non-owner
  `brainiac_app` role. There is no unscoped query path. `migrate()` runs DDL as
  admin; the runtime pool drops to the constrained role in `after_connect`. Never
  widen a read to bypass `scoped_tx`.
- **Layering**: scoring, fusion, reranking, temporal validity, health stay pure in
  `core`; `store` only maps rows ↔ those types and enforces RLS. No SQL in `core`,
  no business math in `store`.
- **Migrations** are append-only and numbered (`migrations/NNNN_*.sql`, applied by
  `sqlx::migrate!`). Add the next number; never edit a shipped migration.
- **The KB is a projection**, not a second source of truth — no bidirectional sync.
- **Types cross the API boundary via `utoipa`** → `openapi.json` → console
  `npm run gen:api`. Never hand-write request/response shapes in TS.
- **Console**: App Router + React 19 + Tailwind 4; `NEXT_DIST_DIR=.next-build` for
  build/start; server-only data access stays behind `server-only`; the honesty
  guard `console/src/kb/kb-data.test.ts` must stay green.
- If the decision changes which files a context owns, **update
  `context-map.json`** in the same change.

### 7h. Verification of runtime behavior

For decisions that change runtime surfaces (HTTP routes, MCP tools, worker
behavior, console pages): actually run it — `cargo run -p brainiac-server --
serve` (:8600), `-- worker`, `-- mcp`, or `npm run dev` in `console/` (:3100) —
and exercise the affected surface. State explicitly when you have NOT verified at
runtime; never claim "works" from code review alone.

---

## Phase 7B: Codify strong patterns

For every strong pattern (new or aging) marked `codify`. Multiple codifications
can run in one session — independent and lower-risk than a Phase 7 execution.

### 7B.a. Pick the vehicle

```
How should "{pattern title}" be codified? Pick one or more:

  1. lint         — a clippy lint in [workspace.lints.clippy] (Cargo.toml) or a
                    crate-level #![deny]/#![warn] attribute
  2. ci-gate      — a step in .github/workflows/ci.yml (fmt/clippy/test/eval already there)
  3. docs-claude  — a convention in .claude/CLAUDE.md (loaded into every session)
  4. docs-arch    — a section in docs/ARCHITECTURE.md (or the relevant docs/*-PLAN.md)
  5. test-guard   — a structural test asserting the invariant (Rust #[test] in the
                    owning crate, or a vitest test under console/src/**)
  6. multiple     — a combination (e.g. "1+3")
```

**Which vehicle fits:**
- A mechanically-detectable code shape → `lint` (the workspace already carries
  `[workspace.lints.clippy] unwrap_used = "warn"` at `Cargo.toml:59` — that's the
  precedent). Falls back to `docs-claude` if too contextual to lint.
- An architectural boundary (crate layering, RLS, KB-as-projection) →
  `docs-claude` (it is already the home of these rules) + ideally `test-guard`.
- A cross-file invariant countable by scanning the tree → `test-guard`.
- A property CI must never regress → `ci-gate`.

If the user picks `multiple`, codify each vehicle in a separate atomic commit.

### 7B.b. Lint vehicle

1. Read `Cargo.toml`'s `[workspace.lints.clippy]` and how crates opt in
   (`[lints] workspace = true` in each crate's Cargo.toml).
2. Add the lint at the workspace level (or a targeted `#![warn(...)]` in the one
   crate that needs it). Default severity `warn`; use `deny` only if the user says
   "ship blocker" — note that CI runs `clippy -- -D warnings`, so a `warn` there
   is already effectively blocking.
3. Run `cargo clippy --workspace --all-targets -- -D warnings`. If the new lint
   produces a large violation set, **pause** — either the pattern is less
   load-bearing than thought, or the rollout needs its own ADR (fix-then-enable).
4. Commit: `architect: codify <pattern> as a clippy lint` — body explains the
   lint, severity, and the violation count at enable time.

### 7B.c. CI-gate vehicle

1. Read `.github/workflows/ci.yml` — the local self-verify bar mirrors it exactly.
2. Add the step next to its siblings (fmt → clippy → test → eval). Keep the local
   command identical to the CI command so `.claude/CLAUDE.md`'s "run it before you
   call a change complete" list stays true; update that list in the same commit.
3. Commit: `architect: codify <pattern> as a CI gate`.

### 7B.d. Docs vehicle (claude or arch)

1. Read the target file. 2. Find the right insertion point (`.claude/CLAUDE.md` →
"Conventions" or "Design standards"; `docs/ARCHITECTURE.md` → the matching
section). 3. Write it: name, why it's load-bearing, canonical example with
`file:line`, the anti-shape, and a pointer to the lint/test if `multiple`.
4. Keep it 10–25 lines — long convention docs go unread. 5. Commit:
`architect: codify <pattern> in <file>`.

### 7B.e. Test-guard vehicle

1. Look at existing invariant-style tests for the shape (e.g. the console honesty
   guard `console/src/kb/kb-data.test.ts`; the gateway mock-server tests under
   `crates/brainiac-gateway/tests/`).
2. Write the guard in the owning crate (`#[test]` walking the tree with
   `std::fs`, or an integration test under `crates/<crate>/tests/`) or as a vitest
   test in `console/src/`.
3. Give it a failure message that names the strong-pattern entry and the rule.
4. Run `cargo test --workspace` (or `npx vitest run`) and confirm it passes today.
5. Commit: `architect: codify <pattern> as a structural test guard`.

### 7B.f. Update the strong-patterns entry

`Codification status: lint-added | ci-gate-added | docs-written |
test-guard-added` (or a combination), plus `Codified: {date}` and
`Codification ADR: [[Architect/decisions/{date}-codify-{slug}]]`. If docs were
used: `Docs at: .claude/CLAUDE.md#<anchor>`. If a lint: `Lint: Cargo.toml
[workspace.lints.clippy] <name>`.

### 7B.g. Mini-ADR

`$VAULT/Architect/decisions/{YYYY-MM-DD}-codify-{slug}.md`:

```markdown
---
date: 2026-07-26
slug: codify-{slug}
status: shipped
type: codification
vehicle: lint | ci-gate | docs-claude | docs-arch | test-guard | combination
parent_strong_pattern: [[Architect/strong-patterns#{title}]]
related_scan: [[Architect/scans/{date}-{theme}]]
commits: [<sha>]
---

# Codify: {pattern title}

## Why now
{"noted N days ago, surfaced as aging" or "identified this run, smell-strength enough to enforce"}

## Vehicle and rationale
{which vehicle, why it fits}

## Rollback
{how to undo — e.g. "drop the lint; the underlying pattern stays noted"}
```

### 7B.h. Aging `snooze` / `drop`

`snooze` → set `Last reviewed: {today}` and `Snoozed until: {today+30d}`; commit
only if it's the run's sole change. `drop` → delete the entry entirely and append
to `Lessons/{date}-architect.md`: `- Dropped strong pattern "{title}" — original
date {date}, reason: {user reason}.` No zombie entries.

---

## Phase 8: Backlog the queued decisions

### 8a. Stub ADR
Same template as 7b with `status: proposed`, sketchy Rollout, no commits/branch.
Save to `$VAULT/Architect/decisions/{YYYY-MM-DD}-{slug}.md`.

### 8b. Append to the backlog

```markdown
- **[{date}] {Title}** — type: {type}, risk: {N}, effort: {s/m/l/xl}, payoff: {N}, reach: {concrete}
  Contexts: {context-map.json names}
  ADR: [[Architect/decisions/{date}-{slug}]]
  Source scan: [[Architect/scans/{date}-{theme}]]
  Status: proposed
  Notes: {any user input from triage}
```

Sort Pending by `(reach × payoff) / (risk × effort)` descending.

### 8c. Update weak-patterns.md / strong-patterns.md

Weak:
```markdown
## {Pattern title}

- First seen: {date}  /  Last seen: {date}
- Reach: {count, current}   Reach trend: {growing | stable | shrinking}
- Contexts: {names}
- Backlog item: [[Architect/backlog#decision-N]] (or "no decision queued yet")
- Examples: `{file:line}`, `{file:line}`, `{file:line}`
```

Strong — write only when the verdict is `note` or `codify`. **Never persist an
entry the user marked `drop`.**
```markdown
## {Pattern title}

- Identified: {date}
- Reach: {count}
- Why it works: {1 sentence}
- Codification status: noted | lint-added | ci-gate-added | docs-written | test-guard-added | combination
- Last reviewed: {date}
- Examples: `{file:line}`, `{file:line}`
```

---

## Phase 9: Resume mode

### 9a. Read the backlog
Print Pending as a numbered table (`R/E/P` = risk / effort / payoff, plus Reach
and Contexts).

### 9b. Pick one
```
Which to execute now? Reply with a decision number (1–{N}).

Shortcuts:
  open N   — read the full ADR before deciding (re-asks after)
  abort    — bail; nothing executes
  Enter    — pick decision #1 (top-priority)   ← default
```

### 9c. Refresh the ADR
Re-verify `file:line` anchors still exist; re-count reach; `git log` the touched
files for conflicts; check whether a step already shipped. If anything material
changed, **stop and present the delta** — proceed, reshape, or abandon.

### 9d. Execute
Jump to Phase 7a and run 7b–7h normally.

---

## Phase 10: Self-reflection

### 10a. Ask why for dropped findings
```
For the dropped findings, why did you drop them?

  [3] {title}
  [5] {title}

Per-item reasons or one overall reason.

Shortcuts:
  skip    — record "no reason given"
  Enter   — same as "skip"   ← default
```

### 10b. Append to Lessons
`$VAULT/Lessons/{YYYY-MM-DD}-architect.md`:
```markdown
## Run: {timestamp} — {theme or area} ({mode})

Sub-agents spawned: {N} angles
Findings surfaced: {weak: M, strong: K, swap: J, struct-bug: L}
Triage:
  - executed: [list]
  - queued: [list]
  - dropped: [list]
  - reworked: [list]

### Drop reasons
- [3] {reason}

### Self-reflection
- Sub-agent angles that produced strong signal: {which}
- Sub-agent angles that produced noise: {which}
- Synthesis miss: {anything I framed wrong that the user corrected}
- Calibration drift: {e.g. "rated 4 findings 'high payoff', user dropped 2"}
- Re-usable insight for future scans: {1 sentence}
```

### 10c. Backfill the scan note (drop reasons — see Phase 11).

### 10d. Pattern promotion
Read all `Lessons/*-architect.md`; after 3+ repeats of a drop reason, propose
adding it to `$VAULT/Patterns/architect-preferences.md`.

### 10e. Durable-fact check
Did this run discover a structural fact future runs need? Architect runs
especially surface boundaries the map didn't have. If yes: edit `.claude/CLAUDE.md`
or `docs/ARCHITECTURE.md` directly (tagged with the run date), and — if it is a
generalizable decision or pitfall — write it to Brainiac org memory:
`POST $BRAINIAC_API_URL/v1/memories {"content": "<one self-contained statement>"}`
with the bearer token from `.env` (never printed). Facts, not transcripts.

### 10f. Update coverage.md
```markdown
### {theme}
- Last scanned: {date}
- Last scan: [[Architect/scans/{date}-{theme}]]
- Findings (last 3 scans): [8, 4, 6]
- Findings actioned: [3, 1, 2]
- Yield density: {actioned / surfaced}
- Notes: {observations across runs}
```

---

## Phase 11: Persist the scan

`$VAULT/Architect/scans/{YYYY-MM-DD}-{theme-or-area-slug}.md`:

```markdown
---
date: 2026-07-26
mode: scan | area | resume
theme: crate-layering | (n/a for resume)
area: pipeline | (n/a unless area mode)
contexts: [{names touched}]
sub_agents_spawned: 4
findings_total: 8
findings_weak: 5
findings_strong: 2
findings_swap: 1
findings_struct_bug: 0
executed: [2]
queued: [1, 3, 5]
dropped: [4, 6]
reworked: [7, 8]
adrs_written: ["[[2026-07-26-scope-stamp-helper]]"]
commits: [<sha1>, <sha2>]
branch: worktree-architect-{slug} | "(committed on main checkout)" | "(no execution this run)"
gates: "fmt ok / clippy 0 / tests N pass (DB up: yes|no) / eval <profile> vs baseline: pass"
---

# Architect scan — {theme or area} ({date})

## Sub-agent reports
{1-2 sentence summary per angle}

## Findings

### [1] {title}  ➤ queued (ADR [[date-slug]])
**Type:** weak-pattern
**Reach / Risk / Effort / Payoff:** ...
**Verdict:** queued — {1 sentence reason}

### [2] {title}  ✅ executed → {commit shas}
...

## Strong patterns observed
- {pattern title} → noted in strong-patterns.md (or codified via {vehicle})

## Cross-references
- Related ADRs: [[...]]
- Related preferences: [[Patterns/architect-preferences]]
```

---

## Phase 12: Final summary

```
Architect run complete.

  Mode:           {scan | area | resume}
  Theme/area:     {theme or area name}
  Contexts:       {names}
  Sub-agents:     {N} angles
  Findings:       {weak} weak / {strong} strong / {swap} swap / {sb} struct-bug

  Triage outcome:
    Executed:     {K} → ADR [[...]], commits {shas}, branch {branch}
    Queued:       {Q} (in backlog)
    Dropped:      {D}
    Reworked:     {R}

  Gates:          fmt {ok} · clippy {0 warnings} · tests {N} (DB up: {yes/no}) · eval {profile}: {pass}
                  console tsc/vitest/build: {ok | n/a}

  Strong patterns:
    Identified:   {N} new this run
    Codified:     {C} → vehicles {[lint|ci-gate|docs-claude|docs-arch|test-guard]}, commits {shas}
    Noted:        {M}
    Aging surfaced: {A}   Aging actioned: {codified: K, snoozed: S, dropped: D}

  Files updated:
    + $VAULT/Architect/scans/{date}-{slug}.md
    + $VAULT/Lessons/{date}-architect.md
    + $VAULT/Architect/decisions/{date}-{slug}.md  (× {N})
    ~ $VAULT/Architect/backlog.md
    ~ $VAULT/Architect/weak-patterns.md  /  strong-patterns.md  /  coverage.md
    {if promoted:}  ~ $VAULT/Patterns/architect-preferences.md
    {if docs updated:} ~ .claude/CLAUDE.md | docs/ARCHITECTURE.md
    {if ownership changed:} ~ context-map.json

  Next?
    1. /architect resume     — execute next decision ({Q} pending)   ← default if Q > 0
    2. /architect scan       — fill the queue with a new theme
    3. /explorer             — daily wandering on an adjacent context
    4. /tiger scan           — the LLM call-site lens
    5. done
    {if a worktree was created: note "merge worktree-architect-{slug}, then
     git worktree remove .claude/worktrees/architect-{slug} and delete the branch"}
```

---

## Notes on use

- **Cadence** — once a week is plenty. Architect runs are heavy; the backlog
  absorbs the inventory and resume mode amortizes the work.
- **Scan vs resume** — alternate. A backlog of 20 pending items means the next
  session should be resume, not scan.
- **Coexist with uncommitted work.** Never require a clean baseline on a shared
  checkout: inspect what's there, edit only your own paths, stage only those
  paths, verify the staged index before every commit. Never `git stash`, never
  `git reset --hard`, never `git checkout --` someone else's work. Physical
  isolation comes from the worktree default (7a), not from cleaning the tree.
- **Conflict signal** — a finding that contradicts a vault `strong-pattern` is the
  most interesting finding of the run. Either the entry is stale or the finding is
  wrong; either way the model changes meaningfully.
- **Drift signal** — 3 consecutive scans producing backlog items with zero
  executed means architect is being used as a brainstorming tool. Surface it.
- **The riskiest decisions here are not tech swaps — they are RLS, migrations, and
  eval baselines.** Anything touching those starts at risk 4, needs a regression
  checklist line of its own, and is never bundled with unrelated steps.

## App context coverage (Personas-managed repos)

This skill declares `contexts: tracked` — the Personas app measures per-context
memory coverage for it. When run inside a Personas-managed repo (a `.personas/`
dir exists, or the app dispatched this run), append JSON lines to
`.personas/memory-outbox.jsonl` at the repo root (append, never rewrite) — one
node per context you meaningfully worked on:

```json
{"type":"node","kind":"progress","title":"<=200 chars: what you did in this context","body":"optional detail","context":"<exact context name from context-map.json>","skill":"architect"}
```

Always set both `"skill":"architect"` and `"context":"<name>"` (must match a
`contexts[].name` in `context-map.json`). The app ingests and deletes the file
when the session ends. Skip silently when the repo is not Personas-managed.
