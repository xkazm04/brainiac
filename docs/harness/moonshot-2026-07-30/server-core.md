# Moonshot scan — server-core

> Total: 2
> Files read: 8

## 1. Brainiac in the write path — the agent-loop interposer

- **Severity**: Critical
- **Category**: agent-loop-interposer
- **File**: `crates/brainiac-server/src/mcp.rs:468` (the `memory_context` description — the
  pleading that marks the ceiling), plus `crates/brainiac-server/src/mcp.rs:434`,
  `crates/brainiac-server/src/http.rs:89-113` (route registration seam),
  `crates/brainiac-server/src/main.rs:23-143` (the clap `Command` enum),
  `crates/brainiac-server/src/projects.rs:44` / `:118` (`normalize_remote`,
  `normalize_path_prefix`), `docs/LIBRARY-PLAN.md:53` (Never #3) and `:132` (the deferred
  "enforcement hooks" item this supersedes in a bigger form).
- **Scenario**: An engineer onboards a repo once. Registration writes not only the MCP server
  entry but three agent hooks into the checkout's `.claude/settings.json`: `SessionStart`,
  `PreToolUse` on `Edit|Write|MultiEdit`, and `Stop`. From then on, *every time the agent is
  about to change a file*, the hook shells the same `brainiac` binary
  (`brainiac hook pre-edit`, reading the hook JSON on stdin) which POSTs
  `{remote, tool, changed_paths, diff_excerpt}` to `POST /v1/guard/pre-edit`. The server maps
  remote + path → project (`brainiac_store::projects::find_by_remote(pool, org, remote,
  path_prefix)` — already the monorepo lever, called at `mcp.rs:1265`), resolves the
  identifiers in the diff to canonical entities, and answers in one round trip:
  *"`payments-refund` — PITFALL: refunds must not be issued before the PSP settlement webhook;
  we shipped that in March and reverted it (memory:… , canonical, 2026-03-11). MANDATORY
  standard `psp-idempotency-key`."* The agent receives that as additional context on the way
  into the edit — **whether or not it thought to ask**. The `Stop` hook posts the session's
  touched entities back and asks the one question nobody asks today: did anything here deserve
  to be a memory? Brainiac stops being a library the agent may visit and becomes a plane the
  agent's edits pass through.
- **Root cause**: Knowledge delivery is entirely **opt-in and prompt-engineered**. The tool
  descriptions in `tool_definitions()` are, read honestly, a plea: *"Reach for this MID-TASK,
  not just at the start… especially deep into a long task, when your earlier context has
  drifted"* (`mcp.rs:434`) and *"Organizational knowledge does not announce itself — if you
  never ask, you build as if the org knows nothing"* (`mcp.rs:468`). That last sentence is the
  product's ceiling stated in its own source. Every governance investment behind it — the
  candidate floor in `parse_filters`, contested withholding at `mcp.rs:863-941`, provenance
  refs, the canonical-only bundle — only pays out on calls the agent *elects* to make. There is
  no code path in this crate where brainiac reaches the agent first. A stdio JSON-RPC loop
  (`serve_stdio`) that only ever answers is structurally incapable of it.
- **Impact**: Delivery coverage goes from voluntary-and-unmeasured to **100% of write events**.
  A real agent session performs tens of edits and, generously, one to three `memory_search`
  calls — so the org's knowledge currently reaches the moment of decision on a low
  single-digit percentage of the decisions it was captured for. This also creates the product's
  first *causal* metric: "N canonical pitfalls delivered at the edit that touched their
  anchored entity, M of them acknowledged" — attributable prevention, which is the number that
  makes an org-memory server a purchase rather than a hope. It is the same move that made
  linters and type-checkers inevitable: not better rules, but rules that arrive in the write
  path.
