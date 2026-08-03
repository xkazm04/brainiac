-- Sibling-dispute lookup: "which OTHER open contradictions argue about this
-- memory?"
--
-- Resolving a contradiction now settles the other open disputes that referenced
-- the memory it just retired (they became unanswerable, and while open they
-- withheld their live counterparts from serving). That runs INSIDE the
-- maintainer's resolve request — an interactive path — and it looks the rows up
-- by memory, on either side of the pair. `contradictions` had no index at all,
-- so every resolve did a sequential scan of the whole org's dispute history,
-- which only grows.
--
-- Two partial indexes rather than one composite: the predicate is
-- `(memory_a = $1 OR memory_b = $1)`, which Postgres serves as a BitmapOr over
-- two single-column indexes — a composite on (memory_a, memory_b) could only
-- ever serve the memory_a side.
--
-- `WHERE status = 'open'` keeps them small and keeps them relevant: settled
-- disputes accumulate forever and are never the target of this lookup. Postgres
-- can use a partial index only when the query's own predicate implies the
-- index's, and both call sites (sibling settlement and the carry-forward
-- duplicate guard) filter on `status = 'open'` explicitly.
CREATE INDEX IF NOT EXISTS idx_contradictions_open_memory_a
    ON contradictions (memory_a) WHERE status = 'open';
CREATE INDEX IF NOT EXISTS idx_contradictions_open_memory_b
    ON contradictions (memory_b) WHERE status = 'open';
