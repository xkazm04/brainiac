# MEMORY.md

Durable, repo-specific learnings — invariants, standing gates and traps that are
**not** derivable from the code or the git log. One file per entry under
`.claude/memory/`.

Read the entries touching your area at session start; add one at session end
when you learn something durable. Facts and invariants only — no session logs,
no status that goes stale, nothing personal or machine-specific.

## Data plane & schema

- [Admin vs `brainiac_app` role split](.claude/memory/rls-role-split.md) — RLS works because the runtime pool demotes to a non-owner role; `admin_pool()` bypasses RLS and is only for the derived-data re-embed backfill.
- [Migration numbering](.claude/memory/migrations-numbering.md) — the `0011` gap is intentional and must not be renumbered; duplicate numbers have bitten twice; new tables carry their own grants + RLS policy.
- [Pg tests self-skip](.claude/memory/pg-tests-self-skip.md) — without `DATABASE_URL` the `_pg` tests print SKIP and pass; local pgvector is on **5433**, CI on 5432.

## Standing gates & calibration

- [KB publishing gate](.claude/memory/kb-publishing-gate.md) — KB0–KB5 are built but `kb_enabled` is false with no publish target by design, pending the extraction-recall gate; the KB is a one-way projection.
- [`DRIFT_MARGIN` is load-bearing](.claude/memory/drift-margin-is-load-bearing.md) — 0.05 with only 0.013 of headroom on the real embedder; changing it is gate-affecting.

## Architecture invariants

- [Library invariants](.claude/memory/library-invariants.md) — the rule is the atom; a DEFERRED trigger forbids unattributed standards; `library_usage_events` has no user column; only `adopted` rules are served.
- [Project is a facet, not a partition](.claude/memory/project-is-a-facet.md) — RESTRICTIVE-policy opt-in isolation, longest-segment-aware `path_prefix`, inclusive retrieval lens, and the known KB-compose isolation gap.
- [Console structure & sweeps](.claude/memory/console-structure-and-sweeps.md) — modules under `app/console/modules/`, one middleware allow-list, no per-page gating; periodic jobs are `sweep_schedules` kinds, not cron.
- [MCP search governance defaults](.claude/memory/mcp-search-governance-defaults.md) — raw rows and both sides of an open contradiction are withheld by default; each default was earned by an adversarial run that failed without it.

## Method & operations

- [UAT trial method](.claude/memory/uat-trial-method.md) — three arms, verdict is C − B and may be negative, primary endpoint is efficiency not code quality; two `gap: none` controls must show no delta.
- [Zero-cost deploy recipe](.claude/memory/zero-cost-deploy-recipe.md) — self-host pgvector because managed Postgres is the only paid part; Singapore-only, the compute trial is one-shot, and the offline fallback costs ~0.19 NDCG.
