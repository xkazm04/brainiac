//! Usage: the vital signs (LIBRARY-PLAN L5).
//!
//! Counted by team, never by person — and that is a property of the SCHEMA
//! (no user column exists), which is why [`record_usage`]'s signature cannot
//! even express the mistake.

use anyhow::Result;
use brainiac_core::{LibraryArtifactKind, LibraryUsageEvent};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// Record one usage signal. Note what this function CANNOT take: a user id.
pub async fn record_usage(
    conn: &mut PgConnection,
    org_id: Uuid,
    artifact_kind: LibraryArtifactKind,
    artifact_id: Uuid,
    version: Option<&str>,
    event: LibraryUsageEvent,
    team_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO library_usage_events (org_id, artifact_kind, artifact_id, version, event, team_id)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(org_id)
    .bind(artifact_kind.as_str())
    .bind(artifact_id)
    .bind(version)
    .bind(event.as_str())
    .bind(team_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The Library's contribution to Knowledge Health (LIBRARY-PLAN follow-up 2).
/// One round trip: a leadership report must not cost six.
#[derive(Debug, Default, Clone, Copy)]
pub struct LibraryHealth {
    pub standards_adopted: i64,
    /// Candidates waiting at the gate — the Library's own review backlog.
    pub standards_at_gate: i64,
    /// How long the oldest candidate has waited, in seconds. The gate's SLA
    /// made visible: mining and agents both file into this queue, and a queue
    /// nobody works turns the whole intake into theatre.
    pub oldest_gate_secs: i64,
    /// Adopted rules the org has had time to use and hasn't touched in the
    /// dormancy window — deprecation candidates that surfaced themselves.
    pub standards_dormant: i64,
    pub skills_published: i64,
    /// Published skills nobody has fetched in the dormancy window.
    pub skills_dormant: i64,
}

/// Read the Library's health signals under the caller's RLS scope, like every
/// other number in the report.
///
/// The dormancy queries lean on [`brainiac_core::health::rule_is_dormant`]'s
/// rule expressed in SQL: an artifact must be OLDER than the window to be
/// called dormant. A rule adopted yesterday with no uses is new, not dead.
pub async fn health_signals(conn: &mut PgConnection, dormant_days: i64) -> Result<LibraryHealth> {
    let r = sqlx::query(
        "WITH used AS (
             SELECT artifact_kind, artifact_id
             FROM library_usage_events
             WHERE occurred_at > now() - make_interval(days => $1::int)
             GROUP BY artifact_kind, artifact_id
         )
         SELECT
           (SELECT count(*) FROM standards WHERE lifecycle = 'adopted')          AS adopted,
           (SELECT count(*) FROM standards WHERE lifecycle = 'proposed')         AS at_gate,
           (SELECT COALESCE(EXTRACT(EPOCH FROM now() - min(created_at)), 0)::bigint
              FROM standards WHERE lifecycle = 'proposed')                       AS oldest_gate,
           (SELECT count(*) FROM standards s
             WHERE s.lifecycle = 'adopted'
               AND s.adopted_at < now() - make_interval(days => $1::int)
               AND NOT EXISTS (SELECT 1 FROM used u
                                WHERE u.artifact_kind = 'standard' AND u.artifact_id = s.id))
                                                                                 AS dormant_rules,
           (SELECT count(*) FROM skills WHERE maturity = 'published')            AS skills_pub,
           (SELECT count(*) FROM skills sk
             WHERE sk.maturity = 'published'
               AND sk.updated_at < now() - make_interval(days => $1::int)
               AND NOT EXISTS (SELECT 1 FROM used u
                                WHERE u.artifact_kind = 'skill' AND u.artifact_id = sk.id))
                                                                                 AS dormant_skills",
    )
    .bind(dormant_days)
    .fetch_one(conn)
    .await?;
    Ok(LibraryHealth {
        standards_adopted: r.get("adopted"),
        standards_at_gate: r.get("at_gate"),
        oldest_gate_secs: r.get("oldest_gate"),
        standards_dormant: r.get("dormant_rules"),
        skills_published: r.get("skills_pub"),
        skills_dormant: r.get("dormant_skills"),
    })
}

/// Usage totals per team WITH team names resolved — what the console renders.
/// A NULL team (an org-scoped token) groups under `None`; the caller labels it.
pub async fn usage_named(
    conn: &mut PgConnection,
    artifact_kind: LibraryArtifactKind,
    artifact_id: Uuid,
) -> Result<Vec<(Option<String>, i64)>> {
    let rows = sqlx::query(
        "SELECT t.name AS team, count(*) AS uses
         FROM library_usage_events e
         LEFT JOIN teams t ON t.id = e.team_id
         WHERE e.artifact_kind = $1 AND e.artifact_id = $2
         GROUP BY t.name ORDER BY uses DESC",
    )
    .bind(artifact_kind.as_str())
    .bind(artifact_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get("team"), r.get("uses")))
        .collect())
}

/// Usage totals per team for one artifact — the console sparkline's query.
pub async fn usage_by_team(
    conn: &mut PgConnection,
    artifact_kind: LibraryArtifactKind,
    artifact_id: Uuid,
) -> Result<Vec<(Option<Uuid>, i64)>> {
    let rows = sqlx::query(
        "SELECT team_id, count(*) AS uses FROM library_usage_events
         WHERE artifact_kind = $1 AND artifact_id = $2
         GROUP BY team_id ORDER BY uses DESC",
    )
    .bind(artifact_kind.as_str())
    .bind(artifact_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get("team_id"), r.get("uses")))
        .collect())
}
