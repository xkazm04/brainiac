-- "Why did you deprecate this?" was unanswerable: the resolve endpoint took a
-- verdict and nothing else, so the permanent deprecation of an org memory
-- carried no rationale into the audit trail.
--
-- A separate column, deliberately: `memory_feedback.note` (0004) belongs to the
-- REPORTER — it is the claim itself ("this is wrong because ..."). Writing the
-- maintainer's answer over it would destroy the evidence the decision was made
-- on. Named to match `contradictions.resolution_note`, which is the same field
-- on the neighbouring governance path.
ALTER TABLE memory_feedback
    ADD COLUMN resolution_note text;
