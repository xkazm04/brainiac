---
name: explorer
description: Wander one context of the Brainiac codebase, surface 10 items worth fixing, triage with the user, then execute the accepted ones in-session. Daily low-friction quality sweeps with per-context coverage memory, anchored on context-map.json.
category: Maintenance
memory: vault
contexts: tracked
---
# Explorer (Brainiac)

Wander one logical context of the Brainiac codebase, surface exactly **10 items**
worth fixing, let the user triage, then execute the accepted ones in-session.
Designed for frequent / low-friction use — daily wandering — and pairs with
`/architect` (heavy structural change), `/tiger` (the LLM call sites), and
`/code-review` + `/simplify` (per-diff passes).

**Anchor:** `context-map.json` at the repo root — 25 contexts, 258 files,
`"groups": []` (the map is flat here), taxonomy `category ∈ {ui, api, lib, data,
test, config}`. Plus `.claude/CLAUDE.md` for the rules a fix must respect.

## Interaction conventions

- **Every prompt is a numbered menu.** Numeric input picks the option; **Enter**
  triggers the default; option `1. other → …` is the deviation lane (free text).
- **Every phase output ends with a `Next?` block** of 2–5 numbered next steps.
- Long free-text answers are still accepted everywhere.

## Input

Ask **two** numbered-menu questions, in this order.

### Q1 — Area

Because `context-map.json` has no groups, the menu is the **crate / surface**
axis; each option resolves to a set of contexts (and their `file_paths`):

```
Area? (Enter = pick for me)
  1. other → type a hint (path fragment, keyword, or exact context name)
  2. core         — crates/brainiac-core        (shared-core, library-core)
  3. store        — crates/brainiac-store       (api-keys, org-directory, projects, document-wiki)
  4. pipeline     — crates/brainiac-pipeline    (extraction-pipeline, sweep-pipeline, standards-governance)
  5. gateway      — crates/brainiac-gateway     (llm-gateway)
  6. server       — crates/brainiac-server      (server-core, auth-middleware, onboarding)
  7. eval         — crates/brainiac-eval, fixtures/  (eval-harness)
  8. publish      — crates/brainiac-publish     (kb-publishing)
  9. console      — console/                    (console-shell, analytics-dashboard, knowledge-health,
                                                 library-landing, practice-divergence-ui, demo-marketing,
                                                 graph-visualization, skills-catalog)
  10. data+deploy — migrations/, Dockerfile, docker-compose*, .env.deploy.example
                                                (database-schema, deploy-config)
  11. pick for me   ← default
```

A single exact context name (e.g. `llm-gateway`, `eval-harness`,
`practice-divergence-ui`) is always accepted via option 1 and is the **preferred**
scope — one context is a tighter feedback loop than a whole crate.

### Q2 — Category

```
Category? (Enter = any)
  1. other → describe (free-form intent; layered onto an auto-picked category)
  2. any            ← default
  3. quality
  4. dx
  5. ui
  6. perf
  7. bug
  8. rls        (tenancy/scoping correctness — Brainiac's load-bearing invariant)
  9. a11y       (console only)
  10. sec
```

Wait for both answers. If the user replies "go"/"wander" or types `/explorer`
with no arguments, treat as "pick for me" + "any".

---

## Constants

- **Codebase reference files** (always loaded):
  - `context-map.json` — the context taxonomy: `name`, `category`, `description`,
    `entry_points`, `file_paths`, `keywords`, `api_surface`, `db_tables`,
    `cross_refs`, `tech_stack`.
  - `.claude/CLAUDE.md` — project rules: crate layering, `anyhow` + `.context`,
    `clippy::unwrap_used`, utoipa→OpenAPI→console types, RLS, append-only
    migrations, KB-as-projection, the 5 quality gates.
  - `docs/ARCHITECTURE.md`, `docs/EVAL.md` — when the area needs deeper background.
