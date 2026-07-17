# Running the ChainSonar field test — the actual runbook

This records the run performed 2026-07-16, exactly, so it reproduces.

## 0. Two product fixes this run forced (both shipped)

Trying to onboard ChainSonar as a customer surfaced two bugs that made the
harness impossible. Both are fixed with regressions before anything else ran:

- **F-1: `lib:*`/`kb:*` scopes were unmintable.** `auth::SCOPES` listed only
  `read|write|admin`, so `POST /v1/tokens {scopes:["lib:read"]}` was rejected
  and only an `admin` key could reach the Library over REST. Fixed:
  `auth::SCOPES` now lists every enforced scope. Regression:
  `library_pg::the_token_endpoint_can_mint_every_enforced_scope`.
- **F-2: the MCP surface rejected managed keys.** `McpState::from_env`
  resolved only env tokens, so the `brk_` device key `/signup` mints "for the
  local device (the MCP agent)" failed — the onboarding→agent loop was broken
  end to end. Fixed: MCP resolves via `resolve_bearer` (env → api_tokens) AND
  now gates each tool by the token's scope. Regression:
  `library_pg::mcp_managed_key_resolves_and_its_scopes_gate_the_tools`.

## 1. The org + admin bootstrap

The self-serve `/v1/provision` flow mints one device key (`read`+`write`) per
identity — the free tier. A multi-developer org is ahead of what it supports,
so the org is bootstrapped with an admin env token (the same way an operator
would before the paid tier exists):

`load/chainsonar/.run-env.json` (gitignored) holds the generated ChainSonar
`org` UUID, `user` UUID, and `admin_secret`, plus the merged `BRAINIAC_TOKENS`
string. The server was restarted with it:

```bash
DB=$(grep '^DATABASE_URL=' .env | cut -d= -f2-)
TOKENS=$(node -e 'console.log(require("./load/chainsonar/.run-env.json").tokens)')
DATABASE_URL="$DB" BRAINIAC_TOKENS="$TOKENS" BRAINIAC_LIB_PROPOSE_PER_HOUR=50 \
  ./target-lb1/debug/brainiac.exe serve --with-worker --mock \
  > load/chainsonar/logs/server.log 2>&1 &
```

(`--mock` so the pipeline is deterministic and free; `PROPOSE_PER_HOUR=50` so
eight scanners sharing one `scan` identity are not throttled — a real finding
about the per-author limit under a shared scanner identity, noted in the report.)

## 2. The five scoped keys

```bash
export BX_ADMIN_TOKEN=$(node -e 'console.log(require("./load/chainsonar/.run-env.json").admin_secret)')
export CHAINSONAR_ORG=$(node -e 'console.log(require("./load/chainsonar/.run-env.json").org)')
node load/chainsonar/setup.mjs   # → load/chainsonar/.keys.json
```

Every key is minted through the real `POST /v1/tokens` — the path F-1 unblocked.
No developer key carries `lib:publish` or `admin`.

## 3. The scan (Opus subagents)

Four Opus scanners, each a slice of the repo, all on the `scan` key. Brief:
`scan.md`. They propose memories + standards; nothing is adopted yet.

## 4. Triage (me, as maintainer)

Using the `maintainer` (admin) key: review the proposal queue and the standards
gate, adopt what is real, reject what is noise. Rejections are remembered
(LB3). The volume and time here is itself measured.

## 5. The three developers

Three worktrees of ChainSonar (`worktrees/dev-{a,b,c}`, branches
`field/dev-{a,b,c}`), each on its own scoped key. Briefs: `agents/dev-*.md`.
They work in isolation — Brainiac is the only channel between them.

## 6. The report

`node load/chainsonar/report.mjs` aggregates the two logs into
`runs/2026-07-16/metrics.json`; the findings narrative is
`runs/2026-07-16/report.md`.

## The dedicated-database rule (learned the hard way)

**The field test MUST run against its own database, not the shared dev DB.**
During this run the harness shared `:5433/brainiac` with the `_pg` integration
suite, and running `cargo test -p brainiac-server` mid-run — to verify the F-1/
F-2 fixes — executed those tests' `TRUNCATE ... standards, memories, sources`
setup and **wiped the ChainSonar org's data**. The org's 8 proposed standards
and all extracted memories vanished; the corpus had to be rebuilt.

This is the exact hazard `uat/driver/seed.sh` hard-refuses ("REFUSING:
DATABASE_URL is not a run-scoped database … Two runs sharing a database
cross-pollute"). The field test should adopt the same guard: a
`brainiac_field_<date>` database, created and migrated for the run, so no test
suite and no other run can touch it. Recorded as finding **H-1** (harness).

## Teardown

```bash
# stop the server (find the pid in the start output), then:
git -C C:/Users/mkdol/.personas/projects/chainsonar worktree remove --force load/chainsonar/worktrees/dev-a  # etc
# the org's data stays in the dev DB; the keys in .keys.json can be revoked via POST /v1/tokens/{id}/revoke
```
