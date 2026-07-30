# The Library's two invariants are owned by the schema, not by app code

The Library is the **normative** layer (memories are descriptive, the KB is
compiled, the Library is what we *should do*). Plan: `docs/LIBRARY-PLAN.md`.
Substrate: migrations `0028` (standards, standard_versions,
standard_provenance, skills, skill_versions, library_usage_events + RLS +
`brainiac_app` grants), `0029` mining, `0030` proposals, `0031` standards pages,
`0032` skill proposals.

**The rule is the atom, not the standards document.** A `Standard` is ONE rule
(scope ▸ category ▸ rule) with a one-sentence statement, rationale and
good/bad examples in `detail_md`, plus `enforcement`, `lifecycle` and `origin`.
A "stack overview" is a projection over rules, never a stored document. Per-rule
identity is what makes per-rule telemetry possible.

Two invariants the database enforces, so you cannot bypass them from Rust:

1. **No unattributed rule.** A DEFERRED constraint trigger in `0028` refuses any
   standard leaving `proposed` without provenance rows or an explicit
   `decreed_by`. It is deferred so provenance written in the same transaction
   counts; `adopt_standard` forces it IMMEDIATE so the failure lands on the
   statement. Post-`0029` it narrows to adopted/deprecated so an evidence-free
   candidate can still be *rejected*.
2. **No leaderboard.** `library_usage_events` has a `team_id` column and
   **no user column**, verified by a schema-shape test; `record_usage` cannot
   accept a user id. All telemetry aggregates per team. Never add per-person
   ranking.

Other settled behaviour: the serve path filters **`adopted` only** (proposals
never reach an agent as policy; `skill_fetch` refuses unpublished bundles);
mining skips a signal already carried as provenance (unconditionally for live
standards, and within `BRAINIAC_LIBRARY_DEDUP_DAYS`, default 90, for rejected
ones — rejection is kept knowledge, the row is never resurrected); the
divergence→standard bridge `ratify_divergence` produces exactly ONE `proposed`
standard, idempotent by provenance (the divergence board detects, the Library
owns the artifact — no second pipeline); Library health signals go red on their
own and are deliberately **not** a 5th Knowledge-Health pillar (the composite is
pinned at 4 by a test so trend lines stay comparable).

Deliberately **not** built, each blocked on a precondition — don't slip them in:
enforcement hooks, cross-org/public exchange, per-rule adoption from repo scan,
generative skill mining, skill eval harness.
