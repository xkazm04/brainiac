---
name: code-review
description: Production-readiness code review for the Brainiac Rust workspace (core/store/pipeline/gateway/eval/publish/server) and the Next.js console. Reads changed files, checks them against Brainiac's real invariants (RLS, crate layering, sqlx, OpenAPI type sync, eval regression gates), and produces a severity-bucketed 5-axis verdict with file+line findings.
allowed-tools: Read, Grep, Glob, Bash, Agent
---

# Code Review — Production Readiness (Brainiac)

You are a senior code reviewer for **Brainiac** — an org-memory service: a Rust
8-crate workspace behind a single axum binary + a Next.js 15 console. Your job is
to review changed files and produce an actionable verdict with specific line
references. Read `.claude/CLAUDE.md` and `context-map.json` first — they carry the
invariants (RLS, crate layering, eval gates) you are grading against.

## Trigger

`/code-review` — reviews all unstaged/staged changes against `HEAD`.
`/code-review <file-or-glob>` — reviews only matching files.
`/code-review --commit <ref>` — reviews files changed in a commit or range.

---

## Coordination & git discipline

Concurrent agents share this checkout on `master` (no per-session branching). This
skill is mostly read-only; if you apply fixes it becomes a writer.

- **Stage only your paths.** `git add <path>` per file — never `git add -A` / `.` /
  `-u`. Another session's in-flight work is visible in `git status`; leave it alone.
- **Never `git stash`** work that isn't yours (it sweeps untracked + other sessions'
  edits into a hidden state). If you need a clean stage, add per-file instead.
- **On `index.lock` contention**, wait 3–10s and retry up to 6 times; don't force-remove
  the lock.
- **Multi-file fixes go in a worktree** under `.claude/worktrees/<slug>` (gitignored),
  committed atomically per fix. Pure read-only review stays on the main checkout.
- After `git add`, before `git commit`, run `git diff --cached --stat`; if more files
  are staged than you added, `git restore --staged <path>` the strangers.

---

## Step 1: Identify changed files

```bash
git diff --name-only HEAD            # default: working-tree changes vs HEAD
git diff --name-only <ref>^..<ref>   # if a commit ref was given
```

Bucket the files:
- **Rust**: `crates/**/*.rs` (note which crate — the layering rules in Step 3D depend on it)
- **Console**: `console/src/**/*.{ts,tsx}`
- **Migrations**: `migrations/*.sql` (Step 5)
- **API contract**: `openapi.json`, `console/src/lib/api-schema.d.ts` (Step 5)

If no changed files match, say so and stop.

## Step 2: Read every changed file

Read each changed file in full — never review from memory or diff snippets alone.
Also read the diff to see exactly what changed:

```bash
git diff HEAD -- <file>
```

## Step 3: Rust review

For every changed `.rs` file, evaluate against ALL of the following. Flag each
violation with file path, line number, and a one-line fix.

### 3A. Security

- [ ] **RLS is load-bearing — never bypass `scoped_tx`.** Every runtime read/write must
  run inside a `scoped_tx(principal)` transaction that `SET LOCAL app.org_id` /
  `app.user_id`, under the non-owner `brainiac_app` role. Flag any new query that runs
  on a raw pool/admin connection, any read that widens across org/user, or any code
  path in `store` that touches data without a verified principal. This is the #1 review
  priority — a leak here is a hard eval gate (`RLS leaks = 0`) and a security bug.
- [ ] **No `.unwrap()` / `.expect()` on fallible/user data** — the workspace lints
  `clippy::unwrap_used = "warn"`. External data (HTTP bodies, DB rows, provider
  responses, env, JSON) uses `?` with `.context(...)`, `.ok_or_else(...)`, or explicit
  match. `unwrap`/`expect` only on compile-time-infallible values, and never in
  non-test code without a `// safe: <reason>`.
- [ ] **SQL is parameterized** — sqlx bind params (`$1`, `query!`/`query_as!` macros).
  No `format!`/string-concat into SQL. Prefer the compile-checked macros where possible.
- [ ] **Auth before logic** — every axum handler that reads org data extracts and
  verifies the principal (bearer → org_id/user_id) before touching the store. New
  routes must not be reachable unauthenticated unless deliberately public (health).
- [ ] **Secret handling** — `BRAINIAC_API_TOKEN` / provider keys / DB URLs are never
  logged, never put in error bodies, never serialized to the console. API keys are
  stored hashed. Check new `tracing` fields and `Serialize` structs for leaks.
- [ ] **Error leakage** — anyhow `.context()` chains and provider errors must not reach
  the HTTP response verbatim (internal paths, SQL, secrets). Map to a sanitized status
  + safe message at the axum boundary.
- [ ] **Command / path injection** — no `std::process::Command` or path built by
  `format!` from unvalidated input.

### 3B. Error handling

