-- Pipeline worker read scope.
--
-- Workers act with pipeline authority, not user authority: later stages
-- (contradict, promote) must read back the team-visible rows the extract
-- stage wrote for ANY team of the org. Team membership is SCIM-truth in
-- team_members and the synthetic worker is deliberately not a member of
-- anything, so the read policy gains an explicit, auditable escape:
-- a transaction that sets app.worker = 'on' (only Store::worker_tx does)
-- reads org + team tiers of its org. The PRIVATE tier stays excluded —
-- workers never read personal memories.

DROP POLICY memories_read ON memories;

CREATE POLICY memories_read ON memories FOR SELECT USING (
    org_id = current_setting('app.org_id')::uuid
    AND deleted_at IS NULL
    AND (
        visibility = 'org'
        OR (coalesce(current_setting('app.worker', true), 'off') = 'on'
            AND visibility <> 'private')
        OR (visibility = 'team' AND team_id IN
            (SELECT tm.team_id FROM team_members tm
             WHERE tm.user_id = current_setting('app.user_id')::uuid))
        OR (visibility = 'private' AND owner_user_id = current_setting('app.user_id')::uuid)
    )
);