- **Fix sketch**: New `crates/brainiac-server/src/guard.rs` (module declared in `lib.rs:5-16`,
  merged at `http.rs:113` alongside `crate::projects::routes()`), registered in
  `openapi.rs:68` so the console's generated types come along for free.
  *Slice 1 (proves it, no model call, no new dependency):* `POST /v1/guard/pre-edit` gated on
  the existing `read` scope via `auth_of`; body = remote + changed paths + a bounded diff
  excerpt (reuse the `within_cap` discipline from `mcp.rs:153`); resolve project with
  `projects::normalize_remote` + `normalize_path_prefix` + `find_by_remote`; run
  `brainiac_store::retrieval::search` with
  `RetrievalFilters { kinds: vec![MemoryKind::Pitfall, MemoryKind::Decision], project_id,
  min_status: Some(MemoryStatus::Canonical), .. }`; union with
  `library::list_standards(&mut tx, stack, Some(Adopted))` filtered to
  `enforcement == mandatory`; return `{decision: "allow", notes: [..], memory_ids: [..]}`.
  Add `Command::Hook { .. }` to the clap enum at `main.rs:23` so the binary devs already have
  becomes the hook — stdin JSON in, hook JSON out, no npm package, no second artifact. Emit the
  `settings.json` snippet from the existing `docs/skills/brainiac-onboard/SKILL.md` flow. Test
  in a new `crates/brainiac-server/tests/guard_pg.rs` mirroring `mcp_pg.rs`: a pitfall anchored
  to `payments` surfaces for an edit under that project's `path_prefix`, and never crosses an
  org (RLS via `scoped_tx`, same as every handler here).
  *Hot-path discipline:* this sits in front of a human's edit, so budget ~150 ms, memoize per
  session on the client side, and **fail open** on timeout — degrade to `allow` with a warning
  line, borrowing the gateway's circuit-breaker posture. A guard that stalls the agent gets
  uninstalled on day two.
  *Slice 2:* per-org `guard_mode: off | warn | block`, default `warn`. This is the honest
  relaxation of LIBRARY-PLAN Never #3 (`docs/LIBRARY-PLAN.md:53`) and it satisfies that plan's
  own condition — "report, then warn, never silently block at first" (`:132`). Note the
  difference from that backlog item: it proposed CI *compliance* annotation; this delivers
  *knowledge* at the keystroke, which is a strictly larger surface and the one the mission
  statement is about.
  *Slice 3:* MCP parity — an `edit_preflight` tool in `tool_definitions()` (scope `read` in
  `tool_scope`) so agent clients with no hook system reach the same computation, keeping the
  REST/MCP parity invariant this crate already holds.

## 2. The stateful session — brainiac as a participant, not an oracle

