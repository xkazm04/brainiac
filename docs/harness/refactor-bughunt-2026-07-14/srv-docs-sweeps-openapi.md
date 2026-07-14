> Context: Server: Docs + Sweeps + OpenAPI
> Total: 5 (Critical: 1, High: 1, Medium: 3, Low: 0)

## 1. `doc_edit` mutates pages and injects into the pipeline on a read-only scope, with no maintainer check
- **Severity**: Critical
- **Lens**: bug-hunter
- **Category**: broken-authorization
- **File**: crates/brainiac-server/src/docs.rs:398 (handler 385-487); contrast 296 + 312-322
- **Scenario**: A managed `brk_…` key minted with only `kb:read` (the scope the module doc at lines 24-29 says is for read-only agents) POSTs `/v1/docs/{slug}/edit`. RLS lets it see any org-visible page, so the guard at line 398 (`auth_of(&state, &headers, SCOPE_KB_READ)`) passes. For a **pinned** section it calls `update_pinned` + `mark_dirty` (425-434) — the edit is auto-published into the live markdown on the next compose with NO review gate (confirmed by doc_edit_pg.rs:294 asserting `#pay-oncall` reaches the published revision). For a **composed** section it calls `insert_source` + `enqueue_source` (460-476), creating an ingest job (LLM cost) whose raw text (449-459) is hardcoded to read "A maintainer edited the … section … They now state:" — attributing the caller's text to a maintainer it never verified.
- **Root cause**: `doc_edit` is a WRITE endpoint gated like a read. `doc_approve` (296) deliberately upgrades to `SCOPE_KB_PUBLISH` ("a token minted to READ the knowledge base must not be able to sign one") and additionally checks `is_maintainer`/`is_any_maintainer` (312-322). `doc_edit` inherited neither guard — it trusts RLS visibility (can-see) as if it were authorization (can-mutate).
- **Impact**: Any read-scoped agent token, or any non-maintainer org member, can (a) silently rewrite the published pinned prose of a shared org page with no human review, and (b) inject arbitrary text into the org's extraction pipeline framed as a maintainer's belief — poisoning candidate memories and burning LLM/billing. This is the exact read/write separation the module's own docstring promises, defeated on the one endpoint that writes.
- **Fix sketch**: Require `SCOPE_KB_PUBLISH` (or a new `kb:edit`) instead of `SCOPE_KB_READ`, AND add the same maintainer check `doc_approve` uses (`is_maintainer(doc.team_id)` / `is_any_maintainer` for org-wide pages) before either branch. Stop hardcoding "A maintainer edited" — derive the framing from the verified role, or drop the claim.

## 2. Concurrent section edits lose updates and an edit racing a recompose is silently dropped
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: lost-update-race
- **File**: crates/brainiac-server/src/docs.rs:408-434; store update_pinned crates/brainiac-store/src/documents.rs:170-180
- **Scenario**: `EditSectionBody` (343-352) carries no version/etag/base-revision. `doc_edit` reads the section for its `mode`/`heading` (415-423) then blindly overwrites via `update_pinned` — `UPDATE … SET pinned_content = $2 WHERE id = $1 AND mode = 'pinned'` (no version predicate). Two maintainers editing the same pinned section: the second `UPDATE` overwrites the first under READ COMMITTED; the first author is never told (no 409). Worse, the edit-vs-compose interleave: `doc_edit` sets `dirty_at` via `mark_dirty` (431), but a `compose_tick` that read the OLD section content can run its `insert_revision` afterwards, which sets `dirty_at = NULL` (documents.rs:266-278). The just-saved pinned prose is now excluded from the published revision AND the page is marked clean — the edit is invisible until some unrelated future dirty event.
- **Root cause**: Section writes are last-writer-wins with no optimistic-concurrency token, and dirty-marking is not coordinated with the composer's read snapshot; the handler assumes edits and recomposes never overlap.
- **Impact**: Silent lost updates on concurrent edits, and a dropped pinned edit that the KB4 "knowledge reaches every page by itself" promise is supposed to guarantee — exactly the failure `oldest_dirty_secs` was built to surface, but here the page reads clean so it stays invisible.
- **Fix sketch**: Add a version/`updated_at` guard to `EditSectionBody` and make `update_pinned` conditional (`WHERE … AND version = $3`), returning 409 on 0 rows affected. For the compose race, bump the section/document version inside `update_pinned` and have `insert_revision` only clear `dirty_at` when it composed from the current version (compare-and-clear), so an edit landing mid-compose keeps the page dirty.

