-- F-4 (load/chainsonar field test): the skill-authoring path for agents.
--
-- LB4 gave agents `standard_propose` but no way to author a SKILL — three
-- scanners independently found a real runbook ("how to add a data provider")
-- and had no channel for it but to smuggle it in as a memory sentence. A skill
-- an agent proposes is a DRAFT awaiting a named-human signature (the same gate
-- publishing already enforces); nothing here serves it.
--
-- `proposed_by` records the authoring identity for two reasons: the maintainer
-- reviewing the draft must see who is asking (as they do for a proposed
-- standard's origin), and the proposal path rate-limits per author from this
-- column — no separate counter to drift. NULL means the console/maintainer
-- created it directly (the pre-existing path), never an agent.

ALTER TABLE skills ADD COLUMN proposed_by uuid;
