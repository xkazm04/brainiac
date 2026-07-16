-- LIBRARY-PLAN follow-up 1 (L8): standards render as knowledge-base pages.
--
-- A per-stack page projected from the org's ADOPTED rules, riding the existing
-- document layer: dirty-marking, revisions, the review gate, the health
-- circuit breaker, and the Confluence target — all unchanged. The Library's
-- judgment reaches the wiki people already read, and it cannot rot there for
-- the same reason no other page can.
--
-- The one thing this page does NOT do is compose. Every other doc_kind hands
-- its memories to a model and asks for prose; a standards page renders its
-- rules deterministically instead. A rule's statement is one sentence a named
-- human ratified — asking a model to re-word it would fork the org's own
-- commitment, which is the same reason `detail_md` is copied verbatim and
-- never re-typed (KB-PLAN D3). There is nothing here for a model to add and
-- everything for it to get subtly wrong.

ALTER TABLE documents DROP CONSTRAINT documents_kind_check;
ALTER TABLE documents ADD CONSTRAINT documents_kind_check
    CHECK (doc_kind IN ('entity_page', 'topic_page', 'runbook', 'onboarding',
                        'digest', 'standards_page'));
