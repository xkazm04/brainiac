---
name: sentry
memory: none
category: Maintenance
description: Fetch active Sentry issues for the Brainiac project, diagnose root causes from stack traces, apply code fixes, and mark issues as resolved. Covers both the Rust workspace (tracing::error!, panics — captured via sentry + sentry-tracing in brainiac-server) and the Next.js console (@sentry/nextjs exceptions, unhandled rejections, server request errors).
allowed-tools: Read, Write, Edit, Grep, Glob, Bash
---

# Sentry Issue Triage & Fix Pipeline (Brainiac)

You process **unresolved Sentry issues** for Brainiac. For each issue: fetch the
stack trace, locate the offending source in the Rust workspace or the console,
propose a fix, apply it after user approval, verify it compiles/tests, commit, and
mark the issue resolved in Sentry via the API.

## How Brainiac reports errors (verified — read before mapping any frame)

**Rust backend** — `crates/brainiac-server/src/main.rs`:
- `init_sentry()` (`main.rs:226`) returns `None` (fully inert) when
  `BRAINIAC_ENV` is unset or `"local"`, or when `SENTRY_DSN` is unset/blank.
  Otherwise `sentry::init` with `environment: BRAINIAC_ENV` and `release:
  sentry::release_name!()`.
- The guard is bound in `main()` (`main.rs:251`) for the process lifetime so
  queued events flush on exit.
- `sentry_tracing::layer()` is installed in the `tracing_subscriber` registry
  (`main.rs:261`) — **`tracing::error!` events become Sentry events**; panics are
  captured by the `panic` feature. Deps: `sentry` 0.48 (features `backtrace`,
  `contexts`, `panic`, `reqwest`, `rustls`) + `sentry-tracing` 0.48 in
  `crates/brainiac-server/Cargo.toml:40`.
- Consequence for triage: a Rust "issue" is often not a crash — it is an
  `error!`-level log from a worker or handler. Read the surrounding code before
  assuming a panic.

**Console (Next.js 15 / React 19)** — `@sentry/nextjs` ^10:
- `console/instrumentation.ts` — `register()` imports `./sentry.server.config`
  on the `nodejs` runtime and `./sentry.edge.config` on `edge`; exports
  `onRequestError = Sentry.captureRequestError`.
- `console/sentry.server.config.ts`, `console/sentry.edge.config.ts`,
  `console/instrumentation-client.ts` — each `Sentry.init` only when
  `NEXT_PUBLIC_SENTRY_DSN` is set, `environment: NEXT_PUBLIC_BRAINIAC_ENV ||
  "local"`, `tracesSampleRate: 0` (error tracking only, no performance tracing).
  The client config also exports `onRouterTransitionStart`.

**No `before_send` / `beforeSend` hook exists on either side** (verified by grep).
There is therefore **no PII scrubbing today** — if an issue's payload contains org
content, memory text, or a token, that is itself a finding worth surfacing to the
user, not something to quietly fix inside a triage run.

**Deploy wiring**: `SENTRY_DSN` and `NEXT_PUBLIC_SENTRY_DSN` are set in
`.env.deploy` (template: `.env.deploy.example:104-107`), consumed by
`docker-compose.deploy.yml`.

## Constants

- **Sentry org / project**: resolved at Phase 0 from `.env` (`SENTRY_ORG`,
  `SENTRY_PROJECT`). They are **not** committed anywhere in this repo — do not
  guess them. If absent, derive nothing; ask the user once and offer to append
  the two keys to `.env`.
- **Auth**: `SENTRY_AUTH` from `.env` at repo root (user auth token, `sntryu_*`).
  Not present in `.env` today — Phase 0 aborts with instructions if missing.
- **API base**: `https://sentry.io/api/0`
- **Triage output**: `.claude/sentry/TRIAGE-<YYYYMMDD>.md`
- **Never print any `.env` value** (`SENTRY_AUTH`, `BRAINIAC_API_TOKEN`,
  `QWEN_API_KEY`, `DATABASE_URL`) — read into a shell var, never `echo`.

## Trigger

- `/sentry` — list all unresolved issues, pick one or a batch to fix
- `/sentry list` — list only, no fixes
- `/sentry fix <issue-id>` — go straight to fixing a specific issue
- `/sentry top <N>` — triage the N issues with the highest event count (default 5)
- `/sentry stats` — summary: unresolved count, frequency buckets, affected releases

---

## Coordination — concurrent sessions

