> Context: Core: Domain Types + Embeddings
> Total: 5 (Critical: 0, High: 1, Medium: 3, Low: 1)

## 1. `SectionBinding` derived `Default` yields `max_items = 0`, silently contradicting the serde default of 12
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: default-divergence
- **File**: crates/brainiac-core/src/types.rs:505-527
- **Scenario**: `SectionBinding` derives `Default` (line 505) *and* declares `#[serde(default = "default_max_items")]` on `max_items` (521-522, `default_max_items() -> 12`). The two "default" mechanisms disagree: JSON deserialization of a binding that omits `max_items` gets **12**, but the derived `SectionBinding::default()` sets `max_items = 0usize` (integer `Default`). Any idiomatic `SectionBinding { query: q, ..Default::default() }` (the exact pattern used at compose.rs:132, extract.rs:498, and throughout the pipeline for *other* structs) that forgets to set `max_items` silently gets 0.
- **Root cause**: A serde field-default and a `#[derive(Default)]` were both bolted onto the same struct without reconciling them; nobody notices because every current caller (compose.rs:575-580, compose_pg.rs:180-183, doc_edit_pg.rs:90-93) happens to set `max_items` explicitly, so the trap is latent.
- **Impact**: A composed section built through the derived `Default` produces an **empty page section with no error**: compose.rs computes the retrieval cap as `binding.max_items * 3` (=0 → `LIMIT 0`, line 118), fan-out `k = max_items * 2` (=0, line 127), and finally `kept.truncate(binding.max_items)` (=0, line 166). The KB layer renders a blank section and reports success — the silent-failure signature. Because `SectionBinding` is a `pub` type in `brainiac-core`, any future internal caller or external consumer using `::default()` is exposed.
- **Fix sketch**: Remove the `Default` derive and provide a hand-written `impl Default` that calls `default_max_items()` (single source of truth), OR drop the serde attribute and give the derive a sane value via a newtype/`Option<usize>` with an explicit resolve step. Add a unit test asserting `SectionBinding::default().max_items == 12`.

## 2. `cosine` silently zips to the shorter vector on dimension mismatch and propagates NaN
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: silent-wrong-result
- **File**: crates/brainiac-core/src/embed.rs:108-118
- **Scenario**: `cosine` computes `dot` via `a.iter().zip(b)` — `zip` stops at the shorter iterator — while `na`/`nb` are each summed over the *full* length of their own slice. Passing a 768-dim vector and a 1024-dim vector returns a plausible-looking number (dot over the first 768 dims, normalized by mismatched norms) instead of erroring. Separately, if a real remote embedder (DashScope, per the module doc) returns a vector containing `NaN`, `na` becomes `NaN`, the `na == 0.0` guard is `false`, and the function returns `NaN`.
- **Root cause**: The doc comment says "assumed same dim" but nothing enforces it; the zero-norm guard was written for the empty-vector case only and does not cover `NaN`/non-finite inputs. The embedding system is explicitly multi-dimension (reembed.rs versions embeddings by `(model_name, dim)`; reembed_pg.rs:162 asserts two embedders have *different* dims), so mismatched-length vectors are a first-class possibility in a bake-off/re-embed window.
- **Impact**: A dimension mismatch yields a wrong similarity rather than a caught error; a `NaN` similarity poisons downstream ranking/fusion comparisons (undefined sort order). Blast radius is currently limited because this `pub` function has **no caller outside its own test module** (see finding 5) — but it is public API and a latent footgun the moment anyone wires it into ranking.
- **Fix sketch**: `debug_assert_eq!(a.len(), b.len())` and early-return `0.0` (or `Result`) on length mismatch; extend the guard to `if !na.is_finite() || !nb.is_finite() || na == 0.0 || nb == 0.0 { return 0.0; }`.

