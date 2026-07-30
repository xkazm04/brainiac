-- A divergence needs a STABLE identity across sweeps.
--
-- `divergence::scan_org` used to DELETE every row for the org and re-INSERT the
-- fresh verdicts, so a divergence that persists across two sweeps came back with
-- a brand-new `id`. Two things depend on that id and both were broken by it:
--
--   1. `library_sweep::blocked()` dedupes mined standard candidates on
--      `standard_provenance.ref_id` = the divergence id. A new id every sweep
--      reads as a new divergence, so the mining sweep minted ANOTHER standard
--      candidate for the same practice on every single run.
--   2. Every already-ratified standard's `standard_provenance` row pointed at an
--      id that no longer existed — the provenance trail from a rule back to the
--      evidence that justified it was dangling.
--
-- The natural key is what the adjudicator actually judges: one verdict per
-- (org, canonical entity, axis) — see the `for axis in axes` loop, which visits
-- each pair at most once per scan. Making that a unique key lets the sweep UPSERT
-- and keep the row's identity.
--
-- `canonical_id` stays nullable (0016 declared it so), and Postgres treats NULLs
-- as distinct, which would let NULL-anchored rows duplicate freely. The index is
-- therefore PARTIAL. `scan_org` only ever writes anchored rows, so this covers
-- every row the sweep produces; a hand-inserted unanchored row remains legal and
-- simply is not deduped.

-- Collapse any pre-existing duplicates before the index goes on, newest kept.
-- Under the old delete-all-then-insert this should be a no-op; it is here so the
-- migration cannot fail on a database that was written to by other means.
DELETE FROM practice_divergences pd
 USING practice_divergences newer
 WHERE pd.canonical_id IS NOT NULL
   AND pd.org_id = newer.org_id
   AND pd.canonical_id = newer.canonical_id
   AND pd.axis = newer.axis
   AND (newer.detected_at, newer.id) > (pd.detected_at, pd.id);

CREATE UNIQUE INDEX idx_pd_identity
    ON practice_divergences (org_id, canonical_id, axis)
    WHERE canonical_id IS NOT NULL;
