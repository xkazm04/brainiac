---
name: simplify
description: Review the changed Brainiac code for reuse, simplification, efficiency, and altitude (right-layer/right-crate) cleanups, then apply the fixes. Quality only — it does not hunt for bugs; use /code-review for that. Preserves behavior, respects RLS and crate layering, and re-runs the gates on what it touched.
argument-hint: "[<file-or-glob> | --commit <ref>]"
allowed-tools: Read, Grep, Glob, Bash, Edit, Agent
---

# Simplify — quality pass (Brainiac)

Make the code you just wrote *cleaner*, then apply the changes. This is a **quality**
pass, not a bug hunt: it improves reuse, simplicity, efficiency, and altitude
(logic living at the right layer). If you find a **bug**, note it and route it to
`/code-review` — do not try to fix behavior here.

Read `.claude/CLAUDE.md`, `crates/CLAUDE.md` conventions, and `context-map.json` first
so "reuse" and "altitude" mean the *real* Brainiac seams, not generic advice.

## Trigger

`/simplify` — the working diff vs `HEAD`.
`/simplify <file-or-glob>` — only matching files.
`/simplify --commit <ref>` — files changed in a commit or range.

## Scope discipline

- Touch **changed code and its immediate collaborators** — don't sweep the repo.
- **Behavior must not change.** A quality pass that alters semantics is out of scope;
  if a simplification would change behavior, stop and flag it instead.
- **Never weaken an invariant to "simplify":** keep every runtime query inside
  `scoped_tx` (RLS is load-bearing — never widen a read to trim a line), keep `core`
  DB-free, keep migrations append-only. Simpler must still be safe.

## The four lenses

Walk the changed code four times, one lens each.

### 1. Reuse — is this already solved?

- **Rust:** before adding a helper, check the crate that owns the concern:
  domain math (embed, fusion, rerank, scoring, temporal, health, `types`) lives in
  `core`; per-aggregate row mapping in `store` (memories, retrieval, governance,
  documents, tokens, …); provider/resilience in `gateway`; worker stages in `pipeline`.
  Collapse a re-implemented scorer / fusion step / row mapper into the existing one.
- **Console:** consume the generated OpenAPI types (`src/lib/api-schema.d.ts`, via
  `npm run gen:api`) — delete hand-written request/response interfaces that duplicate a
  schema type. Reuse existing `console/src/components` before hand-rolling UI.

### 2. Simplification — remove needless complexity

- **Rust:** prefer `?` + `.context(...)` over match-return-err; `if let` over one-arm
  `match` + wildcard; flatten nested conditionals; drop dead code, unused imports, and
  unused struct fields; replace an `.unwrap()` you can trivially turn into `?`
  (the workspace lints `clippy::unwrap_used = "warn"`).
- **Console:** collapse over-nested JSX into named sub-components; remove redundant state
  and derived-value duplication; delete commented-out and unreachable code.

### 3. Efficiency — stop doing redundant work

- **Rust:** batch DB round-trips in `store` (`WHERE id = ANY($1)` over per-row loops);
  don't recompute an embedding / fusion / rerank score already in hand; drop avoidable
  `.clone()` on large rows / `Vec<f32>` (use references or `Arc`); cap unbounded
  collections (retrieval `k`, sweep batches).
- **Console:** `useMemo`/`useCallback` on expensive transforms and large lists; lazy-load
  heavy libs (`recharts`, `mermaid`) behind route/dynamic boundaries.

### 4. Altitude — right logic, right layer

- **Rust:** move business/scoring math that leaked into `store` up into `core` (pure,
  unit-testable); move SQL that leaked into `core` down into `store`; put worker
  orchestration in `pipeline` and HTTP/MCP/CLI concerns in `server`. A function in the
  wrong crate is an altitude bug even when it "works".
- **Console:** keep secret-bearing data access behind `server-only`; split
  data-fetching containers from presentational components; lift shared logic into
  `src/lib` rather than duplicating across routes.

## Apply, then verify

1. Make the edits (small, reviewable, one lens/concern at a time).
2. Re-run the gates on what you touched — a simplification that breaks a gate is not a
   simplification:
   - Rust: `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D
     warnings` · `cargo test --workspace` (Postgres up, or pg tests self-skip and you
     haven't verified the data plane) · eval regression gates if you touched
     retrieval/scoring/fusion/rerank/temporal.
   - Console: `npx tsc --noEmit` · `npx vitest run` (`kb-data.test.ts` honesty guard stays
     green) · `npm run build`.
3. Commit atomically per coherent cleanup.

## Coordination & git discipline

Concurrent agents share this checkout on `master`.

- **Stage only your paths:** `git add <path>` per file — never `-A` / `.` / `-u`. Other
  sessions' in-flight edits show in `git status`; leave them alone.
- **Never `git stash`** work that isn't yours.
- **On `index.lock` contention**, wait 3–10s and retry up to 6 times; don't force-remove
  the lock.
- **Multi-file cleanups** go in a worktree under `.claude/worktrees/<slug>` (gitignored).
- After `git add`, before commit, run `git diff --cached --stat`; `git restore --staged
  <path>` anything you didn't add.

## Handoff

Report, per lens, what you changed (file:line, before→after intent), what you
**deliberately left** and why, and any **bugs found** — routed to `/code-review`, not
fixed here.

---

**Adapted for brainiac:** preserves the built-in `/simplify` method (diff-scoped,
quality-only, applies fixes, defers bug-hunting to `/code-review`) across its four lenses —
reuse, simplification, efficiency, altitude — retargeted to Brainiac's real seams: the
8-crate layering (`core` math / `store` RLS data plane / `pipeline` / `gateway` / `server`),
`anyhow` + `clippy::unwrap_used`, sqlx batching, and the Next.js console's OpenAPI-generated
types + `server-only` boundary. Guardrails added so a cleanup never bypasses `scoped_tx`/RLS,
touches a shipped migration, or leaks logic out of `core`.