## 3. `embed_sync` tokenizer drops single-char tokens by *byte* length and collapses non-space-delimited scripts into one bucket
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: unicode-edge-case
- **File**: crates/brainiac-core/src/embed.rs:71-90
- **Scenario**: Tokenization is `text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|t| t.len() > 1)`. Two issues: (a) `t.len()` is the **byte** length, so single-char ASCII tokens (a digit `5`, a one-letter identifier) are dropped while a single multi-byte CJK character (3 bytes) is kept — an inconsistent threshold. (b) `char::is_alphanumeric()` is `true` for CJK/Japanese/Thai ideographs, which carry no spaces, so an entire phrase like `支付网关重试` becomes **one token** hashed into **one** of `dim` buckets.
- **Root cause**: The filter intends "skip trivial single-character noise" but uses byte length, and the splitter assumes whitespace/punctuation word boundaries that don't exist in several scripts. This is the production `v0` default embedder (`DeterministicEmbedder`), used on the retrieval hot path (retrieval.rs:239) and for content embedding (extract.rs:668).
- **Impact**: For any non-English (CJK/JP/Thai) memory, two near-identical sentences sharing most characters hash to entirely different single buckets → cosine ≈ 0, so near-duplicate detection, contradiction search, and vector retrieval are effectively blind to that content. This exceeds the module's documented caveat (which only claims weakness "on true paraphrase"), and it fails silently. English single-char tokens are also silently discarded.
- **Fix sketch**: Filter on `t.chars().count() > 1` (or `!t.is_empty()`), and segment CJK by emitting per-character (or n-gram) tokens for runs of ideographic characters so token-overlap signal survives; at minimum, document the script limitation where the embedder is selected.

## 4. Graph/source vocabulary is typed as enums but the struct fields are raw `String`, so the type layer enforces nothing
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: type-safety-gap
- **File**: crates/brainiac-core/src/types.rs:371, 615, 628, 678
- **Scenario**: `EntityKind`, `EdgeRelation`, and `SourceKind` exist as validated enums (with `parse`/`as_str`), yet the domain structs still declare `Source.kind: String` (371), `Entity.kind: String` (615), `CanonicalEntity.kind: String` (628), and `Edge.relation: String` (678), each with only a doc comment pointing at the enum. Because these fields are `String`, serde will happily deserialize any value, and any code path that constructs them without routing through `parse()` stores an arbitrary/typo'd kind.
- **Root cause**: A deliberate "wire/DB stay strings" choice (per the comments) that pushes validation out of the type system and into an out-of-band "extraction firewall." That firewall exists only on the extraction path (`extract.rs:616` coerces via `EntityKind::parse(..).unwrap_or_default()`, `extract.rs:647` drops unknown `EdgeRelation`s) — nothing structurally guarantees other write paths (manual `memory_add`, direct store inserts, fixture loads) do the same.
- **Impact**: The invariant the enums were introduced to establish ("no typo ever reaches the DB") holds only where a caller remembers to validate; an unvalidated path silently persists a bogus `kind`/`relation`, and downstream consumers that `parse()` on read get `None` and silently skip/mis-handle the row. The guarantee is path-dependent rather than type-enforced.
- **Fix sketch**: Change the fields to the enum types (keeping DB/wire as strings via a `#[serde(with = ...)]`/`as_str`/`try_from` adapter), so every construction and deserialization is forced through validation; or, if strings must remain, funnel all writes through a single constructor that validates.

## 5. Three separate `cosine` implementations; the core one has no caller outside its own tests
- **Severity**: Low
- **Lens**: code-refactor
- **Category**: duplication/dead-code
- **File**: crates/brainiac-core/src/embed.rs:108-118 (and lib.rs:1-19)
- **Scenario**: `brainiac_core::embed::cosine` (f32) is duplicated by two independent local `fn cosine(a,b) -> f64` copies in the eval crate (extraction_profile.rs:168, docs_profile.rs:180), and the core function itself is referenced only from its own `#[cfg(test)]` module — no production or library caller uses it (store/entities do cosine in pgvector SQL).
- **Root cause**: Eval needed `f64` accumulation and reimplemented locally instead of reusing/extending the core primitive; the core version was never wired into a real consumer, leaving it as public-but-unused surface.
- **Impact**: Directly contradicts this crate's stated charter (lib.rs:4-6: "quality-critical logic … has exactly one implementation"): a similarity metric now has three implementations that can drift (and the core copy carries the finding-2 footgun while the eval copies may not). Maintenance and audit cost with no offsetting use.
- **Fix sketch**: Make `embed::cosine` the single source of truth (generic over `f32`/`f64` accumulator, or expose an `f64` variant) and have the eval profiles call it; delete the duplicates. If the core function is genuinely not meant for library use, drop it or gate it behind the test module.
