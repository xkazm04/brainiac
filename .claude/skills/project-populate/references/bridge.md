# The `/dev-tools` bridge

Personas exposes its scan lanes over an HTTP server bound to `127.0.0.1`. This
is how a terminal session drives the app's own machinery instead of
reimplementing it.

Everything below writes through the same repository functions the UI calls, so
work done here is indistinguishable from work done by clicking.

## Finding the port

The server takes **the first free port at or above 17400**, scanning up to 16
ports. It is therefore not a constant: a restart while another process holds
17400 moves it. A dispatch brief names the port that was live when the
dispatch was composed — start there, and if it does not answer, probe upward:

```bash
for p in $(seq 17400 17415); do
  curl -s -m 1 "http://127.0.0.1:$p/dev-tools/projects" >/dev/null && echo "$p" && break
done
```

No port answers → **Personas is not running.** Stop and say so. There is no
offline path.

## Finding the project

```bash
curl -s "http://127.0.0.1:$PORT/dev-tools/projects"
```

Returns every registered project. Match on `root_path` against the repo you
are in — do not match on name, which is not unique. The `id` field is the
`project_id` every other route wants.

If no project matches this repo, it has not been registered in Personas. That
is the operator's decision to make (`POST /projects` exists, but registering a
repo behind their back is not this skill's job).

## Routes

### Context map

```bash
# whole-tree — delta_mode false = full re-exploration, true = incremental
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/scan-codebase" \
  -H 'Content-Type: application/json' \
  -d '{"project_id":"<id>","root_path":".","delta_mode":false}'

# SUBTREE-scoped — the mode that actually maps a large codebase
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/scan-codebase" \
  -H 'Content-Type: application/json' \
  -d '{"project_id":"<id>","root_path":".","subtree":"src/features/agents"}'

curl -s "http://127.0.0.1:$PORT/dev-tools/scan-status/<scan_id>"
# → {"scan_id":..., "status":"running|completed|failed|not_found", "error":..., "lines":[...]}
```

`lines` carries the lane's milestone output. Relay it while polling — these
scans run for minutes and silence reads as a hang.

### Use the subtree sweep on anything non-trivial

**A whole-tree scan does not scale, and it fails silently.** Contexts reach the
database only as protocol messages parsed from ONE session's stdout, so that
session has to emit the entire map. On a large repo it runs out of room and
stops, and a scan that returns 49 valid contexts looks exactly like a scan that
finished. Measured on the personas repo: one whole-tree pass mapped 392 of
~4,400 hand-written files (**9%**) and reported success; sweeping the same tree
subtree-by-subtree reached **~89%**.

The sweep:

1. Partition the repo into subtrees of roughly **50-500 source files** each —
   top-level feature/module directories are usually the right seam. A 460-file
   subtree fits one session comfortably; the ceiling is the whole tree, not a
   big directory. Split anything much past ~600.
2. Launch **3-4 concurrently**. The single-flight guard is keyed per scope, so
   different subtrees never block each other (a second scan of the SAME subtree
   is still refused, correctly).
3. Poll each to completion, then start the next batch.

**Read the `[Coverage]` line on every scan.** It reports
`Mapped N of M source files in <scope> (P%)` and warns below two thirds. Below
~90% means that subtree did not finish and should be split smaller. Slightly
over 100% is normal — the denominator walks the filesystem while your own
comparison probably uses `git ls-files`, so untracked-but-real files land in the
numerator.

**Which files a context may own.** The ingest filter accepts hand-written CODE:
`ts tsx js jsx mjs cjs py go java kt swift c cpp cc h hpp cs rb php scala lua ex
exs vue svelte sql css scss` (case-insensitive). It rejects generated/vendored
trees (`node_modules`, `target`, `dist`, `build`, `bindings`, `locales`,
`section-locales`, `coverage`, `__pycache__`, `.venv`, `venv`, `.tox`,
`site-packages`, any dot-directory) and data/doc formats (`json`, `md`, `toml`,
`yaml`) — claiming those maps description rather than implementation, and locale
JSON once produced 15 `section-locales-*` contexts and a 5871% coverage line.

