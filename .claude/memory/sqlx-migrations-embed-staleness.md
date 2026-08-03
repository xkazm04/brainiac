# `sqlx::migrate!` does not re-embed when you add a migration

`brainiac-store` has **no `build.rs`**, so the `sqlx::migrate!` macro's embedded
migration set is baked into whatever compilation unit last built it. Adding a
file under `migrations/` does **not** invalidate that — cargo sees no changed
input for the already-compiled `brainiac-store` rlib.

**The symptom, and why it lies to you.** After adding `migrations/NNNN_*.sql`,
dependent test binaries fail with:

```
migration NNNN was previously applied but is missing in the resolved migrations
```

…under `cargo test --workspace`, while the *same* test passes when run standalone.
The message points at the database, so it reads like a corrupt or half-applied
schema. It is neither: one binary applied the new migration, and a **stale**
sibling binary — still carrying the old embedded set — then refused a database
that had moved past it.

**The fix**: `touch crates/brainiac-store/src/lib.rs` (or otherwise dirty the
crate) after adding a migration, so every dependent binary re-embeds. Cheap, and
it must happen before you trust a red workspace run.

**Do not confuse it with the other migration red**, which has the same shape and
a different cause: `migration NN was previously applied but has been modified` is
a checksum mismatch, meaning a *shipped* migration file was edited after being
applied to that database. That one is a real violation of the append-only rule
(or a dev database that needs recreating) — see [[migrations-numbering]].

Discovered 2026-08-04 by a builder adding migration 0046, after it had chased the
failure as a code defect.