Other CLI sessions (`/perfect`, `/uat`, `/explorer`, `/architect`) may be editing
the same tree. Before applying any code fix, read `.claude/active-runs.md` if it
exists (create it with an `## Active` / `## Recently completed` pair if it
doesn't) and append one entry declaring your scope. If a `started` entry <2h old
overlaps your planned files, surface the conflict before proceeding. Overlap on
`.claude/active-runs.md` itself is expected and is not a conflict.

**Declared paths for `/sentry`:** scope is per-issue. After Phase 1 fetches issues
and the user picks, declare:
- The Rust/TS files at the top of each chosen issue's `inApp` frames (typically
  `crates/brainiac-*/src/**/*.rs` and `console/src/**/*.{ts,tsx}`)
- `.claude/sentry/TRIAGE-<YYYYMMDD>.md`
- Always: `.claude/active-runs.md`

Batch-fixing N issues that touch overlapping files → register ONE entry covering
the union of paths.

**At session end** (after the final commit + the last resolve API call): move your
entry to the top of `## Recently completed` with `Status: completed (commit:
<sha>)` (or several SHAs) or `aborted (<reason>)`. Trim entries older than 14 days.

### Parallel-safety primitives (mandatory)

1. **Never `git stash`** other sessions' work — not even `--keep-index`. Use
   `git add <path>` per file (NOT `git add -A` / `.` / `-u`).
