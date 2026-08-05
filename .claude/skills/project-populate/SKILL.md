---
name: project-populate
category: Maintenance
memory: vault
description: Populate a newly managed repository with the data Personas needs to maintain and develop it — a context map, a feature (use-case) inventory, a triaged KPI set, and optionally simulated KPI data for a product that has not shipped yet. Contexts and features are assigned autonomously; KPIs are negotiated with the operator wave by wave. Scopeable — run all four lanes or just the ones you name. Dispatched by the passport wall, or run standalone with /project-populate.
---

# Project Populate

A repository that Personas has just adopted is *registered* but not yet
*understood*. It has no context map, so nothing can be scoped; no feature
inventory, so nothing can be planned against; and no KPIs, so nothing can be
measured. Every later capability — scoped dispatch, the improve plan, the Ship
roadmap, KPI simulation — reads from those three tables.

Your job is to fill them, in that order, because each one is built on the one
before it: features are slices through the context map, and a KPI worth
tracking is a KPI attached to a feature someone cares about.

**You do not do the scanning yourself.** The app owns three scan lanes and
they already encode the doctrine — how a context is shaped, what makes a good
use case, which KPIs are measurable in this stack. You conduct them over the
app's loopback bridge, gate them on freshness so a re-run is cheap, and spend
your own turns on the part no lane can do: deciding with the operator which
KPIs are worth adopting, and wiring them to whatever monitoring the project
already has.

## The vault — durable state across sessions

A KPI sweep over a large codebase does not fit in one session, so the loop
remembers. Resolve the vault root (first hit wins) and use
`$VAULT/ProjectPopulate/<project-slug>/`:

```
VAULT="C:/Users/kazda/Documents/Obsidian/personas"
# Portable fallback if no Obsidian vault exists: <repo>/.project-populate/
```

```
ProjectPopulate/<project>/
  Sweep.md                 # the index: covered / remaining counts, last session, next batch
  contexts/<slug>.md       # one note per context TOUCHED — verdict, KPIs kept, why zero
  Decisions.md             # append-only log of adopted/rejected with the reason
```

**Read `Sweep.md` first, every invocation.** It decides which contexts this
session takes. A context with a note is done — never re-scan it unless the
operator asks or its files changed materially.

Three rules, learned the hard way in this vault: it is **not version-controlled
and Obsidian's file recovery never sees agent writes**, so a clobbered note is
gone.

1. Re-read a note immediately before writing it; never write from a stale read.
2. Append rather than rewrite whole files. Only ever replace a section you own.
3. Update `Sweep.md` after **each context**, not at session end — a killed
   session must lose nothing.

If the operator says a context is "done", that is a fact about their intent, not
a guarantee the note still matches what you read earlier. Re-read.

## Scope — run only the lanes you were asked for

There are four lanes: **contexts**, **features**, **kpis**, **simulation**.

The dispatch brief names the scope; standalone, `--scope` does (comma-separated
lane names, e.g. `/project-populate --scope kpis,simulation`). No scope given
means contexts + features + kpis, with simulation offered at the end.

A lane outside the scope is **not run and not gated** — say it was out of scope
in the final report and spend no turns on it. Dependencies still hold: features
need a context map to slice, and simulation needs adopted KPIs. If a scope asks
for a lane whose input is missing, say what is missing rather than quietly
widening the scope to fix it.

## Modes

**Dispatched (passport wall).** The prompt carries a DISPATCH BRIEF — project
id, repo root, the bridge port, the scope, per-lane freshness verdicts already
computed by the app, and the project's available connector metadata. Trust the
brief's verdicts; they were derived from the same database you would be
querying.

**Standalone (`/project-populate [--scope …]` in a repo).** No brief. Derive
everything yourself per [`references/bridge.md`](references/bridge.md): find the
bridge port, resolve this repo's `project_id` from `GET /dev-tools/projects` by
matching `root_path`, and compute the freshness gates from
`/context-groups/{id}`, `/use-cases/{id}` and `/kpis/{id}`. If the repo is not
registered in Personas at all, say so and stop — registering it is the
operator's call, not yours.

## Before anything else

The bridge is the app. If it does not answer, nothing in this skill works:

1. Confirm the bridge responds (`GET /dev-tools/projects`). If it does not,
   tell the operator Personas is not running and stop. Do not fall back to
   editing the database or writing files that mimic scan output — the app's
   ingest path is the only door, and inventing another one silently corrupts
   the data model.
2. Confirm the `project_id` resolves to this repo's root. A mismatch means you
   would populate the wrong project.

## Phase 1 — Context map

**Gate.** In dispatched mode the brief states the verdict. Standalone, derive it
from `GET /dev-tools/context-groups/{project_id}`: no groups → `full`; newest
`updated_at` older than 14 days → `incremental`; otherwise → `skip`. Do NOT use
`context-map.json`'s file mtime as a proxy — any checkout or merge rewrites it,
so it reports a fresh scan that never happened.

- `full` — see **Sweep, do not single-shot** below.
- `incremental` — `POST /dev-tools/scan-codebase` with `delta_mode: true`.
- `skip` — say what the map already holds (group and context counts, age) and
  move on. Do not rescan a fresh map to feel thorough; it costs the operator
  tokens and can only churn a map that was already right.

Poll `GET /dev-tools/scan-status/{scan_id}` until the status leaves `running`.
These scans take minutes — poll at a human pace (~10s), and relay the
milestone lines as they arrive so the operator can see progress rather than a
frozen cursor.

### Sweep, do not single-shot

**A whole-tree scan silently under-maps anything non-trivial.** Contexts reach
the database only as protocol messages parsed from ONE session's stdout, so that
session must emit the entire map; on a large repo it runs out of room and stops,
and the result looks exactly like success. Measured on a ~4,400-file repo: one
whole-tree pass mapped **9%** and reported completion; the same tree swept
subtree-by-subtree reached **~89%**.

So for a `full` verdict on anything beyond a few hundred files:

1. Partition into subtrees of roughly **50-500 source files** (top-level
   feature/module directories are usually the seam). Show the operator the
   partition before spending tokens on it.
2. Run **3-4 concurrently** via `subtree` on `/dev-tools/scan-codebase` — the
   guard is per-scope, so they do not block each other.
3. **Read the `[Coverage]` line on every scan.** Below ~90% means that subtree
   did not finish; split it and re-run. Slightly over 100% is normal.
4. Consolidate group sprawl at the end with explicit merge pairs, and run the
   idempotent repair routes once.
5. **Then `POST /dev-tools/export-context-map`.** The repair routes mutate the
   database without touching the exported artifacts, so the consolidation step
   above is precisely what leaves `context-map.json` and the generated CLAUDE.md
   block describing a map that no longer exists — and that block tells every
   agent in the repo to scope its edits by that file.

Launching a scan whose `scan_id` you did not capture is not a reason to relaunch
it: `GET /dev-tools/scans/{project_id}` lists every known scan with its subtree
and status. Relaunching pays twice and maps one tree twice.

Full command reference and the failure modes in
[`references/bridge.md`](references/bridge.md).

Two rules learned the hard way: **do not edit the app's Rust while a sweep is
running** (the dev watcher restarts the app and kills in-flight scans), and
**verify in aggregate** — count distinct mapped paths across all contexts
against the repo's real file count, because several bugs in this pipeline were
invisible per-scan and obvious only in the total.

The scan lane fans out over subagents internally. You do not need to explore
the codebase to help it, and you should not: two agents mapping the same tree
produce two different maps, and only the lane's output reaches the database.

If the scan fails, report the error verbatim and stop the phase. A failed
context map makes Phase 2 meaningless, so do not proceed past it.

## Phase 2 — Feature inventory

Features are slices through the context map, so this phase runs **only after**
Phase 1 has a map. If Phase 1 ended in `skip`, the map is already there and
this phase proceeds normally.

**Gate.** Same 14-day rule against `GET /dev-tools/use-cases/{project_id}`: none
→ scan; all older than 14 days → scan; otherwise skip. There is no delta mode
here — a use-case scan is always a fresh proposal pass.

`POST /dev-tools/scan-use-cases`, then poll
`GET /dev-tools/use-case-scan-status/{scan_id}`.

Two failures are expected rather than exceptional, and both are informative
rather than fatal — report them and move to Phase 3:

- *"Scan the codebase into a context map first"* — Phase 1 skipped or failed.
- *"N proposals already await review"* — the operator has an unreviewed queue.
  Point them at Projects → Factory → Overview and move on. Do not triage use
  cases yourself; unlike KPIs, they have a review surface in the app and it is
  the better place to do it.

## Phase 3 — KPIs, with the operator

This is the phase that needs a human, and the reason both dispatch transports
are interactive.

### Which shape: project pass or context sweep

**Project pass** (`POST /dev-tools/scan-kpis` with just `project_id`) proposes
up to 8 KPIs across the whole product. Right for the handful of KPIs that are
genuinely global — activation, onboarding speed, overall reliability. Run it
**once**, early.

**Context sweep** (the same route with `context_id`) proposes up to 4 KPIs for
ONE subsystem, and is how a codebase becomes navigable per module. Right for
everything after the global pass. This is the mode that needs the vault, because
a project with hundreds of contexts takes many sessions.

Do the project pass first if no KPIs exist at all; otherwise go straight to the
sweep.

### The sweep

1. **Verify the map is current before spending anything.** Fetch
   `GET /dev-tools/contexts/{project_id}` and compare against
   `context-map.json` at the repo root. If the counts disagree materially, the
   database is stale — say so and offer an incremental context scan first. A
   sweep over a stale list wastes every session it runs.
2. **Read `Sweep.md`** for what is already covered.
3. **Rank the uncovered contexts** and take the next batch (default 5 per
   session; the operator can say otherwise). Rank by what would matter if it
   broke: number of files owned, whether the context sits on a user-facing path,
   whether it already carries findings or errors. Say the ranking out loud so
   the operator can reorder before you spend tokens.
4. **For each context in the batch:** `POST /dev-tools/scan-kpis` with its
   `context_id`, poll to completion, then triage.
5. **Write the context note and update `Sweep.md` before moving to the next
   one.** Never batch the bookkeeping to the end.

### Review mode — manual, or earned autonomy

Before the sweep starts, ask the operator once which review mode this run uses:

- **`manual`** (default) — every context's proposals go through the operator,
  as described below. Always offer this first; adoption is theirs.
- **`auto`** — you triage on your own recommendation, recording decisions with
  the same per-KPI immediacy. Only enter this mode when the operator names it.
- **`calibrated`** — the middle path, and the one to suggest for a large map:
  run the first **20 contexts** manual while tracking, per proposal, whether
  the operator's decision matched the recommendation you stated BEFORE they
  answered. At 20 contexts, report the agreement rate. **At ≥90%, offer to
  switch to `auto` for the remainder** — and switch only when they accept.
  Below 90%, stay manual and say which kinds of proposals you misjudged, so
  the disagreement pattern becomes part of the record.

Track calibration in the vault note (`Sweep.md`): one line per triaged context
— `recommended / operator kept / match`. The count is honest only if the
recommendation was committed before the answer; never restate a
recommendation after the fact to improve the score. In `auto` mode, keep
writing the same lines with `auto` in place of the operator column, so a later
session (or the operator) can audit what autonomy actually decided — and drop
back to manual the moment the operator asks or a whole batch smells wrong.

### Triage per context — pick 0 to 4

Each context scan returns at most 4 proposals. Present them as ONE multi-select:
the operator keeps any subset, **including none**.

**Zero is the expected answer for most contexts**, and recording it matters as
much as an adoption — a context noted as "no KPI, it is a shared type module"
is a context the sweep never pays for again. Never nudge toward adopting
something to make a context look covered.

For each kept KPI, `POST /dev-tools/kpi-decision` with `active` immediately,
one call per KPI. Anything not kept goes `archived` in the same pass so the
context's queue is empty and the sweep can move on.

**When you adopt a KPI whose measurement the grounding proved wrong, fix it in
the same pass** — `POST /dev-tools/kpi-update` with a corrected `description` /
`measure_kind` / `measure_config` / `baseline_value`. This is common rather than
exceptional: the lane proposes a sound pillar attached to a column that does not
exist, or a `connector` naming a service the project has never integrated.
Adopting the pillar and leaving the instructions makes the row's only record of
how to measure it a fiction, and the next session re-derives what you already
verified. The vault note is for the reasoning; the row should carry the truth.

