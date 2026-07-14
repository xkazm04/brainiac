> Context: Fixtures: Golden Loader + Validate + Schema
> Total: 5 (Critical: 0, High: 2, Medium: 3, Low: 0)

## 1. Missing/misnamed `documents/pages.yaml` silently voids the entire composition-gold + zero-tolerance leak gate
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: vacuous-validation
- **File**: crates/brainiac-fixtures/src/loader.rs:77-84 (loads over empty vec at validate.rs:179-255)
- **Scenario**: `load_unvalidated` treats `documents/pages.yaml` as optional: `if p.exists() { read_yaml(&p)? } else { DocumentsFile { documents: vec![] } }`. Rename the file, move the tree, drop a `documents/` subdir, or introduce a YAML-anchor typo in the path, and `documents` becomes an empty list. Every composition-gold check in `validate::lint` (lines 179-255: `doc-forbidden-unknown`, `doc-staleness-unknown`, `doc-section-shape`, `doc-binding-entity-unknown`) then iterates zero items and emits zero diagnostics — the tree validates green. Downstream, `docs_profile.rs:272` likewise reports "nothing to score" and no-ops.
- **Root cause**: The absence sentinel was added for backward-compat with "older fixture trees" (lib.rs:24-26), conflating "this tree legitimately has no docs profile" with "the docs file failed to load." There is no manifest or expected-count assertion distinguishing the two. Critically, the load-and-validate test (tests/load_v1.rs) asserts teams/merges/temporal/qa/leak counts but **never** asserts `documents.len()`, so nothing guards the regression.
- **Impact**: The file's own header (validate.rs:173-178) calls the forbidden-memories leak list "the highest-stakes reference in the whole tree" and warns a vacuous pass "is worse than having no gate at all, because it reports safety it never verified." An absent pages.yaml produces exactly that failure mode for the WHOLE composition suite, across every eval run, with no signal.
- **Fix sketch**: Make presence explicit — read a required top-level `profiles`/manifest flag (or a `documents/` marker) that declares whether the docs profile is expected; if expected, a missing/empty `pages.yaml` must be a load error, not a silent empty. Minimally, add `assert!(!fx.documents.documents.is_empty())` to load_v1.rs so v1 regressions surface, and distinguish "file missing" from "file present with empty `documents:`".

## 2. `stable_uuid` collision check covers only 4 of ~10 persisted id namespaces (and emits in non-deterministic order)
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: id-collision-coverage
- **File**: crates/brainiac-fixtures/src/validate.rs:257-278
- **Scenario**: The block comments itself "Stable-uuid collision check across every id namespace we persist," then chains only `teams`, `users`, `entities.keys()`, `memories.keys()`. But `stable_uuid` is a namespace-flat pure hash of the raw id string, and the seeders persist far more: documents (`docs_profile.rs:291 stable_uuid(&d.id)`), transcripts (`extraction_profile.rs:242`, `pipeline_profile.rs:243 stable_uuid(&t.id)`), plus contradiction/temporal/qa/leak ids. Any FNV-1a collision between, say, a document id and a memory id — or a document id string accidentally reused as a transcript id — maps two distinct fixture entities onto one UUID and is never detected here. Two rows collapse to one primary key at seed time and one silently overwrites the other.
- **Root cause**: The check was written against the id namespaces that existed when it was added and never grew as documents/transcripts became persisted entities; the comment overclaims coverage, masking the drift. Because the collision domain is the flat string (no per-type prefix baked into the hash), cross-namespace collisions are exactly what a global check must catch — but the iterator stops at four sources.
- **Impact**: Undetected UUID collision = silent corruption of the seeded eval database (lost/overwritten rows) for every profile that persists the omitted namespaces — the precise "success theater" the loader exists to prevent. Secondary: the loop iterates `entities.keys()`/`memories.keys()` (HashMap order), so when a collision *is* hit the reported `prev`-vs-`id` pairing and the diagnostic's position in `lint()` output are non-deterministic across runs, undermining diffable/CI-annotated lint output.
- **Fix sketch**: Extend the chained iterator to include documents, transcripts, contradictions, temporal cases, qa and leak query ids (and the derived `canon-*`/`edge-*`/`*-s{i}`/`::` ids the seeders mint, if those are to be guaranteed too). Iterate the underlying Vecs (not HashMap `.keys()`) so both detection and emission order are deterministic; update the comment to state actual coverage.