2. **Use a worktree for multi-issue batches.** Single-issue fixes touching ≤1 file
   can stay on the main checkout. For batch mode:
   ```bash
   git worktree add .claude/worktrees/sentry-<YYYYMMDD> -b worktree-sentry-<YYYYMMDD>
   cd .claude/worktrees/sentry-<YYYYMMDD>
   ```
   (`.claude/worktrees/` is the repo's existing worktree home.)
3. **Atomic commits per issue** — never bundle N issue fixes into one commit. The
   Sentry shortId belongs in the subject so the resolve call and the commit are
   traceable from each other.
4. **Verify the staged index before commit:** after `git add`, run
   `git diff --cached --stat`. If more files are staged than you added, another
   session pre-staged work — `git restore --staged <path>` per unrelated file, or
   `git commit --only <files>`.
5. **Clean up the worktree after merge:** once the branch is in `git log master`,
   `git worktree remove .claude/worktrees/sentry-<YYYYMMDD>` and
   `git branch -D worktree-sentry-<YYYYMMDD>`.

---

## Phase 0: Load auth and smoke-test the API

```bash
TOKEN=$(grep -E "^SENTRY_AUTH=" .env | cut -d= -f2- | tr -d '\r')
ORG=$(grep -E "^SENTRY_ORG=" .env | cut -d= -f2- | tr -d '\r')
PROJ=$(grep -E "^SENTRY_PROJECT=" .env | cut -d= -f2- | tr -d '\r')
if [ -z "$TOKEN" ] || [ -z "$ORG" ] || [ -z "$PROJ" ]; then
  echo "ERROR: need SENTRY_AUTH, SENTRY_ORG, SENTRY_PROJECT in .env"
  exit 1
fi

# Smoke test — confirm token is valid and project is reachable
curl -sS -H "Authorization: Bearer $TOKEN" \
  "https://sentry.io/api/0/projects/$ORG/$PROJ/" \
  | python -c "import sys,json; d=json.load(sys.stdin); print('Project OK:', d.get('slug'))"
```

Abort if the smoke test fails (bad token, project renamed, org changed). Never
echo `$TOKEN`.

Ensure `.claude/sentry/` exists: `mkdir -p .claude/sentry`.

---

## Phase 1: Fetch unresolved issues

```bash
curl -sS -H "Authorization: Bearer $TOKEN" \
  "https://sentry.io/api/0/projects/$ORG/$PROJ/issues/?query=is:unresolved&statsPeriod=14d&limit=100&sort=freq" \
  > .claude/sentry/issues.json
```

Parse with python/jq. Fields to keep per issue:

| Field | Source path | Use |
|---|---|---|
| `id` | `.id` | API key for resolve PUT |
| `shortId` | `.shortId` | human ref |
| `title` | `.title` | Commit message context |
| `culprit` | `.culprit` | Fast jump to module/file:func |
| `level` | `.level` | `error` / `fatal` / `warning` |
| `count` | `.count` | Event frequency |
| `userCount` | `.userCount` | Distinct affected users |
| `firstSeen` / `lastSeen` | `.firstSeen` / `.lastSeen` | Active or dormant? |
| `platform` | `.platform` | `rust` → workspace, `javascript`/`node` → console |
| `metadata.type` | `.metadata.type` | Exception class |
| `metadata.value` | `.metadata.value` | Exception message |
| `permalink` | `.permalink` | Link to Sentry UI |

Also read the `environment` tag: Brainiac only reports from non-`local`
`BRAINIAC_ENV` (typically `test` / `production`). An issue tagged `local` means
someone ran with a DSN in dev — worth flagging, rarely worth fixing.

Filter out noise before presenting:
- Skip `level: warning` unless the user passed `--include-warn`
- Skip issues not seen in the last 7 days (stale — probably already fixed)
- Skip issues with count == 1 AND lastSeen > 3 days ago (likely flukes)

---

## Phase 2: Present triage table — one approval gate

Write `.claude/sentry/TRIAGE-<date>.md`:

```markdown
# Sentry Triage — 2026-07-26

Total unresolved: 42
Filtered (active, >7d): 18

| # | shortId | lvl | count | users | lastSeen | platform | env | title |
|---|---------|-----|-------|-------|----------|----------|-----|-------|
| 1 | {shortId} | err | {count} | {users} | {ago} | rust | {environment tag} | {issue title verbatim from the API} |
| 2 | ... |
```

Show the user this table and ask:

> Which do you want to fix? Reply with indexes (`1,2,5`), `all`, `top 3`, or `skip`.

**Do not proceed without explicit user selection.** This is the single approval gate.

---

## Phase 3: Fix each selected issue

For each issue (iterate sequentially — fixes may depend on each other):

### 3.1 Pull the latest event with full stack

```bash
ISSUE_ID="<numeric id>"
curl -sS -H "Authorization: Bearer $TOKEN" \
  "https://sentry.io/api/0/issues/$ISSUE_ID/events/latest/" \
  > .claude/sentry/event-$ISSUE_ID.json
```

Extract:
- **Exception class + message**: `entries[].data.values[0].type` + `.value`
- **Frames**: `entries[].data.values[0].stacktrace.frames[]` — `filename`,
  `function`, `lineNo`, `inApp`, `context`
- **Breadcrumbs**: UI/state just before the failure
- **Tags**: `release` (from `sentry::release_name!()` on Rust), `environment`
  (`BRAINIAC_ENV` / `NEXT_PUBLIC_BRAINIAC_ENV`), `server_name`
- **Request** (console only): `entries[] | select(.type=="request") | .data.url`

Focus on `inApp: true` frames. For Rust, `sentry-tracing` events carry the
`tracing` span/field context — read the fields (`org_id`, `stage`, `path`, …);
they usually localize the bug faster than the backtrace.

### 3.2 Locate the source

Map Sentry frames to local paths:

| Sentry frame | Local source |
|---|---|
| `brainiac_pipeline::extract::…` | `crates/brainiac-pipeline/src/extract.rs` (search for `fn <name>`) |
| `brainiac_store::retrieval::…` | `crates/brainiac-store/src/retrieval.rs` |
| `brainiac_server::routes::…` / `auth` | `crates/brainiac-server/src/` |
| `brainiac_gateway::providers::…` | `crates/brainiac-gateway/src/providers/*.rs` |
| `brainiac_core::…` | `crates/brainiac-core/src/` |
| `app/…` or `src/…` (Next.js) | `console/src/…` / `console/app/…` |
| Minified JS (no sourcemap) | Fallback: grep for a unique string fragment from the message |

Crate → path rule: the Rust module path's first segment `brainiac_<x>` maps to
`crates/brainiac-<x>/src/`, remaining segments to the module file. Read the file
at the reported line before theorizing.

### 3.3 Diagnose

Write a short diagnosis (4–6 bullets) covering:
- What was running (HTTP handler? `worker` tick? `eval` run? MCP surface? console
  server component?) — from breadcrumbs, tracing fields, and `request.url`
- Why it fails (unwrap/None, sqlx error, RLS denial, provider timeout/circuit
  breaker open, JSON parse of model output, missing env, race)
- Whether it's a **real bug** or an **expected failure that needs better
  handling** (upstream LLM 429, Postgres restart, missing `DATABASE_URL`)
- Scope: one-line guard, or structural?

Brainiac-specific suspicions worth checking early:
- **RLS**: a query that ran outside `scoped_tx` / without `SET LOCAL app.org_id`
  will fail or return nothing — never "fix" it by widening a read
  (`.claude/CLAUDE.md` § Design standards; `crates/brainiac-store/src/lib.rs` header).
- **Crate layering**: the fix belongs in the layer that owns the concern — domain
  math in `core`, SQL in `store`, orchestration in `pipeline`, HTTP in `server`.
- **Gateway**: timeouts/retries/circuit-breaking already exist
  (`crates/brainiac-gateway/src/resilience.rs`, `providers/mod.rs:36`) — extend
  them, don't add a second ad-hoc retry at the call site.
- **Model output**: unparseable JSON from a provider is expected; the repair loop
  in `crates/brainiac-pipeline/src/extract.rs` and the tolerant
  `extract::extract_json_object` are the sanctioned shapes.

If the issue is **not actionable** (a user's one-off infra failure, third-party
flake, dev-only path), mark for resolution with justification and skip code
changes — jump to 3.6 with a `wontfix` body.

### 3.4 Apply the fix

Use Edit directly on the source. Style (inherits from `.claude/CLAUDE.md`):
- `anyhow::Result` with `.context(...)`; prefer `tracing::error!` with structured
  fields over bare `bail!` when the caller can continue. Note the feedback loop:
  every `tracing::error!` you add is itself a future Sentry event — use `warn!`
  for expected-and-handled conditions.
- The workspace lint `clippy::unwrap_used = "warn"` (`Cargo.toml:59`) — never fix
  a panic by adding another `.unwrap()`.
- Console: types come from the generated OpenAPI schema (`npm run gen:api`) —
  never hand-write request/response shapes; server-only data access stays behind
  `server-only`.
- No extraneous comments (only when the WHY is non-obvious); no backwards-compat shims.
- Migrations are append-only and numbered — if a fix needs schema change, add
  `migrations/NNNN_*.sql`; never edit a shipped migration.

Verify locally (mirrors `.github/workflows/ci.yml`):

```bash
# Rust fix:
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # needs docker compose up -d + DATABASE_URL,
                                # else pg tests SELF-SKIP and you verified nothing
# Console fix (from console/):
npx tsc --noEmit && npx vitest run && npm run build
```

For a fast inner loop, `cargo check -p brainiac-<crate>` is fine, but the
clippy + test gates must pass before you resolve the issue.

If the touched code is covered by an eval profile (pipeline/extraction/docs/
drift/divergence/retrieval/contradiction/resolution), run it against the
committed baseline — and **never** `--write-baseline` as part of a Sentry fix.

### 3.5 Commit

One commit per issue, referencing the shortId:

```bash
git add <fixed files> && git diff --cached --stat
git commit -m "$(cat <<'EOF'
fix(<crate-or-console-area>): <one-line summary>

Sentry: <shortId> — <permalink>
Root cause: <1-2 sentences>
Fix: <1 sentence describing the change>

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

Follow the repo's commit style (lowercase conventional commits with a scope —
`git log --oneline -10` shows `feat(eval)`, `chore(cost)`, `test(gateway)`).

### 3.6 Mark resolved in Sentry

```bash
curl -sS -X PUT -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  "https://sentry.io/api/0/issues/$ISSUE_ID/" \
  -d '{"status":"resolved","statusDetails":{"inNextRelease":true}}'
```

`inNextRelease: true` auto-reopens the issue if it recurs after the next release
deploys — right for a code fix that ships in a later image build. For `wontfix`,
use `{"status":"ignored"}`.

Record in the triage file:

```markdown
## BRA-1A — Fixed ✅
- Commit: abc1234
- Files: crates/brainiac-store/src/retrieval.rs
- Strategy: bounded the acquire with a context'd timeout instead of unwrapping
```

---

## Phase 4: Summary report

Append to the triage file and print:

```markdown
## Summary — 2026-07-26 15:42

- Issues triaged: 18
- Fixed and resolved: 12
- Ignored (wontfix): 2
- Skipped (needs design decision): 4
- Commits created: 12
- Files changed: 19
```

---

## Safety rails

1. **Never resolve an issue you didn't fix.** "Skip this one" = leave the Sentry
   status untouched.
2. **Never commit without the gates passing** — clippy `-D warnings` + tests for
   Rust, `tsc --noEmit` + `vitest run` for the console.
3. **Never bulk-resolve without individual inspection.**
4. **Never fetch more than 100 issues per call** — paginate via the `Link` header.
5. **Don't log secrets.** Read `.env` values into shell vars once; never `echo`.
6. **Never widen an RLS scope to make an error go away**, and never bypass
   `scoped_tx`. If the honest fix is a policy change, stop and surface it.
7. **Never recalibrate an eval baseline** to make a gate pass during a fix.
8. **PII**: neither runtime has a scrubbing hook. If an event payload contains org
   memory content or credentials, report it to the user as its own finding — a
   `before_send` / `beforeSend` filter is a design change, not a triage fix.

## References

- Sentry API docs: https://docs.sentry.io/api/
- Issue status values: `resolved`, `unresolved`, `ignored`
- Rust init: `crates/brainiac-server/src/main.rs:217-269`
- Console init: `console/instrumentation.ts`, `console/sentry.server.config.ts`,
  `console/sentry.edge.config.ts`, `console/instrumentation-client.ts`
- Deploy env: `.env.deploy.example:100-117`, `docker-compose.deploy.yml`
- Project rules: `.claude/CLAUDE.md` (crate layering, RLS, quality standards)
