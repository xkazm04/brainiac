# Moonshot scan — document-wiki

> Total: 2
> Files read: ~16

## 1. Anchor the wiki to the code: every page stamped "true as of `<sha>`", every merge a rot event

- **Severity**: Critical
- **Category**: code-truth-anchoring
- **File**: `crates/brainiac-store/src/documents.rs:449` (`mark_dirty_for_memory` — the ONLY
  automatic rot trigger) · `crates/brainiac-store/src/documents.rs:582` (`document_dependencies`
  edges are `(document_id, memory_id)` and nothing else) · `crates/brainiac-pipeline/src/compose.rs:545`
  (policy: `additive` ⇒ auto-publish) · `migrations/0039_project_path_prefix.sql` +
  `crates/brainiac-server/tests/onboard_pg.rs:433` (the longest-prefix repo→project resolver this
  needs, already built) · `crates/brainiac-server/src/docs.rs:70` (`DocSummary.dirty`) ·
  `console/src/docs/DocWiki.tsx:62` (`TABS`) · `crates/brainiac-publish/src/okf.rs`
  (`x_brainiac_stale`)

- **Scenario**: A payments engineer merges a PR that deletes the retry wrapper. Ninety seconds
  later a post-merge hook calls `brainiac code-event --remote … --sha <sha> --changed-paths …`.
  Brainiac resolves those paths to a project (the longest-`path_prefix` matcher already in
  `project_repos`), finds the four canonical memories anchored to those files, and marks the two
  pages that cite them **suspect** — not dirty. A coding agent that calls `doc_get` on either page
  gets the claim back with `suspect_since: "3 commits ago (a1b2c3d touched crates/pay/src/retry.rs)"`
  attached to the exact paragraph. The reviewer queue gains a machine-authored challenge against
  the memory, routed through the dispute triage that already exists. And every page in the
  console, in Confluence, and in the published OKF bundle carries one new line: **verified against
  `f6e2e75`**. Today no page in the product can say when it was last checked against reality; after
  this, every page says it, and says it in the vocabulary engineers already trust — a commit.

- **Root cause**: The anti-rot machinery is entirely downstream of *someone noticing*. `dirty_at`
  is set by exactly two paths: `mark_dirty_for_memory` (hooked into `set_memory_status` /
  `apply_supersession`) and the manual `mark_dirty`. Both require a human or an agent to have
  already written a superseding memory. The corpus has no edge to the artifact it describes — a
  `Memory` has `project_id` but no file, symbol, or commit; `document_dependencies` is
  memory-only; `grep -rniE 'commit_sha|verified_at|repo_path'` over `crates/` hits only the *publish*
  side (`git.rs`, `okf.rs` writing files OUT). So the single largest generator of staleness in an
  engineering org — a merge — is 100% unobserved, and worse than unobserved: a page whose claims
  the code just falsified is `dirty_at IS NULL`, which `browse_facets` counts as healthy, the
  `additive` branch of the compose policy auto-publishes, and the OKF bundle stamps
  `x_brainiac_stale: false`. The product's headline promise ("the wiki that cannot rot") is
  currently defensible only against rot the org already discovered by other means.
  A second, subtler cap: even if code drift *were* detected, `dirty` is the wrong remedy — the
  memory is what's wrong, so a recompose reproduces the identical page. Code drift needs its own
  state and its own remedy.

- **Impact**: Closes the only open edge in the truth triangle. Brainiac already covers
  memory↔memory (contradictions, supersessions) and docs↔memory (the shipped `drift` profile,
  `fixtures/v1/drift/docs.yaml`). Memory↔**code** is missing, and it is the edge where rot is
  *born*. Concretely: the propagation SLA the KB is proud of (`oldest_dirty_secs`) currently
  measures latency from *belief change* to page; this makes it measurable from *reality change*
  to page — the number a buyer actually asks about. It also flips the org's cost curve on
  re-verification: today `expire_stale_raw` and TTL reverify age memories by the **calendar**
  (30d, blind), which either re-checks facts nothing touched or misses facts that broke on day 2.
  Path-anchored suspicion re-verifies the ~2% of the corpus a given release actually endangers,
  making reverification cheap enough to run continuously instead of on a sweep nobody enables.
  And it is the only feature on this list that makes a page's freshness *falsifiable by a third
  party*: anyone with the repo can check the stamp.

