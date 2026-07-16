-- LB4 (docs/LIBRARY-PLAN.md): active contribution — the substrate delta.
--
-- Every standard now names its ORIGIN: a human at the console, the mining
-- sweep, or an agent proposing mid-session. Triage is about to receive
-- machine-authored candidates at machine speed, and a maintainer deciding
-- whether to trust a rule must see who is asking without archaeology. A
-- column, not a convention: the API sets it, the console renders it, and
-- nothing can forget to.

ALTER TABLE standards ADD COLUMN origin text NOT NULL DEFAULT 'human'
    CONSTRAINT standards_origin_check CHECK (origin IN ('human', 'sweep', 'agent'));

-- Existing rows: everything so far was created by a human path (the console
-- gate, the LB0 bridge) or the mining sweep; rows carrying divergence/memory
-- provenance with no versioned author are the sweep's. Best-effort backfill —
-- the column is authoritative only from this migration forward.
UPDATE standards s SET origin = 'sweep'
WHERE NOT EXISTS (
    SELECT 1 FROM standard_versions v
    WHERE v.standard_id = s.id AND v.rev = 1 AND v.author IS NOT NULL
);
