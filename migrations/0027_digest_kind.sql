-- The proactive digest (KB-PLAN follow-up #3, UAT P1.5) — as a DOC KIND, not a
-- feature. The design note that shaped this: a digest is *also a projection*.
-- "What changed this week" is a page whose binding is a time window over the
-- org's canonical memories, recomposed on cadence by the same worker, reviewed
-- through the same gate, read through the same doc_get an agent already has.
-- No parallel generator, no second delivery pipeline to rot.
ALTER TABLE documents DROP CONSTRAINT documents_kind_check;
ALTER TABLE documents ADD CONSTRAINT documents_kind_check
    CHECK (doc_kind IN ('entity_page', 'topic_page', 'runbook', 'onboarding', 'digest'));