- **Fix sketch**:
  1. **Anchors (the one new table).** `memory_code_anchors(memory_id, project_id, path, symbol
     NULL, first_seen_sha)`, RLS-scoped by the parent memory exactly as `document_reads` is
     (migration 0025 is the template). Populate from sources that ALREADY carry a path and need no
     model: `okf_ingest.rs` knows the doc path per concept; a session-transcript source that names
     files can have paths lifted by a regex pass, not the extraction prompt (the recall scar in
     `extract.rs` is the reason to keep the V1 prompt untouched). Anchoring is best-effort and
     additive — a memory with no anchor behaves exactly as today.
  2. **The event.** `POST /v1/projects/{id}/code-event {remote, sha, changed_paths[]}` +
     `brainiac code-event` CLI, resolving remote+path → project through the existing longest-prefix
     lookup. Cheap, deterministic, no LLM — the same trust posture as `mermaid_neighborhood`.
  3. **The state, kept separate from dirty.** `documents.code_suspect_at` +
     `documents.verified_sha`; a `suspect_documents()` work list beside `dirty_documents()`
     (`documents.rs:379`). Critically the compose worker does NOT pick it up: the remedy is
     `brainiac_store::feedback::insert(verdict = "outdated", note = "<sha> touched <path>")`
     against each anchored memory, routed into the shipped feedback→dispute triage. A merge is a
     **witness, not an authority** — the same rule `okf_ingest` already applies to harvested wikis.
     When the memory is superseded, `mark_dirty_for_memory` fires and the existing loop takes over
     unchanged. `verified_sha` advances when a code event touches anchored paths and every
     resulting challenge closes clean.
  4. **Surfaces.** `DocSummary.code_suspect` + `verified_sha`; a fourth `WikiTab` in
     `DocWiki.tsx:62` alongside `review`/`dirty`; a paragraph-level chip in `DocReader.tsx` reusing
     the `blockLifecycles` machinery (line 217) that already decorates blocks by citation; OKF
     frontmatter `x_brainiac_verified_sha`; and `publish_gate` degrades a suspect page to the
     "verification pending" stamp the health breaker already knows how to apply, rather than
     publishing it as current.
  5. **Smallest slice that proves it** (one `docs_pg` test, no model, no console): anchor a memory
     to `crates/pay/src/retry.rs`, compose+publish a page citing it, POST a code event naming that
     path, assert `code_suspect_at IS NOT NULL`, assert a `memory_feedback` row landed on that
     memory and nothing else, and assert an unrelated path leaves the page untouched. That test
     alone is the product claim.

## 2. Runbooks that execute: the page proves itself by being run, and every run is a truth test