- [ ] **`anyhow::Result` + `.context(...)`** across crates; errors carry enough context
  to diagnose (which memory, which org, which stage). No silent swallowing
  (`let _ = fallible()`, empty `if let Err(_)`); if intentional, require a
  `// deliberate: <reason>`.
- [ ] **Transaction hygiene** — `scoped_tx` work commits or rolls back on every path;
  no early `?` that leaks an open transaction (RAII/`?`-through-a-tx is fine, a manual
  begin without commit is not).
- [ ] **No `.unwrap()` on locks / `Option` / `Result`** outside tests (see 3A).

### 3C. Performance

- [ ] **No N+1 queries** — batch reads in `store` (`WHERE id = ANY($1)`), don't loop
  single-row selects. Retrieval fan-out especially.
- [ ] **No redundant embedding / scoring / rerank** — `core` math is pure; don't recompute
  an embedding or fusion score already available. Cache/reuse within a request.
- [ ] **Clone cost** — no `.clone()` on large rows/vectors where a reference or `Arc`
  works. Watch embedding `Vec<f32>` clones on hot paths.
- [ ] **Bounded collections** — anything populated from a query or provider response has a
  cap / pagination (retrieval `k`, sweep batch sizes).
- [ ] **No blocking in async** — no sync blocking calls or long CPU loops on the axum/tokio
  runtime without `spawn_blocking`; pool connections held only for the critical section.

### 3D. Correctness & crate architecture

- [ ] **Crate layering is respected** (`crates/CLAUDE.md` conventions):
  - `core` = pure domain math (embed, fusion, rerank, scoring, temporal, health,
    `types`) with **no DB**. Flag any SQL / sqlx / `store` import that leaked in.
  - `store` = Postgres data plane, one module per aggregate; maps rows ↔ `core` types
    and enforces RLS. Flag business/scoring math that belongs in `core`.
  - `pipeline` = async workers (extract → contradict → resolve → compose, faithfulness,
    sweeps). `gateway` = BYOM providers + resilience. `server` = axum + MCP + clap CLI.
  Flag any change that puts logic in the wrong crate.
- [ ] **API types cross via utoipa/OpenAPI** — new request/response structs derive
  `ToSchema` (+ `Serialize`/`Deserialize`) and are wired into the OpenAPI doc. See Step 5
  for the regen requirement.
- [ ] **Eval-sensitive changes** — any edit to retrieval / fusion / rerank / scoring /
  temporal / contradiction / resolution must be checked against the eval regression
  gates (`eval --baseline results/baseline.json`). A baseline shift is a deliberate,
  committed `--write-baseline` diff, never an accident. Flag eval-affecting changes that
  don't mention the gate.
- [ ] **Tracing** — new handlers/workers carry `tracing` spans with structured fields
  (org, stage), no secrets.
- [ ] **Idiomatic Rust** — `if let` over one-arm `match` + wildcard; `?` over
  match-return-err; no trailing `return`; no unused imports / dead code
  (`#[allow(dead_code)]` needs justification).
- [ ] **CLI wiring** — new `brainiac-server` subcommands are registered in the clap enum
  in `main.rs`.

## Step 4: Console review (Next.js 15 · React 19 · Tailwind 4)

For every changed `.ts`/`.tsx` under `console/src`, evaluate:

### 4A. Server/client boundary & data honesty

- [ ] **Server-only data access stays server-only** — data fetching that uses secrets or
  privileged endpoints imports `server-only`; never ship a token or admin URL into a
  `"use client"` bundle. Flag `"use client"` on components that read secrets.
- [ ] **The KB is a projection, not a source of truth** — no UI that fabricates or mocks
  data and presents it as real memory/KB content. The honesty-guard test
  (`kb-data.test.ts`) must stay green; flag anything that would break it.
- [ ] **Consume generated OpenAPI types** — request/response shapes come from
  `src/lib/api-schema.d.ts` (regenerated by `npm run gen:api`). Flag hand-written
  interfaces that duplicate a schema type.

### 4B. Bug prevention

- [ ] **Effect deps** — every `useEffect`/`useMemo`/`useCallback` has correct deps (no
  stale closures, no infinite-loop deps).
- [ ] **Null safety** — optional chaining on API responses; no bare access on possibly
  `undefined` data.
- [ ] **Keys** — every `.map()` render uses a stable unique `key` (not array index for
  reorderable lists).
- [ ] **Race conditions** — async effects handle unmount (abort controller / stale flag);
  stale responses discarded.
- [ ] **Type safety** — no `any`; no `as` cast that can fail at runtime; prefer type
  guards / discriminated unions.

### 4C. Structure & performance

- [ ] **Component focus** — a component that fetches, transforms, and renders should split
  (container vs presenter). Flag files that have grown past ~250 lines with concrete
  extraction points (sub-component, hook, `lib/` helper).
- [ ] **Memoization** — `useMemo`/`useCallback` on expensive transforms and list work
  (> ~20 items); selectors return the minimum slice.
