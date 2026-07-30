# Moonshot scan — console-shell

> Total: 2
> Files read: 18

## 1. The Console Address — make every view in the console a link an agent can mint

- **Severity**: Critical
- **Category**: platform-surface
- **File**: `console/src/design/routes.ts:125` (`parseModule` — the entire addressing
  vocabulary is one query param), plus the anchors that prove the fragmentation:
  `console/app/console/page.tsx:101` (raw `searchParams` handed to each module),
  `console/app/console/modules/memories/Archive.tsx:136` (`selected` is client `useState`),
  `console/app/console/modules/graph/CortexMap.tsx:60` (its own `url.searchParams` +
  history dialect), `console/app/console/modules/standards/StandardsBoard.tsx:81`
  (selection never leaves React), `console/src/lib/api.ts:129` (the client that would
  mint links server-side), `crates/brainiac-server/src/mcp.rs:433` (the tools that would
  return them).

- **Scenario**: A coding agent in a developer's editor calls `memory_search` and gets back
  a contradiction blocking its answer. Today it can only say "there is a conflict" and
  stop. In the world this creates, every MCP result carries a **console address** — a
  single URL that opens the console on exactly that object, in exactly that view, with
  exactly those filters and that as-of instant: `/console?m=disputes&sel=<id>&…`. The agent
  ends its turn with "I could not answer this; a human decides here: <link>". That link is
  paste-able into a PR review, a Slack thread, an incident doc, a Jira ticket. A maintainer
  clicks once and lands on the work, already scoped. Audit entries store the address the
  decision was made from, so "what did the reviewer actually see" becomes replayable
  rather than reconstructed. Maintainers pin addresses — "my rust standards at proposed",
  "cross-team contradictions older than 7 days" — and the shell's nav grows a saved-views
  rail beside the three static groups. A wall-mounted display is just an address.
  The console stops being a destination people remember to visit and becomes an
  **addressable surface other systems point into**.

- **Root cause**: The shell has exactly one addressing concept — `?m=<segment>` — and
  `parseModule` is the whole parser. Everything below the module boundary invented its own
  dialect or none at all: `audit` speaks `kind/offset`, `disputes` `filter/page`,
  `memories` `filter/page/sort/dir/asOf`, `reviews` `cstatus/poffset`, while `graph`,
  `health`, `standards`, `skills`, `ingest` and `projects` put nothing in the URL. Worse,
  the thing a link most needs to carry — *which object is open* — is client state
  everywhere: `Archive.tsx` holds `selected` in `useState`, `StandardsBoard` holds
  `selectedId`, `CortexMap` pushes history by hand. So of ~14 modules, **zero can produce
  a link to a specific item.** That single missing primitive caps the entire product loop:
  Brainiac's governance depends on a human deciding, but there is no way to *hand a human
  a decision*. The push direction of the loop does not exist — only pull, only polling,
  only a maintainer who happens to open the tab.

- **Impact**: Turns a pull-only console into a push-capable one, which is the difference
  between governance latency measured in *whenever someone looks* and measured in
  *minutes*. Concretely: today `NavDashboard` polls a scalar count every 30 s and that is
  the only nudge that exists — a pending promotion is invisible unless a human navigates.
  With addresses, every one of the ~8 write-bearing surfaces (promotion approve/reject,
  contradiction resolve, dispute decision, doc revision approve, standard adopt/reject/
  deprecate, onboard decide) becomes reachable from wherever the work was discovered.
  It also multiplies every already-shipped MCP tool: `memory_provenance`,
  `memory_context`, `memory_status`, `standards_for` each gain a human escalation path at
  the cost of one extra JSON field. And it is the precondition for three things that are
  otherwise unbuildable — notifications, embeddable views, and citable audit evidence.

- **Fix sketch**:
  1. **Slice one (pure frontend, no backend, proves everything).** In
     `src/design/routes.ts`, add a typed `ConsoleAddress`
     (`{ module: ConsoleModuleId; view?: string; sel?: string; asOf?: string;
     filters: Record<string,string> }`) with `encodeAddress`/`decodeAddress` beside the
     existing `parseModule`, and a per-module `AddressSpec` declaring its legal filter
     keys — the same registry that already made the nav undriftable now owns the URL
     grammar. Fold the five existing private parsers (`parseFilter` in
     `modules/memories/Module.tsx:128` and `modules/disputes/Module.tsx:57`, `parseWiki`,
     `parseKind`/`parseOffset`, `asStatus`/`asOffset`) onto it so there is one dialect.
     Lift `selected` / `selectedId` / `ViewId` into `sel` / `view` on the address — this
     is the load-bearing change, and it is a handful of `useState` → `useSearchParams`
     swaps. Then add one shared primitive next to `SectionRail`/`Divider`:
     `<AddressBar/>` in `app/chrome.tsx`'s top row (which is display-only today) with
     copy-link, and it is done — every module inherits shareability at once.
  2. **Slice two (the handoff).** In `crates/brainiac-server/src/mcp.rs`, give each read
     tool's result a `console_url` built from a new `console_url(org, address)` helper
     (base from config, same place `BRAINIAC_API_URL` is mirrored); `memory_search` hits
     point at `m=memories&sel=<id>`, `memory_status`'s contradictions at
     `m=disputes&sel=<id>`, `standards_for` at `m=standards&sel=<slug>`. Add one new tool
     — `escalate(memory_id|contradiction_id, note)` — that files the item and returns the
     address, so an agent's "I need a human" is a single call. No schema change.
  3. **Slice three (saved views).** Persist named addresses per identity (the `identities`
     table already exists, migration `0022`) behind `GET/POST /v1/console/views`, render
     them as a fourth nav group in `chrome.tsx`. Then stamp the acting address onto audit
     rows so `modules/audit` can link "the view this decision was made from".
  Smallest proof: `Archive.tsx` selection in the URL + copy-link in the chrome. If a
  maintainer can paste "look at this memory" into a chat, the primitive is real.