If a scan reports `[Skipped] Context: <name> — every path was generated or
non-source` for contexts you believe are real code, check the file's extension
against that list before assuming the model misbehaved. Until 2026-07-30 the list
held only 5 extensions, so a Python-majority subsystem produced correct contexts
that were all silently discarded while the scan still reported success.

**Keep the partition disjoint — `subtree` is a path PREFIX.** Scanning
`src/lib` covers `src/lib/db` too. So never scan a parent after scanning its
children: a scoped scan retires the existing contexts inside its scope and
replaces them, so the parent pass would delete the fine-grained child contexts
and substitute one coarser map. Prefer a single slightly-oversized scan over a
parent/child pair. Where splitting leaves a leftover tail (loose files and small
sibling dirs under an already-split parent), scan those directories
individually or leave them unmapped and say so — do not reach for the parent
prefix to sweep them up.

**Do not edit the app's Rust while a sweep runs.** The dev watcher rebuilds and
restarts the app, which kills every in-flight scan (job state is in-memory) and
leaves partial data with no completion signal. Treat a sweep as a code-freeze
window.

### Repairing a map

All four are idempotent, so running them on a clean map is free:

```bash
# same-named context rows (older writes could duplicate them)
curl -s -X POST "$B/dev-tools/dedupe-contexts"          -d '{"project_id":"<id>"}' -H 'Content-Type: application/json'
# same-named groups (concurrent scans could each create one)
curl -s -X POST "$B/dev-tools/dedupe-context-groups"    -d '{"project_id":"<id>"}' -H 'Content-Type: application/json'
# strip generated / locale / non-source paths, dropping contexts left empty
curl -s -X POST "$B/dev-tools/prune-nonsource-contexts" -d '{"project_id":"<id>"}' -H 'Content-Type: application/json'
# merge semantically-overlapping groups — pairs are EXPLICIT, never inferred
curl -s -X POST "$B/dev-tools/merge-context-groups" -H 'Content-Type: application/json' \
  -d '{"project_id":"<id>","delete_empty":true,
       "merges":[{"from":"Execution & Quality Data","into":"Execution Engine"}]}'
# delete contexts by EXPLICIT id (NOT idempotent in effect — rows are gone).
# For rows no heuristic can pick: e.g. the old coarse map's husks after a
# subtree sweep claimed their files. Ids not owned by the project come back
# in rejected_ids instead of being skipped silently.
curl -s -X POST "$B/dev-tools/retire-contexts" -H 'Content-Type: application/json' \
  -d '{"project_id":"<id>","context_ids":["<ctx>", "..."]}'
```

**Every repair above mutates the DB and leaves the EXPORTED map stale.** The
export only ran at the end of a scan, so the documented good practice — sweep,
then consolidate group sprawl — is exactly what desynchronizes the artifacts.
Re-export from current DB state when the repairs are done:

```bash
curl -s -X POST "$B/dev-tools/export-context-map" -H 'Content-Type: application/json' \
  -d '{"project_id":"<id>"}'          # root_path optional, defaults to the project's
# → {"project_id":..., "root_path":..., "contexts": N}
```

This rewrites `context-map.json` AND the `<!-- personas:context-map -->` block in
the project's CLAUDE.md. It matters more than a stale build artifact normally
would: that generated block instructs every agent working in the repo to read
`context-map.json` and scope its edits to the matching context, so drift is read
as ground truth by the next agent and can point at groups that no longer exist.
Measured on a real sweep: the file said 236 contexts / 34 groups while the DB held
233 / 25.

**Before retiring a context, check what points at it.** `dev_kpis.context_id`
is ON DELETE SET NULL, so deleting a context strands its adopted KPIs as
unbound project-level rows. Re-point them FIRST:

