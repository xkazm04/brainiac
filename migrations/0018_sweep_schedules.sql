-- Sweep schedules — operator config for the periodic org-intelligence sweeps
-- (practice-divergence, knowledge-health snapshot). These sweeps are cross-org
-- operator actions (they run on the RLS-bypassing admin pool and loop every
-- org), so their SCHEDULE is global operator config, not per-org data: one row
-- per sweep kind, no org_id, no RLS. The worker's scheduler tick reads the due
-- rows and runs them; the admin-gated /v1/ops/sweeps endpoints let a UI turn a
-- sweep on/off, set its cadence, and trigger a run.
--
-- Status is recorded back onto the same row (last_run_at / last_status /
-- last_detail / last_duration_ms) so the UI can show "last scanned 2h ago — 7
-- clusters, 1 divergence" without a separate history table. next_run_at is the
-- scheduler's clock: when enabled and next_run_at <= now(), the row is due; a
-- "run now" simply sets next_run_at = now().

CREATE TABLE sweep_schedules (
    kind text PRIMARY KEY,              -- 'divergence' | 'health_snapshot'
    enabled boolean NOT NULL DEFAULT false,
    cadence_secs bigint NOT NULL,       -- interval between runs when enabled
    next_run_at timestamptz,            -- due when <= now(); NULL = not scheduled
    last_run_at timestamptz,
    last_status text,                   -- 'ok' | 'error' | 'running'
    last_detail text,                   -- human summary or error, bounded
    last_duration_ms bigint,
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Seed the two known sweeps, disabled, at a weekly cadence — an operator opts
-- in (and picks a cadence) from the UI; nothing runs on install by surprise.
INSERT INTO sweep_schedules (kind, cadence_secs) VALUES
    ('divergence',      604800),
    ('health_snapshot', 604800);

-- Global config, no RLS — but the console runs as brainiac_app (Store::connect
-- demotes every session), so it needs an explicit grant (0001's blanket grant
-- only covered then-existing tables). The admin pool (owner) reaches it anyway.
GRANT SELECT, INSERT, UPDATE, DELETE ON sweep_schedules TO brainiac_app;