## 3. A sweep that outruns the 2-hour stale window is re-dispatched while still running
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: sweep-reentrancy
- **File**: crates/brainiac-server/src/sweeps.rs:46 + run_due 239-264 (claim predicate 249-250) + execute 267-290
- **Scenario**: `run_due`'s claim excludes rows that are `running` UNLESS `last_run_at < now() - interval '2 hours'` (RUNNING_STALE, line 46). The staleness rule can't distinguish "worker died mid-sweep" from "sweep is legitimately still running." `divergence::scan_all`/`snapshot_all_orgs` loop every org with an LLM call per cross-team cluster; on a large tenant set a run can exceed 2h. When it does, the next 20s tick re-claims the same kind and `tokio::spawn`s a SECOND `execute` while the first is still alive.
- **Root cause**: Liveness of a running sweep is inferred purely from age, with no heartbeat or owning-worker token; the design assumes no sweep ever legitimately exceeds RUNNING_STALE.
- **Impact**: Two concurrent full-org LLM scans — duplicate billing, duplicate divergence/health writes, and two `record_result` calls racing to stamp the same row (267-290, 292-311). The very "expensive sweep" the 5-minute cadence floor and single-claim UPDATE were meant to protect against.
- **Fix sketch**: Heartbeat `last_run_at` (or a `heartbeat_at`) periodically from `execute`, and treat only rows with a stale heartbeat as crashed; or record a worker/run id on claim and refuse to record/redispatch for a stale claim. Raise RUNNING_STALE above any plausible real runtime as a stopgap.

## 4. The three pg test files duplicate their harness, and the docs_pg TRUNCATE list has drifted
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: test-duplication
- **File**: crates/brainiac-server/tests/docs_pg.rs:23-103 + tests/publish_pg.rs:21-103 + tests/doc_edit_pg.rs:32-115
- **Scenario**: `db_guard`/`DB_LOCK` (docs_pg 21-30, publish_pg 21-28) are byte-identical; `Ctx` + `setup` (org/team/user/member upsert) and the giant `TRUNCATE … CASCADE` block are near-identical across all three (doc_edit_pg inlines the same seed inside its single test). `team_page` (docs_pg 113-163) and `published_page` (publish_pg 139-176) are the same `worker_tx` insert_document+insert_revision pattern with the same "RLS applies SELECT policy to UPDATE's WHERE" comment copy-pasted. Separately, docs_pg's TRUNCATE (46-52) omits `document_publications` and `publish_targets`, which the other two files list (doc_edit_pg 35, publish_pg 44) — the lists have silently diverged.
- **Root cause**: No shared `tests/common/` (or `mod`) harness; each file grew its own copy, and edits to one (adding the publishing tables) never propagated to the others.
- **Impact**: Every schema change to the seeded tables must be applied in three places; the drifted TRUNCATE list is a latent flake if a future docs_pg test touches publishing rows (it relies on CASCADE from `documents` to clean them). ~150 lines of copy-paste.
- **Fix sketch**: Extract a `tests/common/mod.rs` with `db_guard`, `Ctx`, `setup`, the single TRUNCATE list, and a `seed_page(worker_tx, …)` helper; have all three files use it so the table set is defined once.

## 5. `doc_get` loads every pending revision in the org just to find one page's pending flag
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: unbounded-query
- **File**: crates/brainiac-server/src/docs.rs:189-193; store pending_revisions crates/brainiac-store/src/documents.rs:343-352
- **Scenario**: On every `GET /v1/docs/{slug}`, the handler calls `pending_revisions(&mut tx)` — a `SELECT … WHERE policy_decision = 'needs_review' AND reviewed_by IS NULL ORDER BY created_at` with NO `LIMIT` (documents.rs:343-352) — then throws all but one away via `.into_iter().find(|r| r.document_id == doc.id)` (192-193). It fetches (and deserializes `composed_from` JSON for) the org's ENTIRE review backlog to answer a single-document question.
- **Root cause**: `pending_revisions` was written for `docs_list` (which legitimately needs all of them to annotate the list), and `doc_get` reused it rather than a per-document lookup.
- **Impact**: Per-page-view cost grows O(org's pending-review queue). A back-logged org makes the hottest read path (viewing a page) progressively slower and heavier, for a boolean + one row it already knows the id filter for.
- **Fix sketch**: Add `pending_revision_for(conn, document_id)` (`… AND document_id = $1 ORDER BY created_at DESC LIMIT 1`) and call it from `doc_get`; leave the unbounded variant to `docs_list`, or give that one a sane cap too.
