-- The raw-memory TTL sweep (UAT P0.3; KB-PLAN follow-up #6).
--
-- Auto-capture industrializes RAW memories; review is human-rate-limited. The
-- UAT's central negative finding was what happens when those two rates diverge:
-- the backlog keeps being SERVED AS TRUTH (default retrieval excludes only
-- `rejected`), and nothing goes red. A raw memory nobody has looked at in a
-- month is not a candidate — it is sediment, and it sits inside every agent's
-- retrieval results carrying implied authority.
--
-- The sweep expires raw memories past their TTL to `rejected` (dropping them
-- from retrieval, preserving them for audit), each with a promotions row naming
-- the sweep — the same audit trail every other status transition leaves.
--
-- Seeded DISABLED like every sweep: a janitor that turns itself on in an org
-- that wasn't expecting it would be deleting-adjacent behaviour. Daily cadence
-- once an operator enables it.
INSERT INTO sweep_schedules (kind, enabled, cadence_secs)
VALUES ('raw_ttl', false, 86400)
ON CONFLICT (kind) DO NOTHING;

-- The alert sweep (UAT P0.4): per-org health breaches (stalled review queue,
-- currency below the publish floor, open cross-team contradictions, pages not
-- recomposing) pushed to the operator webhook (BRAINIAC_ALERT_WEBHOOK_URL).
-- The cadence IS the re-alert debounce: a standing breach re-pages once per
-- cadence, on purpose. Disabled until an operator turns it on.
INSERT INTO sweep_schedules (kind, enabled, cadence_secs)
VALUES ('alerts', false, 21600)
ON CONFLICT (kind) DO NOTHING;
