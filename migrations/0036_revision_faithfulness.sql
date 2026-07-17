-- 0036: citation-faithfulness verdicts on document revisions (KB follow-up).
--
-- The citation firewall catches an uncited paragraph and an invented id, but
-- not the subtler failure: a sentence that cites a REAL memory while
-- misstating it. That check existed only in the offline docs eval; this
-- column brings a sampled version of it to runtime.
--
-- NULL = not judged (the judge is best-effort and sampled — absence of a
-- verdict is not a verdict). Shape:
--   { "model_ref": "...", "checked": <n paragraphs>, "flagged":
--     [ { "excerpt": "...", "memory_id": "...", "note": "..." } ],
--     "judged_at": "<rfc3339>" }
--
-- Verdicts ride the revision, not the document: they describe one composed
-- text, and a recompose gets a fresh judgment or none.

ALTER TABLE document_revisions
    ADD COLUMN IF NOT EXISTS faithfulness jsonb;
