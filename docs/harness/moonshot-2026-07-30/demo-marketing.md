# Moonshot scan — demo-marketing

> Total: 2
> Files read: 18

## 1. The Attested Claim Ledger — a marketing page that fails CI

- **Severity**: Critical
- **Category**: truth-by-construction
- **File**: `console/src/pitch/pitch-data.ts:132` (the `rls` mechanism `artifact`), plus
  `console/src/pitch/pitch-data.ts:84` (PLAYERS), `:272` (MATRIX), `:316` (TRIAL), `:359` (POISON),
  `:392` (WEAKNESSES) · anchors: `migrations/0002_worker_read.sql:14`,
  `crates/brainiac-eval/src/gates.rs:89`, `results/baseline.json`, `.github/workflows/ci.yml:46`,
  `load/chainsonar/runs/2026-07-16/metrics.json`, `console/src/pitch/sections/EvidenceMatrix.tsx:88`

- **Scenario**: A skeptical staff engineer reads the pitch and clicks the ● in the Brainiac column
  of "Permission enforced by the database". A receipt opens: the claim id, the exact policy text
  *read out of the migration that is deployed*, the commit sha it was verified at, the CI run that
  proved it, the eval artifact hash showing `rls_leaks = 0` over 83 queries, and the timestamp. She
  clicks the Mem0 cell instead: a dated snapshot of the vendor's own doc, with the fetch date and a
  note that the sentence has not changed since. Then she clicks the one claim rendered grey — not
  gold, not magenta, *grey* — and reads "asserted; no verifier attached." Six weeks later a
  contributor edits `memories_read`, forgets the pitch, and CI goes red with
  `claim rls.db_enforced: artifact drifted from migrations/0002_worker_read.sql`. The page has
  become the one surface in this category that **cannot** outlive its proof, because a claim without
  a passing verifier is a build failure.

- **Root cause**: The pitch is hand-typed prose in a TypeScript literal with zero binding to the
  machinery that could prove it. Everything needed already exists and nothing is wired: `gates.rs`
  runs hard gates (RLS leaks, superseded-in-top3) and baseline-relative soft gates; `ci.yml:46`
  runs them per commit and uploads the artifacts; `results/*.json` + `results/history/` hold dated,
  per-embedder reports; `load/chainsonar/runs/2026-07-16/metrics.json` holds a real field trial with
  106 logged calls. The pitch cites none of it. It is not a hypothetical risk: the SQL block at
  `pitch-data.ts:132`, presented as *the* proof artifact, is a stale hand-copy of
  `0001_init.sql:252`. Migration `0002_worker_read.sql:14` replaced that policy — the live policy has
  `deleted_at IS NULL` and a worker bypass (`app.worker='on'` sees every non-private row). The
  single code block the page offers as checkable evidence is already not the policy that runs.
  Same class of exposure on the outbound side: `RETREAT` and `PLAYERS` rest entirely on vendor doc
  URLs with no fetch date and no snapshot, so the day Mem0 re-adds contradiction handling the page
  silently becomes the thing `pitch-data.ts:4-23` swears it will not be.

- **Impact**: ~60 discrete assertions across the page move from "trust the author" to
  machine-attested, and the count of *unfalsifiable* claims goes to zero — every claim either
  carries a verifier or is visibly marked unproven. Three compounding effects. (a) Drift becomes
  impossible: the honesty pass documented in `pitch-data.ts:4-23` is currently a one-time act of
  editorial discipline that decays; this makes it a ratchet, the same way `gates.rs` made refusal
  behaviour a ratchet. (b) The receipt bundle *is* the security-review artifact — a JSON feed of
  per-claim evidence at a commit is exactly what an enterprise reviewer asks for, collapsing weeks
  of questionnaire into a URL. (c) It is uncopyable positioning: a competitor can restyle this page
  in a day and cannot reproduce it without first building an eval harness with committed baselines
  and CI gates. The pitch's own thesis — "an eval harness that proves every claim" — becomes
  literally true of the page making the claim.

