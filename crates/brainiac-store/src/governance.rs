//! Governance writes: sources, provenance, promotions, contradictions —
//! plus the status transition the promote worker applies.

use anyhow::Result;
use brainiac_core::{ActorKind, MemoryStatus, PolicyDecision};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use std::collections::HashMap;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn insert_source(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    team_id: Option<Uuid>,
    kind: &str,
    raw_text: &str,
    created_by: Option<Uuid>,
    project_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO sources (id, org_id, team_id, kind, raw_text, created_by, project_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(id)
    .bind(org_id)
    .bind(team_id)
    .bind(kind)
    .bind(raw_text)
    .bind(created_by)
    .bind(project_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Idempotent source insert: like [`insert_source`] but carries an
/// `idempotency_key`. Returns `Some(id)` when the row was written, `None` when
/// an existing keyed source already claims `(org_id, idempotency_key)` — the
/// caller then replays that source's original receipt via
/// [`keyed_source_id`]. The partial unique index scopes keys per org.
#[allow(clippy::too_many_arguments)] // two extra args over the base shape
pub async fn insert_source_idempotent(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    team_id: Option<Uuid>,
    kind: &str,
    raw_text: &str,
    created_by: Option<Uuid>,
    idempotency_key: &str,
    project_id: Option<Uuid>,
) -> Result<Option<Uuid>> {
    let row = sqlx::query(
        "INSERT INTO sources (id, org_id, team_id, kind, raw_text, created_by, idempotency_key, project_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (org_id, idempotency_key) WHERE idempotency_key IS NOT NULL
         DO NOTHING
         RETURNING id",
    )
    .bind(id)
    .bind(org_id)
    .bind(team_id)
    .bind(kind)
    .bind(raw_text)
    .bind(created_by)
    .bind(idempotency_key)
    .bind(project_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

/// The id of the source that already claims `(org_id, idempotency_key)` under
/// the caller's RLS — the replay target when [`insert_source_idempotent`]
/// reports a conflict. `None` only if the row is invisible to the caller
/// (impossible on the conflict path, where org_id matches the principal).
pub async fn keyed_source_id(
    conn: &mut PgConnection,
    idempotency_key: &str,
) -> Result<Option<Uuid>> {
    let row = sqlx::query("SELECT id FROM sources WHERE idempotency_key = $1")
        .bind(idempotency_key)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

/// (team_id, kind, raw_text, project_id). The kind rides along because the
/// extraction stage dispatches on it: a `manual` source is a pre-distilled
/// statement and ingests verbatim (F-3); transcripts/docs go to the model.
/// The project rides along so extraction stamps memories from the source row
/// — the single source of truth — rather than the queue payload (PR0).
pub async fn get_source_text(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<(Option<Uuid>, String, String, Option<Uuid>)>> {
    use sqlx::Row;
    let row = sqlx::query("SELECT team_id, kind, raw_text, project_id FROM sources WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(|r| {
        (
            r.get("team_id"),
            r.get::<String, _>("kind"),
            r.get::<Option<String>, _>("raw_text").unwrap_or_default(),
            r.get("project_id"),
        )
    }))
}

#[allow(clippy::too_many_arguments)] // mirrors the provenance row shape
pub async fn insert_provenance(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    actor_kind: ActorKind,
    actor_id: &str,
    model_ref: Option<&str>,
    source_id: Option<Uuid>,
    pipeline_run_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO provenance (id, org_id, actor_kind, actor_id, model_ref, source_id, pipeline_run_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(id)
    .bind(org_id)
    .bind(actor_kind.as_str())
    .bind(actor_id)
    .bind(model_ref)
    .bind(source_id)
    .bind(pipeline_run_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_promotion(
    conn: &mut PgConnection,
    org_id: Uuid,
    memory_id: Uuid,
    from: MemoryStatus,
    to: MemoryStatus,
    decision: PolicyDecision,
    rule: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO promotions (id, org_id, memory_id, from_status, to_status, policy_decision, policy_rule)
         VALUES ($1,$2,$3,$4::memory_status,$5::memory_status,$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(memory_id)
    .bind(from.as_str())
    .bind(to.as_str())
    .bind(decision.as_str())
    .bind(rule)
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns `true` when a row actually changed. A 0-row update means the memory
/// was deleted since the promotion was queued, or points outside the caller's RLS
/// scope — the caller MUST treat that as a failure (not commit a phantom
/// approval), which is why this reports it instead of silently returning Ok.
pub async fn set_memory_status(
    conn: &mut PgConnection,
    memory_id: Uuid,
    status: MemoryStatus,
) -> Result<bool> {
    let changed = sqlx::query(
        "UPDATE memories SET status = $2::memory_status, updated_at = now() WHERE id = $1",
    )
    .bind(memory_id)
    .bind(status.as_str())
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if !changed {
        // Nothing changed — do not mark_dirty (there is no new standing to
        // propagate) and let the caller reject.
        return Ok(false);
    }
    // §8: any page built on this memory is now suspect — a promotion added a
    // claim, a deprecation removed one. Marking here (rather than in each
    // caller) is what makes the guarantee unconditional: there is no way to
    // change a memory's standing through the governance path and forget the
    // wiki. Cheap when the org has no pages: an indexed lookup that matches
    // nothing.
    crate::documents::mark_dirty_for_memory(conn, memory_id).await?;
    Ok(true)
}

/// Apply a governance-decided supersession: the losing memory is deprecated,
/// pointed at the winner, and its validity window closed at `now()` so the
/// temporal dedupe (retrieval stage 5) serves ONLY the winner from now on —
/// the piece ARCHITECTURE.md §5 row 5 promised but no code path delivered.
///
/// The deprecation is recorded in `promotions`, the same status-transition
/// audit log every other status change flows through, stamped with who/what
/// applied it (`reviewer_id`): a human maintainer (`applied_by = Some`, decided
/// `approved`) or a policy actor (`None`, `auto_approved`). `rule` names the
/// trigger, e.g. `contradiction_supersede`.
///
/// Idempotent and RLS-safe: a memory already superseded, or not updatable
/// under the caller's scope, is left untouched and returns `false`.
pub async fn apply_supersession(
    conn: &mut PgConnection,
    org_id: Uuid,
    loser: Uuid,
    winner: Uuid,
    applied_by: Option<Uuid>,
    rule: &str,
) -> Result<bool> {
    // A memory cannot supersede itself: a self-merge would deprecate a live memory
    // and point it at itself, corrupting the temporal chain.
    if loser == winner {
        return Ok(false);
    }

    // Lock BOTH rows in a fixed order (by id) and read them together. This is the
    // existence + RLS gate for both sides AND the serialization point: two
    // opposite-direction applies (A->B and B->A) would otherwise each snapshot the
    // other as still-`superseded_by NULL` — the reads touch different rows so the
    // row locks never collide — and both commit, closing an A<->B cycle. Locking
    // the pair in id order makes them serialize.
    let rows = sqlx::query(
        "SELECT id, status::text AS status, superseded_by
         FROM memories WHERE id IN ($1, $2) ORDER BY id FOR UPDATE",
    )
    .bind(loser)
    .bind(winner)
    .fetch_all(&mut *conn)
    .await?;
    let loser_row = rows.iter().find(|r| r.get::<Uuid, _>("id") == loser);
    let winner_row = rows.iter().find(|r| r.get::<Uuid, _>("id") == winner);

    // Loser must be visible and not already superseded (a live supersession is
    // final) — otherwise an idempotent no-op.
    let Some(loser_row) = loser_row else {
        return Ok(false);
    };
    if loser_row.get::<Option<Uuid>, _>("superseded_by").is_some() {
        return Ok(false);
    }
    let from_status: String = loser_row.get("status");

    // Winner must be visible to the caller (absent ⇒ not in scope) and must not
    // already point back at the loser, which would close a two-node cycle.
    let Some(winner_row) = winner_row else {
        return Ok(false);
    };
    if winner_row.get::<Option<Uuid>, _>("superseded_by") == Some(loser) {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE memories
         SET valid_to = now(), superseded_by = $2,
             status = 'deprecated'::memory_status, updated_at = now()
         WHERE id = $1 AND superseded_by IS NULL",
    )
    .bind(loser)
    .bind(winner)
    .execute(&mut *conn)
    .await?;

    let decision = if applied_by.is_some() {
        "approved"
    } else {
        "auto_approved"
    };
    sqlx::query(
        "INSERT INTO promotions
            (id, org_id, memory_id, from_status, to_status,
             policy_decision, policy_rule, reviewer_id, reviewed_at)
         VALUES ($1,$2,$3,$4::memory_status,'deprecated'::memory_status,$5,$6,$7, now())",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(loser)
    .bind(from_status)
    .bind(decision)
    .bind(rule)
    .bind(applied_by)
    .execute(&mut *conn)
    .await?;

    // §8, the propagation that makes the wiki self-healing: a maintainer
    // resolved a contradiction, so every page citing the LOSER now states
    // something the org no longer believes — and every page citing the WINNER
    // may now be able to say more. Both recompose. This is the single call that
    // separates "a wiki with a review queue" from "a wiki that cannot rot".
    crate::documents::mark_dirty_for_memory(&mut *conn, loser).await?;
    crate::documents::mark_dirty_for_memory(&mut *conn, winner).await?;
    Ok(true)
}

/// Deprecate a memory outright: end its validity window at `now()` and drop it
/// out of retrieval, WITHOUT inventing a supersessor it does not have. This is
/// what a maintainer means by "the reporters are right" on a disputed memory —
/// the corpus is wrong and nothing replaces it.
///
/// Like [`apply_supersession`], and for the same reason, the transition is
/// recorded in `promotions` — the status-transition audit log every other status
/// change flows through — stamped with who applied it (`applied_by`: a human
/// maintainer ⇒ `approved`, a policy actor ⇒ `auto_approved`). `rule` names the
/// trigger, e.g. `feedback_deprecate`. The inline `UPDATE memories SET status =
/// 'deprecated'` this replaces skipped that row, which is exactly why the
/// permanent deprecation of an org memory was invisible to `/v1/audit`.
///
/// Idempotent and RLS-safe: a memory already deprecated or superseded, or not
/// updatable under the caller's scope, is left untouched and returns `false`.
pub async fn apply_deprecation(
    conn: &mut PgConnection,
    org_id: Uuid,
    memory_id: Uuid,
    applied_by: Option<Uuid>,
    rule: &str,
) -> Result<bool> {
    // Existence + RLS gate and the serialization point in one, mirroring
    // apply_supersession: the read is under the (visibility-scoped) SELECT
    // policy, so the org-only UPDATE policy can never deprecate a row the caller
    // cannot see.
    let Some(row) = sqlx::query(
        "SELECT status::text AS status, superseded_by
         FROM memories WHERE id = $1 FOR UPDATE",
    )
    .bind(memory_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false);
    };
    let from_status: String = row.get("status");
    // Already retired — by an earlier deprecation or by a supersession, whose
    // pointer this must not clobber.
    if from_status == "deprecated" || row.get::<Option<Uuid>, _>("superseded_by").is_some() {
        return Ok(false);
    }

    let changed = sqlx::query(
        "UPDATE memories
         SET status = 'deprecated'::memory_status, valid_to = now(), updated_at = now()
         WHERE id = $1 AND status <> 'deprecated'::memory_status",
    )
    .bind(memory_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if changed != 1 {
        return Ok(false);
    }

    let decision = if applied_by.is_some() {
        "approved"
    } else {
        "auto_approved"
    };
    sqlx::query(
        "INSERT INTO promotions
            (id, org_id, memory_id, from_status, to_status,
             policy_decision, policy_rule, reviewer_id, reviewed_at)
         VALUES ($1,$2,$3,$4::memory_status,'deprecated'::memory_status,$5,$6,$7, now())",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(memory_id)
    .bind(from_status)
    .bind(decision)
    .bind(rule)
    .bind(applied_by)
    .execute(&mut *conn)
    .await?;

    // §8: the org just stopped believing this. Every page built on it now states
    // something untrue and must recompose — the same propagation the supersession
    // path performs, for the same reason.
    crate::documents::mark_dirty_for_memory(&mut *conn, memory_id).await?;
    Ok(true)
}

/// One side of an OPEN contradiction as seen from a result memory: the
/// contradiction row and the memory it conflicts with.
#[derive(Debug, Clone)]
pub struct ContradictionFlag {
    pub contradiction_id: Uuid,
    /// The other memory in the pair (the one the result memory contradicts).
    pub counterpart_id: Uuid,
}

/// Open contradictions touching any memory in `memory_ids`, keyed by the
/// in-set memory to the counterpart it conflicts with. ONE batched query for
/// the whole result set — never an N+1 (mirrors [`feedback::trust_for`]).
///
/// RLS-safe both ways: contradictions is org-scoped, but the counterpart may
/// live in a team the caller cannot read. Joining BOTH sides to `memories`
/// makes the (team-scoped) memories SELECT policy filter out any pair whose
/// counterpart is invisible — so we never surface, nor even reveal the id of,
/// a memory the caller isn't allowed to see (no existence oracle).
pub async fn open_contradictions_for(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<ContradictionFlag>>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT ct.id AS contradiction_id, ct.memory_a, ct.memory_b
         FROM contradictions ct
         JOIN memories ma ON ma.id = ct.memory_a
         JOIN memories mb ON mb.id = ct.memory_b
         WHERE ct.status = 'open'
           AND (ct.memory_a = ANY($1) OR ct.memory_b = ANY($1))",
    )
    .bind(memory_ids)
    .fetch_all(conn)
    .await?;
    let wanted: std::collections::HashSet<Uuid> = memory_ids.iter().copied().collect();
    let mut out: HashMap<Uuid, Vec<ContradictionFlag>> = HashMap::new();
    for r in rows {
        let cid: Uuid = r.get("contradiction_id");
        let a: Uuid = r.get("memory_a");
        let b: Uuid = r.get("memory_b");
        // Flag whichever side(s) are actually in the result set, pointing at the
        // other side (guaranteed visible by the double join above).
        if wanted.contains(&a) {
            out.entry(a).or_default().push(ContradictionFlag {
                contradiction_id: cid,
                counterpart_id: b,
            });
        }
        if wanted.contains(&b) {
            out.entry(b).or_default().push(ContradictionFlag {
                contradiction_id: cid,
                counterpart_id: a,
            });
        }
    }
    Ok(out)
}

/// The evidence chain behind a memory: who/what recorded it, the model used,
/// when, and the originating source (if any). All fields but the memory's own
/// existence are optional — a memory may carry no provenance row, or a
/// provenance row with no source.
#[derive(Debug, Clone)]
pub struct ProvenanceView {
    pub actor_kind: Option<String>,
    pub actor_ref: Option<String>,
    pub model_ref: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub source_kind: Option<String>,
    /// The source's full raw text; the caller bounds it into an excerpt.
    pub source_text: Option<String>,
    /// The HUMAN who authored the originating source (`sources.created_by` →
    /// email), when there is one. This is the "who decided" the payload was
    /// missing: `actor_ref`/`model_ref` name the agent/model that *recorded* the
    /// memory, not the person whose session it came from.
    pub recorded_by: Option<String>,
    /// The memory's own temporal validity + governance state, so the attribution
    /// tool can answer "is it still true?" — `valid_to = None` means still live.
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

/// The provenance chain for one memory, RLS-scoped. `None` means the memory is
/// not visible to the caller — the SAME answer as a nonexistent memory, so the
/// tool gives no existence oracle. The memories SELECT policy gates the lead
/// row; provenance and sources are org-scoped and reached by LEFT JOIN, so a
/// visible memory with no provenance still resolves (all-None fields).
pub async fn provenance_for_memory(
    conn: &mut PgConnection,
    memory_id: Uuid,
) -> Result<Option<ProvenanceView>> {
    let row = sqlx::query(
        "SELECT p.actor_kind, p.actor_id, p.model_ref, p.created_at AS prov_created_at,
                s.kind AS source_kind, s.raw_text AS source_text,
                u.email AS recorded_by,
                m.valid_from, m.valid_to, m.status::text AS status
         FROM memories m
         LEFT JOIN provenance p ON p.id = m.provenance_id
         LEFT JOIN sources s ON s.id = p.source_id
         LEFT JOIN users u ON u.id = s.created_by
         WHERE m.id = $1",
    )
    .bind(memory_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| ProvenanceView {
        actor_kind: r.get("actor_kind"),
        actor_ref: r.get("actor_id"),
        model_ref: r.get("model_ref"),
        created_at: r.get("prov_created_at"),
        source_kind: r.get("source_kind"),
        source_text: r.get("source_text"),
        recorded_by: r.get("recorded_by"),
        valid_from: r.get("valid_from"),
        valid_to: r.get("valid_to"),
        status: r.get("status"),
    }))
}

/// A compact, resolved provenance reference for a served memory: who/what
/// recorded it and — when LLM-produced — the model. This is the citation handle
/// ARCHITECTURE.md §4.6 attaches to packed context lines; it is deliberately
/// leaner than [`ProvenanceView`] (no source excerpt), so a bundle can carry one
/// per entry cheaply.
#[derive(Debug, Clone)]
pub struct ProvenanceRef {
    pub actor_kind: Option<String>,
    pub actor_ref: Option<String>,
    pub model_ref: Option<String>,
}

/// Batched provenance refs for a result set — ONE query for N memories, never
/// an N+1 (mirrors [`open_contradictions_for`] / [`feedback::trust_for`]). Used
/// to attach a citation ref to every packed `memory_context` line without a
/// per-entry round-trip.
///
/// RLS-safe: the lead `memories` join means the caller's SELECT policy gates the
/// rows, so a memory the caller cannot see contributes nothing. The INNER join
/// to `provenance` means only memories that actually carry a provenance row
/// appear in the map — a visible memory with no provenance is simply absent, and
/// the caller treats absence as "no ref".
pub async fn provenance_refs_for(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<HashMap<Uuid, ProvenanceRef>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT m.id AS memory_id, p.actor_kind, p.actor_id, p.model_ref
         FROM memories m
         JOIN provenance p ON p.id = m.provenance_id
         WHERE m.id = ANY($1)",
    )
    .bind(memory_ids)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("memory_id"),
                ProvenanceRef {
                    actor_kind: r.get("actor_kind"),
                    actor_ref: r.get("actor_id"),
                    model_ref: r.get("model_ref"),
                },
            )
        })
        .collect())
}

pub async fn insert_contradiction(
    conn: &mut PgConnection,
    org_id: Uuid,
    memory_a: Uuid,
    memory_b: Uuid,
    detected_by: &str,
    suggested_resolution: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
         VALUES ($1,$2,$3,$4,$5,'open',$6)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(memory_a)
    .bind(memory_b)
    .bind(detected_by)
    .bind(suggested_resolution)
    .execute(conn)
    .await?;
    Ok(())
}