- [ ] **Lazy-load heavy libs** — `recharts` / `mermaid` and other large deps load behind
  dynamic import / route boundaries, not in the shared shell.
- [ ] **Tailwind 4** — no inline `style={{}}` where a utility exists; no conflicting
  utilities.

## Step 5: Cross-cutting

- [ ] **OpenAPI ↔ TS type sync** — if a `utoipa` schema or route changed, `openapi.json`
  must be regenerated and the console types refreshed (`npm run gen:api`). Flag a Rust
  API-shape change with no matching `openapi.json` / `api-schema.d.ts` update.
- [ ] **Route / subcommand registration** — new axum routes are mounted in the router; new
  CLI roles registered in `main.rs`.
- [ ] **Migration safety** — migrations are append-only and numbered
  (`migrations/NNNN_*.sql`, applied by `sqlx::migrate!`). A new migration takes the next
  number; a **shipped** migration is never edited (flag as HIGH RISK). New columns are
  nullable or defaulted; DROP/column-removal flagged HIGH RISK. New tables carry an RLS
  policy consistent with the `brainiac_app` role.

## Step 6: 5-Axis review

Score the change across five independent axes. Each gets a verdict
(`PASS` / `FLAG` / `FAIL`) and its own findings, so no category is skipped even
when one dominates.

### Axis 1 — Correctness
Logic errors, wrong branches, missing edge cases (empty/null/boundary), serialization
mismatches (Rust ↔ OpenAPI ↔ TS), race conditions, stale closures.

### Axis 2 — Readability & simplicity
Function/component size, naming clarity, unnecessary complexity (nested match/ternary,
deep callbacks), import/module boundary clarity. *(Deep reuse/altitude refactoring is
`/simplify`'s job — here, just flag; don't rewrite.)*

### Axis 3 — Architecture conformance
Crate layering (no SQL in `core`, no math in `store`), correct HTTP→store→`scoped_tx`
path, worker vs handler placement, migration append-only, OpenAPI type flow, console
server/client boundary.

### Axis 4 — Security
`scoped_tx`/RLS bypass, unscoped or cross-org reads, `unwrap` on fallible data, SQL
injection, secret leakage in logs/errors/responses, missing auth on new routes, token
leakage into client bundles.

### Axis 5 — Performance
N+1 queries, redundant embedding/score compute, unbounded collections, avoidable clones,
blocking-in-async, missing memoization, heavy console bundle imports.

## Step 7: Produce the review

Output exactly this shape:

```
## Code Review: <scope description>

### Verdict: APPROVE | APPROVE WITH NOTES | REQUEST CHANGES

### 5-Axis Scorecard
| Axis              | Verdict | Findings |
|-------------------|---------|----------|
| 1. Correctness    | PASS    | 0        |
| 2. Readability    | FLAG    | 2        |
| 3. Architecture   | PASS    | 0        |
| 4. Security       | FAIL    | 1        |
| 5. Performance    | FLAG    | 1        |

### Critical (must fix before merge)
- [Security] `crates/brainiac-store/src/retrieval.rs:42` — query runs on the raw pool,
  bypassing scoped_tx; wrap in `scoped_tx(principal)` so RLS applies.

### Warnings (should fix)
- [Performance] `crates/brainiac-store/src/memories.rs:88` — N+1: loops single selects
  inside a fan-out; batch with `WHERE id = ANY($1)`.

### Nits (optional)
- [Readability] `console/src/kb/KbTable.tsx:12` — unused import `useState`.

### Verification
- Ran / recommend: cargo fmt --all --check · cargo clippy --workspace --all-targets
  -D warnings · cargo test --workspace · eval regression gates (if eval-sensitive) ·
  console: npx tsc --noEmit && npx vitest run && npm run build
- Files reviewed: N · Issues: X critical, Y warnings, Z nits · Worst axis: Security (FAIL)
```

Rules:
- Every finding has a file path + line number and its axis tag (`[Security]`, …).
- Every finding has a concrete fix, not "fix this".
- Group by severity, then file. If an axis is clean, mark PASS — don't invent problems.
- Don't suggest adding comments/docstrings to unchanged code, and don't propose refactors
  beyond what a finding requires — deep quality refactoring is `/simplify`'s lane.
- Be direct and specific.

---

**Adapted for brainiac:** method (diff-scoped read → Rust + frontend checklists →
cross-cutting → 5-axis, severity-bucketed, file+line findings) preserved from the Personas
`code-review` skill. Retargeted from Tauri/rusqlite/ts-rs/Zustand to Brainiac's Rust
8-crate workspace (RLS `scoped_tx`, sqlx, crate layering, utoipa/OpenAPI, eval regression
gates) and Next.js 15 console (server-only boundary, generated OpenAPI types, KB honesty
guard); verification ladder swapped to Brainiac's actual CI gates.
