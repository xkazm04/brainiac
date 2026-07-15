# Fix Wave 9 — Correctness Edge Cases

> 4 commits, 7 findings resolved (all High). **This closes the last bug wave** —
> Themes A–J are done; only the refactor tail (Theme R) remains.
> Gates green: `cargo check --workspace --all-targets` 0 errors · 118 Rust DB-free
> tests · console `tsc` 0 / vitest 58/58.

Two of this theme's items (R07#3 reembed dim-mismatch, R12 cyclic dedupe) already
landed in Wave 2, so seven remained.

## Commits

| # | Findings | Files |
|---|---|---|
| 1 | R11#1 SectionBinding default divergence | `core/types.rs` |
| 2 | R03#1 `as_of` silently answers "now" | `server/mcp.rs` |
| 3 | R08#1 graph expansion ranked by UUID | `store/memories.rs` |
| 4 | C12#1 + C10#2 + C14#1 + C02#1 | 7 console files |

## What was fixed

1. **`SectionBinding` had two defaults that disagreed (R11#1).** It both derived
   `Default` (giving `max_items = 0`) and declared `#[serde(default)]` (giving 12).
   So the idiomatic `SectionBinding { query, ..Default::default() }` silently got a
   cap of 0 — and compose derives everything from it (`LIMIT max_items * 3` = 0,
   `k = max_items * 2` = 0, `truncate(0)`), rendering an **empty page section and
   reporting success**. Latent only because every current caller sets `max_items`
   explicitly, but the type is `pub` in brainiac-core. Replaced with a hand-written
   `impl Default` calling `default_max_items()`; test asserts all three paths agree.

2. **`as_of` silently answered a historical question with live data (R03#1).**
   Parsed with `.and_then(|s| s.parse().ok())` — deliberately lenient — so an
   unparseable value became `None`, and `None` means "as of now". An agent asking
   "what was true at T" got "what is true now": no error, no marker, no way to
   detect the substitution — a confidently-wrong point-in-time answer in a
   *temporal* memory engine. Easy to hit by accident too: RFC3339 requires an
   offset, so the common date-only `2026-01-01` fails to parse. Every other
   narrowing param already errored with -32602; `as_of` now matches.

3. **Graph expansion returned the lowest UUIDs, not the strongest (R08#1).**
   `SELECT DISTINCT ON (m.id) … ORDER BY m.id, m.created_at DESC LIMIT $2` —
   Postgres forces DISTINCT ON's ORDER BY to lead with the key, and that ordering
   is final, so the LIMIT kept the N smallest UUIDs and the trailing `created_at
   DESC` was dead code. Since retrieval scores every graph extra with the identical
   `graph_relevance(anchor_strength)`, **the selection is the entire result** — the
   headline "cross-team knowledge surfaces here" feature was arbitrary, and with
   time-ordered (v7) UUIDs deterministically returned the *oldest*. Nested the
   dedupe inside the ranking.

4. **Console (one commit, four bugs):**
   - **C12#1** — `new Date(x).toISOString()` throws on an unparseable value and
     ran per-card in a mapped list, so one malformed `detected_at` crashed the
     whole divergence board; the route had no `error.tsx`, so it white-screened.
     Guarded, and added the missing boundaries for **divergence, docs and health**
     (the finding named two; there were three). All ten module routes now have one.
   - **C10#2** — the archive header printed `{visible.length} memories true then`
     while rendering only `slice(0, 40)`, with no pagination or affordance: the
     remainder was never rendered, never selectable, its provenance unreachable,
     while the header claimed otherwise. Index-driven window + honest "showing X of
     N" + load-more.
   - **C14#1** — LedgerField's `draw` self-reschedules, and the ResizeObserver
     called `draw()` → a second RAF chain overwriting the shared handle (leaking
     the old id past cleanup) and advancing the shared physics once per chain per
     frame. RO fires once on `observe()`, so the hero ran at 2× from mount.
   - **C02#1** — under `prefers-reduced-motion` Home's `draw` runs once, but
     `resize()` clears the surface, so any resize blanked the pitch's primary
     visual forever.

## Patterns established (catalogue items 21–22)

21. **Two defaults on one field is a bug waiting for its first caller.** A derive
    and a serde attribute both named "default" and disagreed silently. If a type
    has a domain default, it needs exactly one source of truth — and a test that
    pins every construction path to it.
22. **A silent fallback on a *narrowing* parameter is a wrong answer, not a
    kindness.** `as_of` degrading to "now" and the archive's `.slice(0, 40)` are the
    same shape: the system quietly answered a different question than the one asked.
    Narrowing inputs should error; capped outputs should say they were capped.

## Campaign status

**All nine bug waves are complete.** ~51 findings closed across Waves 1–9 + R01#1,
including all 5 Criticals. Remaining: the ~48 Medium/Low refactor tail (Theme R) —
dead `NavStatus.tsx` and `MODULE_BAND`, duplicated `vector_literal` / `cosine` /
TRUNCATE lists, forked `governance-api.ts` transport, god-components (Home 765 /
StationModules 906 / Pitch 851 LOC).

## ⚠ Still needs a Postgres run before merge

1. Migration **0021** + the `make_interval` compose-backoff expression (Wave 7) —
   `dirty_documents` now depends on it.
2. **R05#1** promotion counts in `pipeline_pg` (Wave 2).
3. Wave 3's document-path fixes: `compose_pg` / `docs_pg` / `publish_pg`.
4. Wave 2's `reembed_pg`, Wave 5's `store_pg` (`set_memory_status` → `Result<bool>`),
   R01#1's `console_pg` health assertions.