- **Vault root** (resolved at Phase 0), then `$VAULT/Explorer/`:
  - `Explorer/sweeps/` — one note per run, the canonical artifact
  - `Explorer/state.md` — informational claim board (what's being explored now)
  - `Explorer/coverage.md` — heatmap of last visit per context + yield density
  - `Explorer/passes.md` — per-context "already considered and rejected" memory
  - `Patterns/explorer-preferences.md` — distilled rules across runs
  - `Lessons/{date}-explorer.md` — append-only self-reflection
- **Categories** — `quality | dx | ui | perf | bug | rls | a11y | sec`
- **Severities** — `critical | high | medium | low`
- **Effort buckets** — `xs (<15m) | s (15-60m) | m (1-3h) | l (>3h)`
- **The gates** (what "validated" means here):
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace` — **needs `docker compose up -d` and
    `DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac`; without
    it the pg-backed tests self-skip and the data plane is NOT verified**
  - Console (from `console/`): `npx tsc --noEmit`, `npx vitest run`, `npm run build`
  - Eval, when the touched code is covered:
    `cargo run -p brainiac-server -- eval --fixtures fixtures/v1 --profile <p>
    --baseline results/<p>-baseline.json --out results/ci-run.json`
    (profiles: `retrieval | extraction | contradiction | resolution | pipeline |
    docs | drift | divergence`). **Never `--write-baseline` from an explorer run.**

---

## Phase 0: Resolve vault path

Same resolution the repo's `/perfect` skill uses, so all skills share one vault:

```bash
for v in "C:/Users/mkdol/Documents/Obsidian/brainiac" "C:/Users/kazda/Documents/Obsidian/brainiac"; do
  [ -d "$v" ] && VAULT="$v" && break
done
# First run: if an Obsidian root exists (C:/Users/*/Documents/Obsidian) but no
# brainiac vault, CREATE the folder there.
# Portable fallback: no Obsidian root at all → use <repo>/.explorer/ (gitignore it).
```

### Bootstrap (one-time per vault)

Create if missing:

- `$VAULT/Explorer/`, `$VAULT/Explorer/sweeps/`
- `$VAULT/Explorer/state.md`:
  ```markdown
  # Explorer State

  Active claims by `/explorer` runs. Informational only — not a hard lock.
  Stale entries (>2h) are released automatically by the next run.

  ## Active

  _No active explorers._
  ```
- `$VAULT/Explorer/coverage.md`:
  ```markdown
  # Explorer Coverage

  Heatmap of contexts explored. Used by Phase 2 to pick the stalest, highest-yield one.

  ## Areas
  ```
- `$VAULT/Explorer/passes.md`:
  ```markdown
  # Explorer Passes

  Per-context record of items surfaced and **rejected** in past runs.
  Future passes over the same context skip these.

  ## Areas
  ```
- `$VAULT/Patterns/explorer-preferences.md`:
  ```markdown
  # Explorer Preferences (distilled from /explorer runs)

  > Rules upgraded from `Lessons/` after 3+ observations. Loaded by Phase 1.

  _No patterns yet. Will be populated as runs accumulate._
  ```

Don't create `Lessons/` (shared with the other skills).

---

## Phase 1: Load context & memory

### 1a. Required-file check
`context-map.json` and `.claude/CLAUDE.md` must exist. If `context-map.json` is
missing, stop and tell the user to re-run the context scan — don't improvise a
taxonomy.

### 1b. Read in order
1. `context-map.json` — the taxonomy: names, categories, `entry_points`,
   `file_paths`, `keywords`, `cross_refs`.
2. `.claude/CLAUDE.md` — layering, RLS, error style, type-sync, the gates.
3. `$VAULT/Architect/strong-patterns.md` (if present) — canonical shapes this
   codebase does well. When proposing a fix in Phase 5, **prefer an existing
   strong pattern's shape** over inventing one; record it in `strong_pattern_ref`.
4. `$VAULT/Patterns/explorer-preferences.md` — finding shapes the user rejects.
5. `$VAULT/Explorer/state.md` — what other explorers are on right now.
6. `$VAULT/Explorer/coverage.md` — last-visit dates and yield per context.
7. `$VAULT/Explorer/passes.md` — items already rejected per context.
8. The 3 most recent `$VAULT/Lessons/*-explorer.md`.

### 1c. Stale-claim sweep
In `state.md`, drop any entry whose `claimed_at` is older than 2 hours.

### 1d. Snapshot freshness
Parse `generated_at` in `context-map.json`. If >30 days old, or the repo has
advanced a lot since (`git rev-list --count HEAD`), warn but continue:
```
Warning: context-map.json may be stale ({D} days old). Consider a context rescan.
```

---

## Phase 2: Pick area

### 2a. If the user gave a hint
Resolve against `context-map.json`:
- Exact `name` (e.g. `llm-gateway`) → that context.
- Crate/dir fragment (e.g. `brainiac-pipeline`, `console/src/library`) → contexts
  whose `file_paths` overlap.
- Keyword (e.g. `sweeps`, `rls`, `pgvector`) → contexts whose `keywords` match.

If >3 plausible contexts, present a short numbered list and ask "which one?".

Two contexts currently have **empty `file_paths`** (`graph-visualization`,
`skills-catalog`). If one is picked, say so and either widen to a `cross_refs`
neighbor or ask the user for the real paths — do not fabricate a scan.

### 2b. If the user said "pick for me"
Score each context by:
- **Staleness** — days since last visit per `coverage.md`. Never-visited = max.
- **Past yield density** — accepted / surfaced in the last 1–2 visits (tie-breaker).
- **Active claim penalty** — appears in `state.md` Active → score 0, skip.

Pick the top-scored context; ties broken by smaller `file_paths` count. Tell the
user which and why in one sentence, then:

```
Next?
  1. other → name a different context
  2. proceed with {picked-context}   ← default
  3. abort
```

### 2c. Category filter
If not `any`, narrow the scan focus. The area stays the same; the filter changes
what counts toward the 10-item budget.

---

## Phase 3: Claim the area

Append to `$VAULT/Explorer/state.md` under `## Active`:

```markdown
- **{context-name}** — claimed_at: {ISO timestamp}, run_id: {short id}, category: {filter}
```

Informational, not a lock. Also append one entry to `.claude/active-runs.md`
under `## Active` (create the file with `## Active` / `## Recently completed` if
it doesn't exist yet) declaring the context's paths as your scope. If an existing
`started` entry <2h old overlaps those paths, surface the conflict before
scanning. Print the claim line so the user knows what's recorded.

---

## Phase 4: Wander the code

Read enough to identify 10 items. Budget tool calls — sample strategically.

### 4a. Sampling strategy
For a context with N `file_paths`:
- N ≤ 5: read all.
- 5 < N ≤ 20: read all `entry_points` + a sampling of the rest, capped at 10 reads.
- N > 20: read `entry_points` + the largest files (glob, sort by line count) +
  sample 5–8 of those.

Use `Read` with offset/limit on files >500 lines (top + a middle slice + bottom).
For Rust, the module doc-comment at the top of each file is unusually rich in this
repo — read it first; it often states the invariant the file is supposed to hold.

### 4b. What to look for, by category

**Hard exclusions — never surface these as items:**
- Anything `cargo fmt` or `cargo clippy -- -D warnings` already catches. CI runs
  both at zero-tolerance, so "add `#[must_use]`", "this could be `&str`",
  formatting nits, and mechanical clippy suggestions are not findings — they'd
  already be failing the build.
- Eval **baseline** deltas. A baseline is a deliberate committed artifact
  (`docs/EVAL.md` §3.2); "the score moved" is not an explorer item.
- Generated files: `console/src/lib/api-schema.d.ts` (from `npm run gen:api`) and
  `openapi.json` — fix the `utoipa` annotation upstream instead, or skip.
- Shipped migrations (`migrations/NNNN_*.sql`) — append-only; a "fix" there is an
  `/architect` decision, never a paper cut.

For `quality`:
- Dead code, unreachable branches, unused `pub` exports across crate boundaries.
- Duplicated logic (3+ near-identical blocks), especially the same SQL shape or
  the same JSON-recovery dance repeated per module.
- Misleading names; a doc-comment that no longer matches the code it heads.
- Comments explaining "what" instead of "why".
- Commented-out code older than the current branch.

For `dx`:
- Errors bubbling without `.context(...)` — the repo's convention is
  `anyhow::Result` with context at every boundary.
- Test setup boilerplate that should be a fixture (see `crates/brainiac-fixtures`).
- A pg-backed test that self-skips silently and therefore proves nothing in CI.
- Repeated env-var parsing that should reuse a helper (e.g.
  `crates/brainiac-gateway/src/providers/mod.rs`'s `first_env` / `secs`).
- CLI/subcommand ergonomics in `crates/brainiac-server/src/main.rs`.

For `ui` (console):
- Hand-rolled duplicates of shared components in `console/src/components/`.
- Missing loading / empty / error states on a user-facing surface.
- Visual bugs (overflow, alignment, contrast) — only if reproducible from code.
- Inconsistent Tailwind spacing/radius vs the rest of the module.
- Data access that should sit behind `server-only` leaking into a client component.
- Hand-written request/response types instead of the generated OpenAPI schema.

For `perf`:
- N+1 queries; a query inside a loop that could be one `IN (...)`.
- Per-row `embed()` calls where `embed_batch` exists.
- Missing index for a hot filter/order path (cross-check `migrations/`).
- Unbounded prompt/context growth (whole memory bodies where a digest would do).
- Sync/blocking work inside an async worker tick.
- Vector work in `store` that belongs in `core` (and vice versa) causing repeated
  conversions.
- Console: unnecessary re-renders, unmemoized expensive children, unvirtualized
  long lists.

For `bug`:
- Races: read-then-write outside a transaction; a worker tick without idempotency.
- Unhandled edge cases (empty slice, `None`, NaN, zero-length embedding).
- Errors swallowed (`let _ =`, `if let Ok(..)` with no else, empty `catch`) —
  note that `lighttrack.rs` swallows deliberately by design; that one is correct.
- Off-by-one / boundary errors in chunking, batching, or top-k slicing.
- `.unwrap()` / `.expect()` in non-test code (the workspace lints it as `warn`).

For `rls` (Brainiac's load-bearing invariant):
- Any `sqlx` call that doesn't run inside the caller-scoped transaction
  (`scoped_tx`, `SET LOCAL app.org_id` / `app.user_id`).
- A helper that takes a raw `&PgPool` where it should take a scoped connection.
- DDL/admin-role work happening on the runtime `brainiac_app` path.
- A read widened "temporarily" to unblock something.
- Auto-promote to severity `high` at minimum; `critical` if a cross-org read is
  actually reachable.

For `a11y` (console):
- Missing labels on form inputs; icon-only buttons without an accessible name.
- Clickable `div`s without role/tabIndex; broken keyboard nav; missing focus styles.
- Modals without focus trap / escape handler.

For `sec`:
- Externally reachable surfaces (HTTP routes in `crates/brainiac-server`, MCP
  tools, console route handlers) without auth/validation.
- User input interpolated into SQL or a shell string.
- Credentials or org content logged, or surfaced in an error message that reaches
  a client (remember: `tracing::error!` becomes a Sentry event, and there is no
  `before_send` scrubber — see `/sentry`).
- Token handling in `crates/brainiac-store/src/tokens.rs` / `auth.rs`.
- Auto-promote sec findings to severity `critical`.

### 4c. Honor the deprioritization signals
- Skip finding shapes listed in `Patterns/explorer-preferences.md`.
- Skip exact matches in `Explorer/passes.md` for this context. A near-match may be
  surfaced with "previously passed; resurfacing because <reason>".
- Cross-check prior sweeps (`Explorer/sweeps/*-{context}.md`) — don't resurface an
  item a past run already raised unless its status changed.

### 4d. Dedupe against recent history (one command)

```bash
git log --oneline -20 -- <the context's file_paths>
```

Drop any candidate plausibly fixed by a recent commit (verify by reading the
current code, not the commit message). If a candidate survives despite recent
activity, note "still present after <sha>" in its evidence.

### 4e. Stop conditions
- 10 items → stop, go to Phase 5.
- Exhausted the context without 10 → widen to a `cross_refs` neighbor from
  `context-map.json` (note the widening). If still <10 after widening twice, stop
  and explain the shortfall.
- Tool budget exceeded (>40 file reads) → stop with what you have.

**Do not pad the list.** Quality over quota.

---

## Phase 5: Categorize and structure each item

### Premise verification (hard gate — no item ships without it)

Every item's `anchor` must be a `file:line` **you actually Read this session**,
and its `evidence` must quote or paraphrase the real code there. Re-Read the
anchor lines of any item whose premise came from a grep hit, and confirm the
defect is really there (the guard isn't in the caller, the "dead" export isn't
used by another crate — `rg "\bname\b" crates/ console/` settles it; the "missing"
scope stamp isn't applied by the wrapper). Pattern-matched suspicion is not an
item. If verification kills a candidate, replace it or run short.

For each item capture:

```yaml
- id: 1
  title: "<short imperative phrase, ≤60 chars>"
  category: quality | dx | ui | perf | bug | rls | a11y | sec
  severity: critical | high | medium | low
  effort: xs | s | m | l
  anchor: "<file_path>:<line_number>"
  evidence: "<2-3 sentences, with a verbatim snippet if helpful>"
  suggested_fix: "<1-2 sentence shape of the fix — not the fix itself>"
  strong_pattern_ref: "<wikilink to Architect/strong-patterns#... entry>" | null
  layer_check: "<which crate/layer the fix belongs in, per .claude/CLAUDE.md>"
  gate_impact: "<none | needs DATABASE_URL | touches openapi.json (regen + gen:api) |
                 covered by eval profile <p> | console build>"
  cluster_hint: "<other ids that ship naturally with this one, or 'standalone'>"
```

**On `layer_check`:** if the honest fix belongs in a different crate than the
anchor (SQL leaking into `core`, scoring math sitting in `store`, orchestration in
`server`), say so — an altitude fix is usually the better item, and if it's too
big for a paper cut it belongs to `/architect`.

### Severity rubric (be honest)
- **critical** — cross-org data exposure, data loss risk, panic on a common path.
- **high** — wrong behavior on the golden path, broken common edge case,
  regression risk if left.
- **medium** — paper cut, confusing code, small perf hit, latent risk.
- **low** — polish, taste-level.

If most items are "high," recalibrate. A 10-item list typically lands 0–1
critical, 2–3 high, 4–6 medium, 1–3 low.

### Cluster detection
Same file → same commit. Type/function dependency → ship in order. Same migration
or same OpenAPI regeneration → one commit. Note in `cluster_hint`.

---

## Phase 6: Present to user

### Summary table

```
#   Cat     Sev    Effort  Title                                              Anchor
─   ─────   ────   ──────  ─────────────────────────────────────────────────  ──────────────────────────
1   {cat}   {sev}  {eff}   {imperative phrase, ≤60 chars}                     {crates/…|console/src/…}:{line}
2   ...
```

(Shape only — every row must be a real anchor you Read this session, per the
premise gate above. Never ship a placeholder row.)

### Per-item detail

```
[N] {title}
    Category / Severity / Effort:  {cat} / {sev} / {effort}
    Anchor:    {file:line}
    Evidence:  {explanation + snippet}
    Suggested: {1-2 sentence fix shape}
    Follows:   {strong-pattern wikilink + canonical example, or "—"}
    Layer:     {crate the fix belongs in — and whether that differs from the anchor}
    Gates:     {none | DATABASE_URL | openapi regen | eval <profile> | console build}
    Cluster:   {standalone | ships with [a, b]}
```

If items are clustered, end with a short `Clusters:` block naming the order.

---

## Phase 7: Triage

```
Which to action? Reply with item numbers (e.g. "1, 3, 4").

Shortcuts:
  all     — accept every surfaced item
  none    — accept nothing (still write the sweep note)
  ask     — guided walkthrough item-by-item
  Enter   — same as "none"   ← default
```

Execute each accepted item **in this same session**: discover → decide →
implement → commit, all in one context window.

### Execution rules

**Single accepted item with a clear anchor (Option A):**
1. Apply the edit at `anchor`.
2. Validate — run the smallest sufficient subset, then the gate that matters:
   - Rust → `cargo fmt --all --check` and
     `cargo clippy --workspace --all-targets -- -D warnings`, plus
     `cargo test -p brainiac-<crate>` (and `cargo test --workspace` before the
     final commit of the run). For anything touching `store`/`pipeline`/`eval`:
     `docker compose up -d` and export
     `DATABASE_URL=postgres://brainiac:brainiac@localhost:5433/brainiac` first —
     otherwise the pg tests self-skip and you have verified nothing.
   - Console → from `console/`: `npx tsc --noEmit` && `npx vitest run`
     (+ `npm run build` if the change touches build config or a server boundary).
     The honesty guard `console/src/kb/kb-data.test.ts` must stay green.
   - Types crossed the API boundary → regenerate `openapi.json`, then
     `npm run gen:api` in `console/`, and commit both.
   - Covered by an eval profile → run it against its committed baseline. Never
     `--write-baseline`.
3. **Stage scoped + verify in ONE Bash invocation** (concurrent sessions rewrite
   the index between separate calls):
   ```bash
   git add path/one path/two && git diff --cached --stat
   ```
   Never `git add -A`, `git add .`, or `git add -u`. If the cached stat lists more
   files than you added, another session pre-staged work — `git restore --staged
   <path>` each unrelated file, re-verify, THEN commit.
4. Commit atomically with the repo's conventional style
   (`fix(pipeline): …` / `perf(store): …` / `refactor(gateway): …` — see
   `git log --oneline -10`) + Co-Authored-By footer + a body explaining the why.

**2+ accepted items (Option B):**
1. Print the inline plan (one paragraph per item: file, change shape, validation).
2. Execute in **risk-ascending order** (xs effort first, l last; ties broken by
   category — `rls`/`sec` and `bug` before `perf` before `quality`).
3. Atomic commit per item, validation per commit, same one-invocation
   stage-verify-commit discipline.
4. Validation fails → fix inline. Do NOT stack failing commits. No `--no-verify`,
   no `--amend`.
5. If a downstream item becomes redundant after an upstream commit, drop it and
   note the drop in the run record.

**Item that needs more thought (Option D — escape hatch):**
Record it in the run record as `decided: deferred` with the reason. Do NOT write a
handoff file; the run record is the future search target. If it turns out to be
structural (a boundary, a missing primitive, a convention), hand it to
`/architect` explicitly rather than half-fixing it.

### Non-negotiables for any change
- **RLS**: never widen a read, never bypass `scoped_tx`, never move a runtime
  query onto the admin role.
- **Layering**: domain math stays in `core`, SQL in `store`, orchestration in
  `pipeline`, HTTP/MCP/CLI in `server`. If the fix would violate this, defer it.
- **Migrations** are append-only and numbered; never edit a shipped one.
- **Console types** come from the generated OpenAPI schema; never hand-write them.
- **`context-map.json`** gets updated if the change moves file ownership between
  contexts.
- If you can't honor these, defer the item — don't ship it half-converted.

### Runtime verification
If a change is user-visible (console UI, an HTTP/MCP surface, worker behavior),
either exercise it (`npm run dev` at :3100, `cargo run -p brainiac-server --
serve` at :8600, `-- worker`) or state explicitly that you have NOT verified at
runtime. Don't claim "looks good" from code review alone.

---

## Phase 8: Persist the sweep

`$VAULT/Explorer/sweeps/{YYYY-MM-DD}-{context-name}.md`:

```markdown
---
date: 2026-07-26
run_id: {short id}
area: {context name}
category_of_context: ui | api | lib | data | test | config
files_sampled: {N}
category_filter: any | quality | ...
total_items: 10
accepted: [1, 3, 4]
declined: [2, 5, 6, 7, 8, 9, 10]
deferred: []
commits: [<sha1>, <sha2>]
widened: false
gates: "fmt ok / clippy 0 / tests {N} (DB up: yes|no) / console tsc+vitest: ok|n/a / eval {profile}: pass|n/a"
---

# {Context} sweep — {date}

## Items

### [1] {title}  ✅ accepted → {commit sha} `{commit subject}`
**Category / Severity / Effort:** {cat} / {sev} / {effort}
**Anchor:** `{file:line}`
**Evidence:** {evidence}
**Fix shape:** {what was actually done}

### [2] {title}  ❌ declined
**Decline reason:** _filled in Phase 9_

### [3] {title}  ⏸ deferred
**Reason:** {concrete blocker — or "handed to /architect: <why structural>"}

## Cross-references
- Adjacent contexts not yet swept: {from cross_refs / coverage.md}
- Related preferences: [[Patterns/explorer-preferences]]
```

---

## Phase 9: Self-reflection

### 9a. Ask why for declined items
```
For the declined items, why did you skip them?

  [2] {title}
  [5] {title}

Reply per-item ("2: too vague, 5: already planned") or one overall reason.

Shortcuts:
  skip    — record "no reason given"
  Enter   — same as "skip"   ← default
```

### 9b. Append to Lessons
`$VAULT/Lessons/{YYYY-MM-DD}-explorer.md`:
```markdown
## Run: {timestamp} — {context} ({category filter})

Sampled: {N} files
Surfaced: {M} items
Accepted: [list]
Declined: [list] (with reasons)
Deferred: [list] (with blockers)

### Self-reflection
- Categories that resonated: {pattern}
- Categories that didn't: {pattern}
- Calibration drift: {e.g. "rated 7 items 'high' but user accepted 2"}
- Tools/gates to use more or less next time: {observation}
```

### 9c. Backfill the sweep note with the decline reasons.

### 9d. Update passes.md
```markdown
## {context}

- {file:line} — {1-line summary of the rejected suggestion} — pass {date}, run {id}, reason: {short reason}
```

### 9e. Pattern promotion check
Read all `$VAULT/Lessons/*-explorer.md`. If a decline reason appears in **3+ runs**:
```
I've seen this 3+ times — promote to permanent rule?
  "{distilled rule}"

Source runs: [[...]], [[...]], [[...]]

Next?
  1. promote to Patterns/explorer-preferences.md   ← default
  2. snooze (re-ask after 3 more observations)
  3. drop (don't promote, reset the counter)
```

### 9f. Update coverage.md
```markdown
## Areas

### {context-name}

- Last visited: {date}
- Last run: [[Explorer/sweeps/{date}-{context-name}]]
- Items surfaced (last 3 runs): [10, 8, 10]
- Items accepted (last 3 runs): [3, 5, 4]
- Yield density: {accepted / surfaced average}
- Notes: {anything noteworthy across runs}
```

### 9g. Release the claim
Remove the Phase 3 entry from `$VAULT/Explorer/state.md`, and move the
`.claude/active-runs.md` entry to the top of `## Recently completed` with the
commit SHA(s) (or `aborted (<reason>)`).

### 9h. Durable knowledge (optional)
If the run produced a decision or a pitfall other sessions would benefit from,
write one self-contained statement to Brainiac org memory:
`POST $BRAINIAC_API_URL/v1/memories {"content": "..."}` with the bearer token from
`.env` (read without printing). Facts, not transcripts; skip silently if the repo
has no Brainiac keys.

---

## Phase 10: Final summary

```
Explorer run complete.

  Context:        {name} (category: {ui|api|lib|data|test|config})
  Filter:         {category}
  Files sampled:  {N}
  Items surfaced: {M} / 10
  Accepted:       {K} → {commit shas}
  Declined:       {L}
  Deferred:       {D}

  Gates:          fmt {ok} · clippy {0} · cargo test {N} (DB up: {yes/no})
                  console tsc/vitest {ok|n/a} · eval {profile} {pass|n/a}

  Coverage update: last visit {date} → {today}, yield density {X}/{Y}

  Files updated:
    + $VAULT/Explorer/sweeps/{date}-{context}.md
    + $VAULT/Lessons/{date}-explorer.md
    ~ $VAULT/Explorer/coverage.md
    ~ $VAULT/Explorer/passes.md  (if any declines)
    ~ $VAULT/Explorer/state.md   (claim released)
    {if promoted:} ~ $VAULT/Patterns/explorer-preferences.md

  Next?
    1. /explorer {stalest adjacent context}              ← default
    2. /explorer {same context, different category}
    3. /tiger scan          (the LLM call-site lens)
    4. /architect resume    (drain the structural backlog)
    5. done
```

If zero items were accepted, frame the run as a successful pass over a healthy
context. The point is signal, not action.

---

## Notes on use

- **Pair with `/architect`** — explorer finds the paper cuts; when three of them
  share one root (a missing primitive, a boundary nobody enforces), that's an
  `/architect` decision, not ten explorer items.
- **Pair with `/tiger`** — anything about a model call, its prompt, its grounding,
  or its cost belongs to `/tiger`, not here.
- **Cadence** — daily or every other day. `coverage.md` tells you when the 25
  contexts are uniformly fresh and it's time for `/architect` instead.
- **Coexist with uncommitted work.** Multiple CLIs share this tree. Explorer never
  stashes, resets, or discards anything it didn't author. Stage **only** the paths
  you touched. Forbidden at all times: `git stash`, `git reset --hard`,
  `git restore` / `git checkout --` on working-tree paths you didn't author,
  `git clean -f`, `git add -A|.|-u`. (`git restore --staged <path>` to unstage a
  foreign pre-staged file is allowed — it never touches the working tree.)
- **A green `cargo test --workspace` without Postgres is a lie.** If you didn't
  bring the DB up, say so in the sweep note's `gates:` field.
- **Drift signal** — 3+ runs in a row with 0 accepted items means the calibration
  is off. Read the last 3 sweeps and ask the user "what shape would have actually
  been useful?"

## App context coverage (Personas-managed repos)

This skill declares `contexts: tracked` — the Personas app measures per-context
memory coverage for it. When run inside a Personas-managed repo (a `.personas/`
dir exists, or the app dispatched this run), append JSON lines to
`.personas/memory-outbox.jsonl` at the repo root (append, never rewrite) — one
node per context you meaningfully worked on:

```json
{"type":"node","kind":"progress","title":"<=200 chars: what you did in this context","body":"optional detail","context":"<exact context name from context-map.json>","skill":"explorer"}
```

Always set both `"skill":"explorer"` and `"context":"<name>"` (must match a
`contexts[].name` in `context-map.json`). The app ingests and deletes the file
when the session ends. Skip silently when the repo is not Personas-managed.
