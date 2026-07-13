-- Make feedback actionable: a `wrong` / `outdated` verdict is a claim
-- against the corpus that a maintainer must answer (re-verify the memory,
-- deprecate it, or dismiss the report). Open = unresolved = in the triage
-- queue. `helpful` verdicts are self-resolving — they assert nothing to fix.

ALTER TABLE memory_feedback
    ADD COLUMN resolution   text
        CHECK (resolution IN ('reverified', 'deprecated', 'dismissed')),
    ADD COLUMN resolved_by  uuid,
    ADD COLUMN resolved_at  timestamptz;

-- The triage queue reads unresolved negative verdicts, newest claim first.
CREATE INDEX idx_memory_feedback_open
    ON memory_feedback(memory_id)
    WHERE resolved_at IS NULL AND verdict IN ('wrong', 'outdated');