Write the note:

```markdown
# <context name>
group: <group> · files: <n> · scanned: <date>
verdict: <adopted N | none — reason>

## Kept
- <KPI name> — target <n><unit> — <one line on why>

## Rejected
- <KPI name> — <the operator's reason, in their words where you have them>
```

### Judge on value first, measurability second

The most common failure of a KPI set is not that the numbers are wrong — it is
that they are numbers about the *repository* (coverage, bundle size, compiler
errors) rather than about whether the *product works for the person using it*.
Those pass every measurability check and steer nothing.

So apply two filters, in this order:

1. **Would this number change what the team works on?** A metric already
   enforced by a CI gate cannot; it can only restate a rule. Say so and
   recommend rejection, however cleanly it measures.
2. **Can it be measured, and how?** Only now. And note the asymmetry: a *value*
   KPI that cannot be measured today is often still worth adopting — the pillar
   has to be named before anything can steer toward it, and Phase 5 can put
   simulated numbers against it. A *technical* KPI that cannot be measured today
   is just noise.

If a whole batch comes back as repository metrics, say that plainly and offer to
reject it and re-scan rather than walking the operator through five variations
of the same mistake.

### Triage in waves (project pass only)

Fetch the proposals (`GET /dev-tools/kpis/{project_id}?status=proposed`) and
walk them **four at a time** — a wave has to fit whatever select surface the
session is driving, and four is the safe ceiling. For each wave:

1. Present the five compactly — name, what it measures, the proposed target,
   and the one thing that matters most: **how it would actually be measured in
   this repo**. A *technical* KPI whose measurement you cannot describe
   concretely should be rejected, and you should say so rather than let it pass.
   For a value KPI, say instead what it would take to measure it — a connector,
   a launch, or a simulated journey.
2. Ask for a decision per KPI: **adopt** · **adopt with a different target** ·
   **reject** · **defer**. Recommend one, and say why in a clause, not a
   paragraph.
3. Record each answer immediately via `POST /dev-tools/kpi-decision` —
   `active`, `archived`, or `paused`, plus `target_value` when the operator
   adjusted it. One call per KPI as the answer arrives, never a batch at the
   end: a run interrupted mid-wave then leaves every answered proposal
   correctly filed instead of losing the whole wave.
4. Move to the next wave. Do not re-litigate a decided KPI.

Waves exist because a list of twenty proposals gets rubber-stamped and a list
of five gets read. If the operator starts answering "yes to all", that is a
signal the proposals are too vague to judge — say so, and offer to reject the
batch and re-scan rather than banking agreement nobody examined.

## Phase 4 — Wire the adopted KPIs to real monitoring

A KPI with no measurement path is a number someone has to remember to update
by hand, which means it stops being true within a month.

Read the connector metadata in the brief (standalone: ask the operator what
the project already uses). If the project has monitoring, error tracking, CI,
or analytics available, then for each adopted KPI that could be fed by one:

- Name the **exact** binding — which connector, which KPI, what it would
  measure. Never "you could wire this up to Sentry"; say which credential and
  which metric.
- Where the wiring is a repo change you can make (a metric emission, a CI step
  that records a number, a query the project can run), propose it as concrete
  work and execute what the operator accepts.
- Where the wiring is a credential binding inside Personas, name it as a
  follow-up for the operator to do on the passport wall. You cannot bind
  credentials from here and should not pretend otherwise.

If the project has no monitoring connectors at all, say that plainly and note
which single one would unlock the most adopted KPIs. One specific
recommendation beats a survey.

## Phase 5 — Simulated data for a product that has not shipped

A pre-production project has the honest problem this whole skill exists around:
you need pillars to know what to measure, but there is no traffic to measure. A
target invented to fill the field is worse than an empty one, because it looks
like evidence.

The simulation lane answers it — representative characters walk the KPI-bound
journeys over the code, producing values tagged as the simulation they are
(`local`/`test`, never `production`), plus target proposals researched from
comparable products with citations.

**Offer this when** any adopted KPI is a value/outcome pillar with no
measurement path today. **Skip it** when every adopted KPI already measures
cleanly, or when `simulation` is outside the scope — and say which.

