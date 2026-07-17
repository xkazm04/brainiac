-- PROJECT-PLAN PR3: cross-PROJECT divergence is its own detected class.
--
-- "Two teams disagree" (axis=team) and "checkout retries one way, billing
-- another" (axis=project) are different findings with different audiences:
-- the first is a team-alignment conversation, the second is usually a
-- per-stack rule the Library already models. The sweep now adjudicates both
-- axes; existing rows are team-axis by definition (the only axis that
-- existed), which is exactly what the DEFAULT backfills.
ALTER TABLE practice_divergences
    ADD COLUMN axis text NOT NULL DEFAULT 'team'
    CHECK (axis IN ('team', 'project'));
