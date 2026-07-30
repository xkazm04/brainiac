# Migration numbering: the 0011 gap is intentional, duplicates are the real danger

Migrations are embedded at compile time (`sqlx::migrate!("../../migrations")`)
and keyed by the `NNNN_` prefix, with each file's checksum verified on every
boot. Renaming or editing an already-applied migration makes `migrate()` fail
on every database that ran it — including dev boxes. Append only.

Facts that are easy to get wrong (all stated in `migrations/README.md`):

- **The sequence has a long-standing gap at `0011`.** sqlx tolerates gaps, so
  the gap is fine and must not be "fixed" by renumbering. What is *not* fine is
  a duplicate number — that has happened twice already (two `0015`s, then two
  `0017`s) when parallel branches each grabbed "the next" number.
- **Claim your number when you start, not when you commit.** If another branch
  is in flight, take a number safely above theirs and expect to bump it if they
  land first. (Real examples in the history: `0036` went to revision
  faithfulness while a concurrent session's project work took `0037`; `0025`–
  `0027` were taken by parallel workstreams before the Library got `0028`.)
- **A new table must carry its own grants and policies.** `0001`'s blanket
  `GRANT … TO brainiac_app` covered only the tables that existed then. An
  org-scoped table must also `ENABLE ROW LEVEL SECURITY` and add a
  `USING (org_id = current_setting('app.org_id')::uuid)` policy. Global
  operator config (e.g. `sweep_schedules`) is the deliberate exception: no
  `org_id`, no RLS, just the grant.
- Ordering within the sequence need not match commit order; a migration only
  has to sit after what it depends on.

Current tail as of migration `0040_project_isolation.sql`.
