> Context: Store: Orgs + Entities + Governance
> Total: 5 (Critical: 0, High: 2, Medium: 2, Low: 1)

## 1. `set_memory_status` swallows a 0-row update, so promotion review records a phantom approval
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: crates/brainiac-store/src/governance.rs:154-172
- **Scenario**: The human promotion-review path (`console.rs:190-202`) first stamps the `promotions` row `policy_decision='approved', reviewer_id=…, reviewed_at=now()`, then calls `set_memory_status(memory_id, new_status)` and commits. `set_memory_status` runs `UPDATE memories SET status=… WHERE id=$1` and returns `Ok(())` regardless of rows affected. If that UPDATE matches 0 rows — the memory was hard-deleted since the promotion was queued, or the promotion's `memory_id` points at a memory outside the caller's org (nothing validates `promotions.memory_id`'s org at insert; the `memories_update` policy filters it to org), — the status never changes, yet the audit row is already marked approved and the API returns `memory_status = new_status`.
- **Root cause**: The function signature is `Result<()>`; it cannot report "no row changed." It trusts RLS/existence to hold instead of verifying. The correct pattern lives 15 lines below it in `apply_supersession`, which snapshots the row (existence + RLS gate) and returns `bool`.
- **Impact**: Success theater in the governance audit trail: a reviewer "approves" a promotion, the ledger says approved, the API confirms the new status, but the memory's real status is unchanged — the two sources of truth silently diverge. `mark_dirty_for_memory` is also spent for nothing. No error surfaces anywhere.
- **Fix sketch**: Return `Result<bool>` from the rows-affected count (`res.rows_affected() == 1`), or add `RETURNING id` + `fetch_optional`. Have `console.rs` treat a `false`/`None` as a hard error and roll back the promotion stamp rather than committing a phantom approval. Only `mark_dirty` when a row actually changed.

## 2. `apply_supersession` never validates the winner — self-supersession and A↔B cycles corrupt the temporal graph
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: state-corruption / TOCTOU
- **File**: crates/brainiac-store/src/governance.rs:187-248
- **Scenario**: The function deprecates `loser`, sets `superseded_by = winner`, closes `valid_to`, and its only guard is `loser`'s own `superseded_by IS NULL`. `winner` is never checked. (a) Self-merge: `loser == winner` (a maintainer resolving a contradiction against the same memory) sets `superseded_by = self` and `status = deprecated` — a memory permanently deprecated and pointing at itself. (b) Cross-visibility winner: `winner` may live in a team the readers of `loser` cannot see; retrieval stage 5 then drops `loser` and serves a winner nobody in that scope can read — the fact vanishes. (c) Concurrent cycle: two transactions apply `A→B` and `B→A`. Each snapshots the *other* row (`fetch_optional`, no `FOR UPDATE`) seeing `superseded_by IS NULL`, then updates *different* rows (A vs B) so the locks never collide; both commit, yielding `A.superseded_by=B` and `B.superseded_by=A`.
- **Root cause**: The `superseded_by IS NULL` guard only prevents re-superseding the *same* loser; it says nothing about the winner's identity, existence-in-scope, live-ness, or whether the winner is itself already superseded by the loser. The snapshot SELECT is a plain read, not a locking read, so it is a classic check-then-act.
- **Impact**: Temporal dedupe (stage 5, the "wiki that cannot rot" promise in the header) follows `superseded_by`. A self-pointer or an A↔B cycle makes it serve neither side or loop; a cross-team winner makes a live fact silently unreachable. Governance-driven data loss / corruption.
- **Fix sketch**: Reject `loser == winner` up front. Verify `winner` is visible/live to the caller and not already `superseded_by = loser` (a single `SELECT … FOR UPDATE` over both ids, ordered by id to avoid deadlock, or a guard `AND winner NOT IN (SELECT loser-chain)`). Take `FOR UPDATE` on the snapshot so concurrent opposite-direction applies serialize.

## 3. `insert_contradiction` has no self-pair guard and no dedup — duplicate/self contradictions flood the queue
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: edge-case / unbounded-duplication
- **File**: crates/brainiac-store/src/governance.rs:425-446
- **Scenario**: Every call inserts a fresh `uuid` with `status='open'` and no `ON CONFLICT`, no `memory_a <> memory_b` check, and no `(a,b)` ordering normalization. The `contradictions` table (migrations/0001_init.sql:181-192) has only a PK on `id` — no unique index on `(org_id, memory_a, memory_b)`. So the same conflicting pair detected on every pipeline run inserts a brand-new open row each time; `(A,B)` and `(B,A)` are stored as two distinct conflicts; and `memory_a == memory_b` (a memory contradicting itself) is accepted.
- **Root cause**: The detector is expected to be exactly-once and to always pass distinct, canonically-ordered ids — neither the store function nor the schema enforces it.
- **Impact**: `open_contradictions_for` (governance.rs:268) returns N copies of the same `ContradictionFlag` for a served memory, and the maintainer review queue fills with duplicates that must each be resolved; a self-pair flags a memory as conflicting with itself. Governance signal quality degrades over time.
- **Fix sketch**: Guard `memory_a <> memory_b` (return early / error). Normalize the pair (`least/greatest`) and add a partial unique index `ON contradictions (org_id, memory_a, memory_b) WHERE status='open'`, then `ON CONFLICT DO NOTHING` so a re-detected open conflict is a no-op.

## 4. `vector_literal` is duplicated verbatim from `memories.rs` (drift risk in pgvector serialization)
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-store/src/entities.rs:337-350
- **Scenario**: `vector_literal(&[f32]) -> String` builds the `"[1,2,3]"` pgvector text literal, and its own doc comment admits "mirrors memories.rs." Two independent copies of the exact float-formatting/serialization logic now exist, each bound as text and cast `::vector` in several queries here (`upsert_canonical_embedding`, `nearest_canonical`).
- **Root cause**: A pgvector client-crate dependency was deliberately avoided, so the hand-rolled literal builder was copy-pasted into whichever module needed it rather than shared.
- **Impact**: The two copies can drift (e.g., one gains NaN/precision handling, the other doesn't), silently producing embeddings that serialize differently in the entity vs memory spaces — the kind of divergence that corrupts cosine results without a compile error. Pure maintenance cost otherwise.
- **Fix sketch**: Hoist one `pub(crate) fn vector_literal` into `lib.rs` (or a small `pgvector` util module) and call it from both `entities.rs` and `memories.rs`; delete the duplicate.

## 5. The `NewMemory` builder closure is copy-pasted across ~6 tests in the 1386-LOC store_pg god-module
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: duplication / god-module
- **File**: crates/brainiac-store/tests/store_pg.rs:104-123 (also 280-297, 365-382, 734-751, 1171-1188, plus inline literals at 478-497 and 558-575)
- **Scenario**: Nearly every test redefines a local `let mk = |id, …| memories::NewMemory { … }` closure that fills the same 15 fields with the same defaults (`owner_user_id: None, status: Canonical, kind: Fact, lifecycle: Default, language: "en", valid_*: None, …`), differing only in id/team/content. Two more tests inline the full struct literal. The 1386-line file is a single flat module of these near-identical fixtures.
- **Root cause**: No shared test fixture/builder; each test grew its own copy of the seed helper.
- **Impact**: ~100+ lines of duplicated struct boilerplate; adding a field to `NewMemory` means touching every copy, and the divergent inline literals are easy to miss. Slows every future change to the store test suite.
- **Fix sketch**: Extract one `fn new_memory(id: u8, team: u8, content: &str) -> NewMemory` (or a small builder with `.visibility()/.owner()` overrides) in a `mod common`/test helper; replace the per-test closures and inline literals with calls to it.
