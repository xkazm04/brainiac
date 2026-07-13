-- Direction 2: make pipeline_runs a real per-source run record.
--
-- The v0 worker runs the whole extract -> resolve -> contradict -> promote
-- chain for one source per job, but pipeline_runs only ever held
-- id/org_id/stage/status/detail/timestamps and was never written — so
-- Memory→provenance.pipeline_run_id was always NULL and GET /v1/pipeline/runs
-- read an empty table. This migration is purely ADDITIVE: it widens the table
-- so one row per processed source can carry the stage stats, extract cost
-- counters, and the model ref, alongside the existing outcome/timing columns.
-- Every column is nullable or defaulted, so existing rows (there are none) and
-- the existing console SELECT keep working unchanged.

ALTER TABLE pipeline_runs
    ADD COLUMN source_id             uuid,
    ADD COLUMN model_ref             text,
    -- stage stats
    ADD COLUMN memories_written      int NOT NULL DEFAULT 0,
    ADD COLUMN entities_created      int NOT NULL DEFAULT 0,
    ADD COLUMN entities_resolved     int NOT NULL DEFAULT 0,
    ADD COLUMN contradictions_opened int NOT NULL DEFAULT 0,
    ADD COLUMN auto_promoted         int NOT NULL DEFAULT 0,
    ADD COLUMN needs_review          int NOT NULL DEFAULT 0,
    -- extract cost / resilience counters
    ADD COLUMN chunks                int NOT NULL DEFAULT 0,
    ADD COLUMN llm_calls             int NOT NULL DEFAULT 0,
    ADD COLUMN repairs               int NOT NULL DEFAULT 0,
    ADD COLUMN parse_failures        int NOT NULL DEFAULT 0,
    ADD COLUMN deduped               int NOT NULL DEFAULT 0;
