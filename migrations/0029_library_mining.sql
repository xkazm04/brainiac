-- LB3 (docs/LIBRARY-PLAN.md): passive mining — the substrate deltas.
--
-- 1. REJECTION IS KNOWLEDGE. The mining sweep must dedup against candidates a
--    maintainer already said no to, or it re-proposes the same rejected idea
--    on every run and turns triage into a treadmill. So `rejected` becomes a
--    real lifecycle state (proposed → rejected, terminal like deprecated)
--    instead of a delete: the row and its provenance stay, and the dedup
--    window reads them.
--
-- 2. The attribution trigger narrows to the lifecycles that ASK to be
--    followed ('adopted', 'deprecated' — a retired rule was once binding).
--    Rejecting an evidence-free candidate must be possible: refusing to let a
--    maintainer say no to a rule with no provenance would be exactly
--    backwards.
--
-- 3. The 'library' sweep joins the schedule table, disabled, weekly — an
--    operator opts in; nothing mines by surprise.

ALTER TABLE standards DROP CONSTRAINT standards_lifecycle_check;
ALTER TABLE standards ADD CONSTRAINT standards_lifecycle_check
    CHECK (lifecycle IN ('proposed', 'adopted', 'deprecated', 'rejected'));

-- Leaving `proposed` still requires a named human on record — including a
-- rejection ("who said no" is the question the next proposer asks).
-- (standards_adoption_check already enforces adopted_by for every
-- non-proposed lifecycle; 'rejected' inherits it unchanged.)

CREATE OR REPLACE FUNCTION standards_attribution_check() RETURNS trigger AS $$
BEGIN
    IF NEW.lifecycle IN ('adopted', 'deprecated') AND NEW.decreed_by IS NULL THEN
        IF NOT EXISTS (SELECT 1 FROM standard_provenance sp WHERE sp.standard_id = NEW.id) THEN
            RAISE EXCEPTION 'standard % cannot leave proposed without provenance or a named decree', NEW.id
                USING ERRCODE = 'check_violation';
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

INSERT INTO sweep_schedules (kind, cadence_secs) VALUES ('library', 604800);