```bash
curl -s -X POST "$B/dev-tools/kpi-rebind" -H 'Content-Type: application/json' \
  -d '{"kpi_id":"<kpi>","context_id":"<new ctx>"}'
```

The natural successor for a KPI is whichever new context now owns the old
context's files. And if a doomed context's files have NO new owner, that
context is not superseded — keep it rather than retiring on age alone.

Expect **group sprawl** after a sweep: each scan sees the existing group list
and is told to reuse it, but a large sweep still tends to add some. Consolidate
at the END with explicit merge pairs — the overlaps are semantic ("Execution
Engine" vs "Execution & Quality Data") and no string rule separates those from
groups that genuinely differ.

### Reading the map back

```bash
curl -s "$B/dev-tools/contexts/<project_id>"        # every context + file_paths
curl -s "$B/dev-tools/context-groups/<project_id>"  # the group taxonomy
curl -s "$B/dev-tools/use-cases/<project_id>"       # the feature inventory
```

Verify a sweep by counting DISTINCT paths across all contexts and comparing
against the repo's real source-file count. Do not trust a per-scan number alone;
several of this pipeline's bugs were visible only in the aggregate.

### Features (use cases)

```bash
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/scan-use-cases" \
  -H 'Content-Type: application/json' -d '{"project_id":"<id>"}'
# → {"scan_id":"..."}   or 500 with the reason

curl -s "http://127.0.0.1:$PORT/dev-tools/use-case-scan-status/<scan_id>"
```

Two 500s are expected rather than exceptional:

- *"Scan the codebase into a context map first"* — there is no map to slice.
- *"N proposals already await review (cap …)"* — an unreviewed queue exists.
  Send the operator to Projects → Factory → Overview; that surface is built
  for reviewing use cases and this session is not.

### KPIs

```bash
# project-wide pass — up to 8 KPIs across the whole product
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/scan-kpis" \
  -H 'Content-Type: application/json' -d '{"project_id":"<id>"}'

# context-scoped pass — up to 4 KPIs, all bound to that one context
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/scan-kpis" \
  -H 'Content-Type: application/json' \
  -d '{"project_id":"<id>","context_id":"<ctx>"}'

curl -s "http://127.0.0.1:$PORT/dev-tools/kpi-scan-status/<scan_id>"

# the sweep's worklist — every context, with file_paths for ranking
curl -s "http://127.0.0.1:$PORT/dev-tools/contexts/<project_id>"

# read (omit ?status= for all statuses)
curl -s "http://127.0.0.1:$PORT/dev-tools/kpis/<project_id>?status=proposed"

# decide — one call per KPI, as each answer arrives
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/kpi-decision" \
  -H 'Content-Type: application/json' \
  -d '{"kpi_id":"<id>","status":"active","target_value":95}'
```

`status` is one of `active` (adopted), `archived` (rejected), `paused`
(deferred), `proposed` (back to the queue). `target_value` is optional and
applied in the same write, so "adopt, but at 95 not 99" is one call.

**Backpressure is per-scope.** A project pass is refused while the whole review
queue is at its cap; a context pass is refused only while THAT context has 4
untriaged proposals. So one unreviewed subsystem never blocks a sweep across
the rest — but a context you scanned and walked away from will refuse a rescan
until its queue is cleared.

Anything else is a 400. `kpi-decision` accepts nothing but a status and a target
BY DESIGN — it is the triage verb, and one call per answer means an interrupted
run leaves every answered proposal correctly filed.

### Correcting a KPI's definition

Triage decides *whether* to keep a metric. It cannot fix a metric whose
**measurement instructions are wrong**, and the scan lane produces those often:
a sound pillar paired with a column name that does not exist, or
`measure_kind: "connector"` naming a service the project has never integrated.
Adopt-and-leave-it writes those false instructions into the row as the only
record of how to measure the thing.

```bash
curl -s -X POST "$B/dev-tools/kpi-update" -H 'Content-Type: application/json' \
  -d '{"kpi_id":"<id>",
       "description":"…what it measures, corrected…",
       "measure_kind":"codebase",
       "measure_config":{"cmd":"…","parse":"regex:(\\d+)"},
       "baseline_value":25}'
```

Every field is optional and omitted fields are untouched, so fixing one sentence
cannot blank the rest. Accepts: `name`, `description`, `category`,
`measure_kind`, `measure_config` (any JSON), `unit`, `direction`, `cadence`,
`tier`, `baseline_value`, `needed_connector`. Enum values are validated up front
and return 400 with the allowed set rather than an opaque 500 from a SQL CHECK.
A body with no updatable field is a 400, not a silent no-op — a mistyped field
name must not read as "saved". `rationale` is NOT settable (the shared repo
function has no parameter for it); put corrected instructions in `measure_config`.

Still not here on purpose: renaming a KPI's *identity* across surfaces, and
deleting one. Both belong in the app's editor where the operator can see what
references it.

### Before relaunching a scan, ask what is already running

```bash
curl -s "$B/dev-tools/scans/<project_id>"
# → {"project_id":…, "running": 2,
#    "scans":[{"scan_id":…, "status":"running", "subtree":"app/_lib", "lines": 41},
#             {"scan_id":…, "status":"completed", "subtree":null,      "lines": 124}]}
```

A `scan_id` used to be the only handle on a scan: lose it and the scan became
unobservable, so the safest move looked like relaunching — scanning the same
subtree twice at full token cost and producing two maps of one tree. This is the
cheap check. `subtree: null` means a whole-tree scan; the field matches the
single-flight scope key, so compare it against what you are about to launch.

The registry is in memory and entries evict 30 minutes after finishing, so this
answers "what is happening now", not "what ever happened".

**Where ids get lost, concretely:** a `curl` inside `while read … done < file`
inherits the loop's stdin and swallows the remaining lines, so the POSTs fire but
their output vanishes. Always `curl … </dev/null` in a read loop — and if you are
unsure whether a launch landed, call this route instead of relaunching.

### Simulation

```bash
# write kpi-sim/snapshot.json (the sim refuses to run without it)
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/kpi-sim/prepare" \
  -H 'Content-Type: application/json' -d '{"project_id":"<id>"}'
# → {"snapshot_path":"...","root_path":"...","kpi_count":N}

# after /kpi-sim run has written kpi-sim/runs/<id>/result.json
curl -s -X POST "http://127.0.0.1:$PORT/dev-tools/kpi-sim/ingest" \
  -H 'Content-Type: application/json' -d '{"project_id":"<id>"}'
# → {"run_dir":"...","measurements_recorded":N,"proposals_created":N,
#    "findings_created":N,"skipped":[...]}
```

Omit `run_dir` to ingest the newest un-ingested run. `skipped` is never empty
by accident — it lists every row the validator refused, so report it rather
than treating a partial ingest as clean.

## Reading freshness

Every gate is computable over the bridge:

```bash
curl -s "http://127.0.0.1:$PORT/dev-tools/context-groups/<project_id>"   # Phase 1
curl -s "http://127.0.0.1:$PORT/dev-tools/use-cases/<project_id>"        # Phase 2
curl -s "http://127.0.0.1:$PORT/dev-tools/kpis/<project_id>"             # Phase 3
```

Compare `updated_at` against a 14-day window. **Do not substitute
`context-map.json`'s file mtime** — it is a build artifact in a git repo, so a
checkout, merge or rebase rewrites it and it will report a scan that never
happened.

Every one of these 404s when the project id matches nothing. That is
deliberate: an empty array means the project genuinely has no rows, so if you
get `[]` you can trust it rather than wondering whether you resolved the wrong
id.

In dispatched mode the gates arrive pre-computed in the brief, from the same
tables.