## 3. Schema export drifted from the loader: `DocumentsFile`/pages.yaml has no generated JSON Schema
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: schema-drift
- **File**: crates/brainiac-fixtures/src/export.rs:14-54
- **Scenario**: `schemas()` enumerates org/entities/merges/memories/transcript/contradictions/temporal/qa/leak but omits `DocumentsFile` (`documents/pages.yaml`), even though `schema.rs:132-201` fully defines it and the loader parses it. `export_schemas` therefore writes no `documents.schema.json` and the generated README wires up no glob for it, so the one fixture file carrying the leak-gate `forbidden_memories` lists gets zero editor validation.
- **Root cause**: `export.rs` maintains a hand-written parallel list of `(out_name, target, schema_for!(T))` tuples that must be updated in lockstep with the loader; documents was added to loader/schema/validate but not to this list. The module's own doc comment claims the export exists "so the editor-facing contract can never drift from what the loader actually parses" — it has drifted.
- **Impact**: Fixture authors editing pages.yaml get no schema-driven autocomplete/validation on the highest-stakes file, so structural mistakes (wrong section `mode`, missing `bindings`) survive until the Rust validator runs. Also a latent trap: any future struct added to schema.rs but forgotten here repeats the omission silently.
- **Fix sketch**: Add `("documents.schema.json", "documents/pages.yaml", schema_for!(DocumentsFile))` to the list. Better, drive the list from a single source (e.g. a small macro or a table also consumed by the loader) so a fixture file can't exist in the loader without a corresponding schema entry, and add a test asserting one schema per loaded fixture file.

## 4. Transcript loader silently drops unreadable dir entries and accepts a zero-transcript tree
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: crates/brainiac-fixtures/src/loader.rs:56-65
- **Scenario**: Directory entries are filtered with `.filter_map(|e| e.ok().map(|e| e.path()))` — any per-entry `io::Error` (permission, transient FS error, broken symlink) is swallowed and that transcript vanishes from the corpus. The surrounding load still succeeds. Separately, if the `transcripts/` dir is empty (or every file has a non-`.yaml`/`.yml` extension), `transcripts` is `[]` and `validate` imposes no minimum, so `load()` returns a valid `Fixtures` with zero transcripts.
- **Root cause**: `e.ok()` was used for ergonomic filtering, trading a fallible read for a silent skip; the loader treats "some transcripts" as always acceptable because transcripts are referenced by-id from memories rather than counted.
- **Impact**: A partially-readable or partially-mislabeled `transcripts/` dir yields a silently truncated corpus. Extraction/pipeline profiles then run against fewer transcripts than the eval assumes, quietly deflating recall/extraction scores with no error — a corpus-shrinkage variant of success theater. Only the count assertion in load_v1.rs (`== 9`) would catch it, and only for the exact v1 tree, not for CLI/eval callers.
- **Fix sketch**: Propagate the entry error (`for e in entries { let e = e?; ... }`) instead of `.ok()`, so an unreadable entry fails loudly. Add a validate-time check that `fx.transcripts` is non-empty (or matches an expected manifest count) so a wiped/mislabeled transcripts dir surfaces as a diagnostic rather than a green load.

## 5. ~10× duplicated "id ∈ map else emit unknown-X" referential-check boilerplate bloats validate.rs
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-fixtures/src/validate.rs (e.g. 310-320, 334-343, 420-428, 430-443, 445-453, 504-514, 540-552, 559-567, 616-626, 681-687, 712-723, 757-764)
- **Scenario**: The same shape — "look up an id in `entities`/`memories`, and on miss push an `unknown-entity`/`unknown-memory` diagnostic naming the file, item locator, and message" — is hand-written a dozen-plus times across the memories, merges, transcripts, contradictions, temporal, qa, and leak sections. Each copy re-specifies the rule slug, the `at(...)` locator, and a near-identical message, so the 803-LOC file is dominated by this repeated scaffolding.
- **Root cause**: `check_unique` was extracted as a helper but the equally-repetitive *reference* check never was; each new referential rule was added by copy-paste, and the `let at = |field| ...` closure is likewise re-declared per loop.
- **Impact**: High edit-cost and drift risk — the `transcript-gold` block already shows the hazard, mixing `F_TRANSCRIPTS` and `F_MEMORIES` files across two nearly-identical emits (lines 504-537); a copy-paste that forgets to swap the map or the rule slug produces a check that silently validates the wrong thing. Volume also obscures the genuinely distinct semantic checks (visibility/can_read, supersede-direction) among the boilerplate.
- **Fix sketch**: Add a helper `fn check_ref(map: &HashMap<&str,_>, id: &str, rule, file, item, msg, e)` (or a small `require_memory`/`require_entity` pair returning `Option<&T>` so callers can chain the found value) and route the ~12 sites through it. Fold the repeated `let at = |field| format!(...)` into the helper or a tiny per-item struct. This shrinks validate.rs materially and makes each rule a one-liner whose intent is visible.