## 2. One live field — replace four pollers and every snapshot with a single org event stream the whole shell subscribes to

- **Severity**: High
- **Category**: live-surface
- **File**: `console/src/lib/api.ts:79` (`call()` — one-shot `fetch`, `cache: "no-store"`,
  15 s timeout; there is no streaming primitive in the client at all),
  `console/src/components/NavDashboard.tsx:78` (30 s poll), `console/src/components/NavStatus.tsx:37`
  (a second one), `console/app/console/modules/projects/Projects.tsx:45` (a third, 10 s),
  `console/app/console/modules/ingest/useIngestFeed.ts:6` (a fourth, self-scheduling),
  `console/app/console/layout.tsx:16` (where the provider mounts),
  `console/app/api/status/route.ts` (the token-safe proxy pattern the stream copies).

- **Scenario**: An operator leaves the console open on a second monitor. A source lands,
  the pipeline extracts, a promotion appears in the rail *while they are looking at it* —
  the row slides in with its accent, the to-review count in the chrome ticks up without a
  reload, and the item they are currently signing shows that a colleague has it open, so
  nobody double-adjudicates. On the analytics wall, retrieval traffic pulses as agents
  across the org actually pull knowledge: this project asked for a Rust standard, that
  session got a canonical answer, this query came back empty. The interference field the
  landing page draws (`src/home/Home.tsx:121`) stops being an animation about the product
  and starts being a rendering of the org's actual field — which is the single most
  compelling artifact this product could put on a screen in front of a CTO. Governance
  becomes a control room rather than a series of stale photographs.

- **Root cause**: The shell has no concept of *time passing*. `api.ts` exposes exactly one
  verb — request/response, no-store, per-render — and every module is therefore a
  snapshot taken at page load; there is no `subscribe`, no `EventSource`, no reader over a
  `ReadableStream` anywhere in `console/`. The server side is symmetric: `grep` for
  `event-stream` / `Sse` across all 8 crates returns **nothing**. So the only way anything
  freshens is a component growing its own interval, which four of them independently did
  — four sockets, four dialects, four sets of failure semantics, none of them able to tell
  a module below the chrome that its data just went stale. The ceiling this sets: a
  memory server whose writers (agents) run continuously can only ever be *observed
  discontinuously*. No notifications, no presence, no collaboration safety, no wall
  display, no live demo — and no way to show the demand side at all, because
  `memory_search` leaves no trace anywhere (`library_usage_events` records standard/skill
  fetches and `document_reads` records page reads, but memory retrieval — the product's
  main event — is unrecorded, so even a poll would have nothing to poll).

- **Impact**: Collapses N pollers into 1 connection and, more importantly, changes what
  the console *is*. Governance latency drops from "next time a human loads the page" to
  seconds. Double-adjudication of the same promotion — currently possible and invisible —
  becomes impossible. It makes the shipped-but-unwitnessed machinery visible for the
  first time: the `pipeline_runs` ledger, the per-org fairness worker, the sweep schedules
  and the compose/recompile loop all currently do their work where no one can see it.
  And it opens the demand-side story that no dashboard in the product tells today —
  answer rate, empty-result queries as a ranked ingest backlog, which memories are
  actually earning their keep — which is the argument that turns "we store knowledge"
  into "here is the knowledge our agents used this week".

- **Fix sketch**:
  1. **Backend, thinnest possible.** `GET /v1/events` in `brainiac-server` as SSE over an
     in-process broadcast channel, with `Last-Event-ID` resume from a monotonically-keyed
     tail. Do not invent an event bus: publish from the write paths that already exist
     (promotion create/decide, contradiction open/resolve, `pipeline_runs` transitions,
     `document_revisions` compose, `library_usage_events` insert) — one `emit()` call each.
     Postgres `LISTEN/NOTIFY` is the multi-process upgrade later; a single-process
     broadcast proves the shape now.
  2. **Shell, the actual moonshot.** Add `subscribe(cfg, since?) → AsyncIterable<Event>`
     beside `call()` in `src/lib/api.ts`, and — because the bearer token must never reach
     the browser (`api.ts:1`) — a passthrough route handler `app/api/events/route.ts`
     mirroring how `app/api/status/route.ts` already resolves the token server-side.
     Mount one `<EventStream>` client provider in `app/console/layout.tsx` next to
     `<Chrome/>`, owning reconnect/backoff and an offline state that reuses the existing
     `ApiOffline`/`DemoBanner` honesty contract (a stale stream must *say* it is stale —
     that rule is already the shell's law).
  3. **First consumers, in order.** `NavDashboard` drops its interval and derives counts
     from the stream (proves resume + offline). Then a shell-level
     `useRefreshOn(kinds[])` that calls `router.refresh()` when an event touches the open
     module — every server-rendered module becomes live with one hook and no rewrite;
     delete the `Projects` and `useIngestFeed` pollers. Then presence: publish
     "viewing/claimed" for an address (finding 1's `sel`) and render it on the reviews
     rail so two maintainers cannot collide.
  4. **The demand feed.** A `retrieval_events` emit at the `memory_search` /
     `memory_context` MCP handlers (`mcp.rs:675`) — org, project, k, top score, hit-or-miss,
     team, never a person, matching the `library_usage_events` no-leaderboard rule already
     set in migration `0028`. That stream is what makes the live field worth watching, and
     its miss list is the highest-signal ingest backlog the product could offer.
  Smallest proof: `/v1/events` emitting promotion-created, `NavDashboard` counting from it,
  interval deleted.