- **Fix sketch**:
  1. `claims/claims.toml` (schema alongside `fixtures/schema/*.schema.json`, validated the way
     `brainiac-fixtures/src/schema.rs` validates fixtures). Each entry: `id`, the sentence as shown,
     and one `verifier` of five kinds — `sql_region` (a named policy in a migration; verified by
     reading the *last* migration that defines it, plus an optional live `pg_policies` compare),
     `gate` (a field on `RetrievalReport` / a `regression_failures` check, resolved from
     `results/*.json`), `test` (a named test that demonstrates the behaviour — e.g. the
     contested-serving refusal whose wording lives at `crates/brainiac-server/src/mcp.rs:907`),
     `field` (a metric in `load/chainsonar/runs/<date>/metrics.json`, carrying its run date and n),
     `citation` (external URL + fetch date + hash of an archived snapshot).
  2. `brainiac claims verify` — a new arm on the `Command` enum in
     `crates/brainiac-server/src/main.rs:23`, beside `Serve` / `Mcp` / `eval`. It emits
     `results/claims/<sha>.json`: `{claim_id, status: verified|stale|unproven|failed, evidence_ref,
     artifact_hash, verified_at, commit}`. `stale` ≠ `failed` — a moved vendor doc must not red-build
     the repo, it must grey the cell and open a review item.
  3. A CI step directly after `ci.yml:46` (the eval artifacts it already uploads are the inputs).
     Hard-fail on `failed`, warn on `stale`.
  4. Console: `MechanismCard` (`Pitch.tsx:567`) renders `m.artifact` from the receipt rather than
     from the literal, and the `Matrix` cell renderer (`Pitch.tsx:781`) and `EvidenceMatrix`
     (`sections/EvidenceMatrix.tsx:88`) gain a receipt popover. `/claims.json` served publicly.
  5. **Smallest proving slice**: the six `MECHANISMS` entries only, `sql_region` verifier only.
     That one slice already deletes a live falsehood — the `rls` artifact — and produces the first
     red build for a drifted claim, which is the whole argument in one commit.
  6. Then extend to `TRIAL`/`POISON` as `test` + `field` verifiers (the POISON rounds are three
     reproducible cases and belong as an adversarial eval profile beside
     `crates/brainiac-eval/src/contradiction_profile.rs`), and to `PLAYERS`/`MATRIX`/`RETREAT` as
     dated `citation` verifiers with a scheduled re-check.

## 2. One command, a real org: a per-request-authenticated remote MCP surface turns the pitch into the install

- **Severity**: Critical
- **Category**: platform-surface
- **File**: `crates/brainiac-server/src/mcp.rs:302` (`serve_stdio`) and `:239`
  (`McpState::from_env`) · anchors: `crates/brainiac-server/src/mcp.rs:188` (`McpState.principal`),
  `:395` (`handle_message`), `:216` (`tool_scope`), `crates/brainiac-server/src/http.rs:170`
  (`auth_of`), `crates/brainiac-server/src/auth.rs:141` (`mint_secret`),
  `crates/brainiac-server/src/provision.rs:40`, `console/app/demo/page.tsx`,
  `console/src/pitch/Pitch.tsx:533` ("walk the demo org")

- **Scenario**: The reader finishes the poison section, and instead of a CTA into a diorama they get
  two lines to paste:
  `claude mcp add --transport http brainiac https://try.brainiac.dev/mcp -H "Authorization: Bearer brk_…"`.
  Sixty seconds later *their own agent*, in *their own terminal*, is talking to a real Brainiac over
  real RLS in an ephemeral org seeded from the twelve-team bank corpus. They ask the trial's second
  question — is a 15s client abort safe — and their agent answers correctly, citing a memory and a
  date, because the cross-team decision is actually in the store. Then they try to break it: they
  `memory_add` a false canonical claim about the PSP timeout, and their agent comes back saying the
  claim is contested, declining to adjudicate, escalating to a human. The three prose cards in
  §7 become something the reader *did*. The demo is no longer a rendering of the product; the demo
  is the product, and the demo is also the install — the same `claude mcp add` line they will run in
  production, against their own host.