The sequence matters and is easy to get wrong:

1. **Adopt first.** The simulation only measures adopted KPIs and deliberately
   ignores `proposed` ones. Running it before Phase 3's triage produces nothing.
2. `POST /dev-tools/kpi-sim/prepare` `{project_id}` — writes
   `kpi-sim/snapshot.json`. The sim refuses to run without it and only the app
   may produce it, which is why this is a route and not something you write.
3. **Tell the operator what it will cost before running it** — it walks journeys
   with several characters and researches comparable products, so it is the
   longest phase here. Get a yes.
4. Run `/kpi-sim run` (add `--l2` only if the operator wants the live app driven
   as well). It writes `kpi-sim/runs/<id>/result.json`.
5. `POST /dev-tools/kpi-sim/ingest` `{project_id}` — validated and idempotent;
   a run dir is marked once ingested and refused on a second attempt.
6. Report what landed: simulated measurements, target proposals, findings. Say
   **out loud** that the measurements are simulated and which environment they
   are tagged as. A simulated number reported as a real one is the single worst
   outcome available in this skill.

If the sim declines to measure something, that is a correct answer, not a
failure — real traffic and revenue cannot be simulated, only researched into a
target proposal.

## Using subagents

Phases 1-3 are conducted, not performed — the lanes do the heavy reading, and
duplicating their work fights them. Use subagents for **your** reading, where
one exists:

- Phase 3 grounding: before presenting a wave, dispatch subagents to establish
  how each proposed KPI would really be measured in this repo — one per KPI,
  in a single message so they run concurrently. This is what turns "Test
  coverage: 80%" into "there is no coverage reporter configured; adopting this
  means adding one first".
- Phase 4: one subagent per candidate connector to find the integration points
  that already exist in the repo.

Do not fan out for the sake of it. A subagent that reports what you already
know costs a turn and buys nothing.

## Hard rules

- **Never write to the database directly.** The bridge is the only door. If a
  route you need does not exist, say so and stop — do not route around it.
- **Never invent a scan result.** If a lane fails, the phase failed. Reporting
  a plausible context map you assembled yourself would poison every downstream
  feature that trusts the table.
- **Never decide a KPI for the operator.** Recommend, then wait. Adoption is
  the one thing in this skill that is theirs.
- **Report skipped phases as skipped.** A run that skipped two gates because
  the data was fresh is a *good* run. Do not dress it up as work, and do not
  hide it. Same for a lane left out of the scope.
- **Never present a simulated number as a measured one.** Every value that came
  from the simulation lane carries its environment tag into every sentence you
  write about it.

## Coordination — active-runs ledger

If the target repo keeps `.claude/active-runs.md`, register at start and move
your entry to `## Recently completed` at the end. Declared paths: whatever
Phase 4 executes (nothing, in the common case). Phases 1-3 write only through
the app.

## Final report

End with a short, honest summary:

- Scope: which lanes were in it.
- Context map: scanned (full / incremental), skipped-as-fresh, or out of scope,
  with counts.
- Features: scanned, skipped, or out of scope, with the proposal count and where
  to review.
- KPIs: how many proposed, adopted, adjusted, rejected, deferred — and how many
  of the adopted ones can actually be measured today.
- Simulation: run or not, and if run, what landed and that it is simulated.
- Monitoring: what got wired, and the single most valuable thing still unwired.
- What a human should do next, if anything.

When a sweep ran, also say **where it stopped**: contexts covered this session,
contexts remaining, and what the next batch would be. That sentence is what
makes the next session cheap to start.

Then the machine-readable line, on its own, as the last line of your final
message. Use `out_of_scope` for a lane the scope excluded and `unknown` for a
gate you could not determine — never guess a value to fill the field:

```
PROJECT_POPULATE_RESULT: contexts=<full|incremental|skipped|failed|out_of_scope|unknown> features=<scanned|skipped|failed|out_of_scope|unknown> kpis_adopted=<n> kpis_rejected=<n> kpis_deferred=<n> simulation=<run|declined|skipped|out_of_scope> wired=<n> swept=<contexts covered this session> remaining=<uncovered contexts>
```