- **Severity**: High
- **Category**: closed-loop-verification
- **File**: `crates/brainiac-core/src/types.rs:437` (`DocKind::Runbook` — a string with **zero**
  behavior; `grep -rn runbook crates/` finds it only in OKF frontmatter mapping at
  `crates/brainiac-publish/src/okf.rs:82` and in a doc comment) ·
  `crates/brainiac-pipeline/src/compose.rs:239` (`enforce_citations` — per-paragraph citation, the
  schema a step list needs, already enforced) · `crates/brainiac-store/src/documents.rs:504`
  (`record_read` — the only production signal a page emits) ·
  `crates/brainiac-server/src/mcp.rs:225` (`doc_get`/`doc_search` are the whole agent surface) ·
  `console/src/docs/markdown.ts:40` (`Block` already parses `list` + `cite`) ·
  `crates/brainiac-store/src/feedback.rs:23` (`insert` — the loop's terminus, already built)

- **Scenario**: An on-call agent is paged. It calls `runbook_start("payments/psp-failover")` and
  gets step 1 as a typed object: the instruction, the memory it cites, and an optional
  machine-checkable `verify`. It runs the step, calls `runbook_step(run_id, 1, "ok")`, and walks
  the procedure. At step 4 the documented health endpoint 404s. The agent reports
  `{outcome: "failed", observed: "404 /healthz"}` — and Brainiac does not file a ticket for a human
  to triage the *docs*: it files an `outdated` verdict against **exactly the memory step 4 cited**,
  with the observed output as the note, into the dispute triage that already exists. Three weeks
  later the Knowledge Health page says something no documentation product has ever been able to
  say: *"Of 41 published procedures, 33 last ran green, 4 failed at a cited step, 4 have never been
  executed."* The wiki has a **pass rate**.

- **Root cause**: Every page in the system is prose to be READ. The document layer's entire
  production feedback channel is `document_reads` — `(via, was_dirty)`, i.e. *that* a page was
  served and whether it was stale at that instant. Nothing anywhere records whether the page
  **worked**. Consequently all evidence for KB quality comes from the eval harness against golden
  fixtures (`docs_profile`, `drift_profile`) and from the advisory faithfulness judge (0036) —
  which grades prose against memories, never memories against the world. `DocKind::Runbook` is the
  place this gap is most expensive and most fixable: a runbook is the one page kind whose claims
  are *inherently executable*, and the product currently treats it as indistinguishable from a
  topic page. The cap is not the model or the retrieval; it is that a page has no way to be
  **wrong out loud**.

- **Impact**: Turns the KB from a read surface into a closed-loop control system, and turns the
  org's normal work into free, continuous verification with zero maintenance discipline required —
  the thing every wiki has failed at for thirty years. It also attacks the product's hardest
  substrate problem from a new angle: extraction recall sits ~0.38–0.46 and is the standing blocker
  on KB3 GA (the sequencing rule in KB-PLAN). Execution outcomes are *ground truth labels*
  generated by production — the first signal in the whole system that is not a model grading a
  model or a human grading a model. Platform surface: a run stream (`runbook_runs`) is exactly what
  an org's incident tooling wants to subscribe to, and it makes the KB the system of record for
  "how we do things" in a way a wiki page never can be. Reach: every deploy, incident response, and
  onboarding becomes a proof run instead of a read.

- **Fix sketch**:
  1. **The step list needs no new storage.** A `procedure: true` flag on `SectionBinding` selects a
     compose prompt variant that emits a numbered list where each step is ONE cited sentence.
     `enforce_citations` already guarantees per-paragraph citation, so this is the cheapest schema
     that could exist on top of the current firewall, and the steps parse out of the **published
     markdown** with the parser already shipped: `parseDoc` → `{t:"list"}` blocks whose inline
     `{t:"cite"}` node names the memory (`markdown.ts:40`, `markdown.ts:254`). No second
     representation to drift.
  2. **Runs.** One migration: `runbook_runs(id, org_id, document_id, revision_id, started_by,
     status, started_at)` + `runbook_step_events(run_id, step_no, memory_id, outcome, observed)`.
     Append-only, INSERT+SELECT grants only — the `document_reads` posture (0025), for the same
     reason.
  3. **MCP.** `runbook_start(slug)` (scope `kb:read`, published-only — the existing rule that an
     unsigned draft never reaches an agent applies verbatim) and `runbook_step(run_id, step,
     outcome, observed?)` (a write-class scope, same tier as `memory_feedback`). Add to
     `tool_scope` at `mcp.rs:217`.
  4. **The auto-verdict — the whole point.** A failed step calls
     `brainiac_store::feedback::insert` with `verdict = "outdated"` on the step's cited memory and
     rides the shipped feedback→dispute triage. Never direct-to-deprecated: one failed run is a
     witness, and an agent's shell is not an authority on the org's beliefs.
  5. **Health + reader.** `runbooks_green` / `runbooks_red` / `runbooks_never_run` beside
     `pages_never_read`, with an attention item when a published runbook's newest run is red;
     a Run panel in `DocReader.tsx` beside the existing `SectionEditor` wiring, rendering the
     parsed steps with their last outcome.
  6. **Smallest slice that proves it — build NO executor.** Ship `runbook_runs` +
     `runbook_step` + the auto-verdict, and let a coding agent that already has a shell be the
     runner. One integration test: publish a runbook whose step 3 cites memory M, report step 3
     failed, assert a feedback row lands on M and only M, assert the page reports a red run, and
     assert an `ok` run leaves the corpus untouched. `verify` predicates, retries, and an in-product
     executor are later rungs; the loop is worth more than the ergonomics.