- **Root cause**: MCP is stdio-only and single-principal by construction. `McpState` holds one
  `principal`, one `scopes`, one `project_id`, one `session_remote` (`mcp.rs:188-210`), all resolved
  once at process start from `BRAINIAC_MCP_TOKEN` (`mcp.rs:247`), and served over a newline-delimited
  stdin loop (`mcp.rs:302`). One process = one identity = one machine. This caps three things at
  once. (i) There is no way to *try* the product: every reader must clone a Rust workspace, run
  Postgres, run 34+ migrations and wire a model gateway before a single tool call — so
  `console/app/demo/page.tsx` can only ever be fixtures with `live:false` and writes disabled, and
  the CTA at `Pitch.tsx:533` promises a walk through a diorama. (ii) No hosted or shared deployment
  is possible at all: a team cannot point ten agents at one server over MCP, because the server can
  only be one principal. The REST surface has none of this limitation — `http.rs:170 auth_of` already
  resolves a bearer per request and `mcp.rs:216 tool_scope` already mirrors REST's scope gate — which
  is why the `load/` field test had to bypass MCP entirely and drive REST through `bx.mjs`
  (`load/README.md`, decision F1, "the MCP surface only accepts env-declared tokens"). The harness
  routed around the ceiling instead of the ceiling being lifted. (iii) The pitch's own listed
  weakness "it only helps if the agent asks" (`pitch-data.ts:406`) can never be measured against
  strangers' agents, only against our own.

- **Impact**: The conversion path collapses from "clone a Rust repo, provision Postgres, wire a
  gateway" to one command — for an MCP product that is the difference between a page people admire
  and a product people run, and it is the single largest addressable multiple on adoption in the
  repo. It simultaneously unlocks the deployment shape the product needs anyway: one server, many
  agents, each with its own scoped `brk_` key and its own RLS identity — i.e. the hosted/enterprise
  tier, and per-agent attribution for free. And it closes the evidence loop the other direction:
  every visitor session is real telemetry on which tools agents reach for unprompted, which is the
  exact measurement `WEAKNESSES[2]` says is missing. The POISON section stops being a claim and
  becomes a standing open invitation — the strongest possible form of the argument, and one no
  zero-config competitor can accept.

- **Fix sketch**:
  1. Split `McpState` in two. `McpState` keeps the process-wide parts (`store`, `embedder`,
     `embedding_version`); a new per-request `McpSession { principal, scopes, project_id,
     session_remote }` carries identity. Change `handle_message(&state, &msg)` (`mcp.rs:395`) to
     `handle_message(&state, &session, &msg)` and thread `session` where `state.principal` /
     `state.scopes` are read today. `serve_stdio` builds exactly one `McpSession` from
     `from_env`, so the existing stdio path and all its tests are behaviour-identical.
  2. Add `POST /mcp` to the router in `http.rs:89`: resolve the bearer with the *same*
     `crate::auth::resolve_bearer` that `auth_of` uses, build an `McpSession` from the resulting
     `AuthContext`, dispatch one JSON-RPC frame per request through the untouched `handle_message`.
     Because `tool_scope` is unchanged, REST/MCP scope parity holds by construction — the invariant
     the comment at `mcp.rs:213` already demands. Keep the `MAX_FRAME` bound (`mcp.rs:52`) as the
     body limit. Streaming/SSE is a later increment; request/response covers every existing tool.
  3. Kill finding F-2 in `load/README.md` for real, and let a future `load/` run drive MCP rather
     than `bx.mjs` — the harness's own decision table becomes the regression test for this work.
  4. Public instance: `provision.rs` already provisions an org; `auth::mint_secret` (`auth.rs:141`)
     already mints scoped keys — add a TTL + a reaper sweep beside `sweeps.rs`, and a rate-limited
     `POST /v1/try` that returns `{mcp_url, token, expires_at}` for a fresh visitor org cloned from
     the existing `fixtures/bank` corpus (loaded by `brainiac-fixtures/src/loader.rs` — no new
     fixture authoring, which is deliberately *not* the seed-demo scaffolding that was rejected
     before; the ambition here is the transport and the per-request principal).
  5. **Smallest proving slice**: steps 1–2 plus a single long-lived read-scope key against one
     shared read-only org, exposed as a "connect your agent" block on the pitch beside the existing
     CTA. That alone proves the transport, proves RLS holds for a stranger's agent, and makes
     `Pitch.tsx:533` point at something live. Ephemeral per-visitor orgs and write scope follow.
  6. Keep `/demo` as the no-install tour; it stops being the only option rather than being replaced.
