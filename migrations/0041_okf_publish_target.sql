-- The `okf` publish target could never be persisted.
--
-- 0020 wrote `publish_targets_kind_check CHECK (kind IN ('git', 'confluence'))`
-- when those were the only two publishers. KB5 then shipped a third —
-- `brainiac-publish/src/okf.rs`, dispatched by `publisher_for` and documented in
-- KB-PLAN as the OKF bundle target — but nobody widened the constraint. The
-- result: `insert_target` rejects every `okf` row, so the entire OKF export path
-- (bundle, index.md, log.md, agent pointer files) was unreachable in production
-- while its unit tests stayed green, because they construct the publisher
-- directly and never touch the database. The first integration test to drive
-- `publish_org` end to end (crates/brainiac-publish/tests/okf_pg.rs) found it.
--
-- Widening an allowed set is safe by construction: every existing row already
-- satisfies the new predicate, so the revalidation is a formality.

ALTER TABLE publish_targets
    DROP CONSTRAINT publish_targets_kind_check;

ALTER TABLE publish_targets
    ADD CONSTRAINT publish_targets_kind_check
    CHECK (kind IN ('git', 'confluence', 'okf'));