- **Severity**: High
- **Category**: stateful-agent-protocol
- **File**: `crates/brainiac-server/src/mcp.rs:415` (`capabilities: { "tools": {} }`),
  `crates/brainiac-server/src/mcp.rs:188-211` (`McpState` — immutable, per-process),
  `crates/brainiac-server/src/mcp.rs:302-350` (`serve_stdio`, a strict read→reply loop),
  `crates/brainiac-server/src/mcp.rs:372-391` (`write_frame`, already the isolated writer),
  `crates/brainiac-server/src/mcp.rs:934-941` (`contested_withheld` — the dead-end),
  `crates/brainiac-server/src/mcp.rs:1295-1302` (`memory_add`'s throw-over-the-wall reply).
- **Scenario**: The agent's `initialize` (or an explicit `session_open` carrying `task_hint`)
  opens a durable `agent_sessions` row: org, project, remote, task hint, principal, watermark.
  Three things become possible that are impossible today. **(a) Deltas.** Every tool response
  gains a `since_last_call` block: canon that changed in the entities this session has touched,
  proposals of yours that were decided, and — closing the product's most conspicuous dead end —
  *contradictions that were resolved*, including the ones an earlier `memory_search` withheld
  from you. **(b) Push.** The server declares real capabilities and writes unsolicited
  `notifications/message` frames: forty minutes into a refactor, a maintainer canonicalizes a
  reversal in your area and brainiac **interrupts** — "the decision you are building on was
  reversed nine minutes ago". **(c) Coordination.** Fifty agents in one org are fifty
  independent sensors that currently cannot see each other. With a session registry, two
  sessions capturing the same pitfall independently becomes a *convergent-evidence signal*
  attached to the review queue item (a far stronger promotion argument than one agent's
  proposal), and two agents about to change the same entity each learn the other exists.
- **Root cause**: The MCP surface is deliberately amnesiac. `McpState` is built once in
  `from_env` and shared immutably; `handle_message` is a pure function of `(state, msg)` with no
  continuity between frames; `capabilities` advertises `{ "tools": {} }` and nothing else — no
  `resources`, no `logging`, no `listChanged` — so the protocol contract itself forbids the
  server from speaking first; and `serve_stdio`'s only writer is `write_frame`, called from
  inside the request path. A grep for `session`/`notification`/`subscribe` across this crate
  returns nothing but doc-comments explaining that notifications *get no reply*. The result: a
  memory server with no memory of the conversation it is in. Every governance event the product
  works so hard to produce — a promotion, a resolution, a dispute verdict — lands in a database
  that has no way to tell the agent who needed it.
- **Impact**: Turns a lookup into a **subscription**, which is the difference between a
  reference work and a colleague. Two concrete 10x claims. First, the withheld-contested path
  today ends the story: `mcp.rs:934` tells the agent "3 results withheld — a maintainer must
  reconcile this", and when the maintainer does, the agent that needed the answer is long gone.
  Sessions make governance latency invisible instead of fatal. Second, the ranking/feedback loop
  currently learns only from what an agent voluntarily reported via `memory_feedback` — a few
  verdicts per session at best; a session ledger of served-vs-cited-vs-ignored yields one to two
  orders of magnitude more ranking signal per session at zero agent effort, and it is the
  denominator the `library_usage_events` telemetry never had. It is also the flywheel: each
  additional agent in the org makes every other agent better, in-org, with no cross-org data
  sharing to negotiate.
- **Fix sketch**: *Schema:* `agent_sessions (id, org_id, project_id, remote, task_hint,
  principal_user, started_at, last_seen_at, watermark timestamptz, touched_entities uuid[])` plus
  `session_events`, both org rows read through the existing `scoped_tx` so RLS is inherited and a
  session id from another principal is simply not-found (same no-existence-oracle posture as
  `memory_provenance`).
  *Slice 1 — works on every MCP client alive today, no protocol change:* give `McpState` a
  `session: tokio::sync::Mutex<SessionCursor>` (`serve_stdio` already holds an
  `Arc<McpState>`, so this is additive and `handle_message` stays directly test-drivable the way
  `tests/mcp_pg.rs` drives it). Accumulate touched entities from the anchors already returned by
  `entities::canonical_anchors_for` / `retrieval::search`, and attach a `since_last_call` block
  to every tool result in `call_tool` (`mcp.rs:695`), computed with the batched helpers already
  present: `governance::open_contradictions_for` (now reporting *resolved* since the watermark),
  the `promotions` rows `knowledge_propose` writes, and status changes on watched memories.
  Piggybacking on responses is what makes this shippable before any client supports push.
  *Slice 2 — real push:* split `serve_stdio` into a reader task and a writer task communicating
  over `mpsc::Sender<Value>` (`write_frame` is already factored out for exactly this), declare
  `{"tools":{"listChanged":true},"logging":{}}` at `mcp.rs:413`, and drive the writer from a
  Postgres `LISTEN`/`NOTIFY` (or, simplest first, a watermark poll on the session row) so a
  canonicalization in a watched entity emits `notifications/message`. Gate behind
  `BRAINIAC_MCP_PUSH` so a client that ignores unsolicited frames is byte-for-byte unaffected —
  the same opt-in discipline `session_remote` and the reranker seam already use.
  *Slice 3 — the coordination surface:* `GET /v1/sessions/live` (merged in `http.rs`, registered
  in `openapi.rs:68`) giving the console its first *present-tense* module — who is working
  where, right now, in a product whose every existing surface is retrospective — and a
  convergent-capture badge on review-queue items when N independent sessions recorded the same
  thing. Pairs directly with moonshot #1: the guard's pre-edit calls are the richest possible
  source of "what this session is touching", so the two together give the server a continuous,
  accurate model of the work in flight.
