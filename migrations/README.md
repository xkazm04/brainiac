# Migrations

Embedded at compile time via `sqlx::migrate!("../../migrations")` (see
`crates/brainiac-store/src/lib.rs`) and applied in **version order** — the
`NNNN_` filename prefix. `_sqlx_migrations` keys on that version number and
stores each file's checksum.

## Rules

1. **Append-only. Never edit or renumber an applied migration.** The version is
   a primary key and the checksum is verified on every boot: renaming a file or
   changing its body after it has run anywhere makes `migrate()` fail (orphaned
   version / checksum mismatch) on every DB that already applied it — including
   dev DBs. To change the schema, add a *new* migration.
2. **Additive SQL only** — new tables/columns, backfills, new indexes. A new
   table must `GRANT … TO brainiac_app` itself (0001's blanket grant only
   covered then-existing tables) and, if org-scoped, `ENABLE ROW LEVEL SECURITY`
   + a `USING (org_id = current_setting('app.org_id')::uuid)` policy. Global
   operator config (e.g. `sweep_schedules`) is the deliberate exception: no
   `org_id`, no RLS, just the grant.
3. **Claim the next free number when you start, not when you commit.** Parallel
   branches that both grab the next number collide (this happened: two `0015`s,
   then two `0017`s). If you're working alongside another in-flight branch, take
   a number safely above theirs and expect to bump it if they land first — a
   gap in the sequence is fine (sqlx tolerates gaps; there's a long-standing one
   at 0011), a duplicate is not.
4. **Order within the sequence doesn't have to match commit order.** A migration
   only needs to sit after the migrations it depends on. Authorship interleaving
   (ops migrations between KB migrations, say) is cosmetic and harmless.

## Current tail

```
0014 knowledge_health_snapshots   0016 practice_divergences   0018 sweep_schedules
0015 memory_facets                0017 document_layer
```
