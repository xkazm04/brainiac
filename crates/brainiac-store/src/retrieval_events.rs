//! The demand ledger (migrations/0043_retrieval_events.sql) — what agents asked
//! for, and what they got back.
//!
//! Everything in this module is written on the *read* path, which means two
//! rules govern it:
//!
//! 1. **It must never fail a retrieval.** Recording demand is bookkeeping; an
//!    agent's answer does not depend on it. Callers use the fire-and-forget
//!    wrapper on the server side (the same posture as
//!    `record_library_usage_quietly` and `documents::record_read`).
//! 2. **Team and project, never person.** There is no user column, by schema
//!    shape — see the invariant note in 0043 and its ancestor in 0028.

use anyhow::Result;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// One retrieval, as it happened.
#[derive(Debug, Clone)]
pub struct NewRetrievalEvent {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// The API key that asked — the agent. `None` for env bootstrap tokens.
    pub token_id: Option<Uuid>,
    /// `http` | `mcp` — the door, not the species.
    pub surface: &'static str,
    /// The entry point: `search`, `memory_search`, `memory_context`.
    pub tool: &'static str,
    pub query_text: String,
    pub returned_n: i32,
    pub top_score: Option<f64>,
    pub withheld_contested: i32,
    pub latency_ms: Option<i32>,
    /// `(memory_id, rank, score)` for what was served, rank 0-based.
    pub served: Vec<(Uuid, i32, Option<f64>)>,
}

/// The aggregate form of a question.
///
/// Lowercased and whitespace-collapsed so "PSP retry  idempotency" and
/// "psp retry idempotency" are one demand row — but otherwise left ALONE.
/// Identifiers are the point of the corpus: stripping punctuation would fuse
/// `retry_policy` with `retry policy` and, worse, make `Store::connect` and
/// `store connect` indistinguishable in the gap list an operator acts on.
pub fn normalize_query(q: &str) -> String {
    q.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Append one event and its served set. Runs inside the caller's RLS-scoped
/// transaction, so `app.org_id` is already set.
pub async fn record(conn: &mut PgConnection, ev: &NewRetrievalEvent) -> Result<()> {
    sqlx::query(
        "INSERT INTO retrieval_events
            (id, org_id, team_id, project_id, token_id, surface, tool,
             query_text, query_norm, returned_n, top_score, withheld_contested, latency_ms)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(ev.id)
    .bind(ev.org_id)
    .bind(ev.team_id)
    .bind(ev.project_id)
    .bind(ev.token_id)
    .bind(ev.surface)
    .bind(ev.tool)
    .bind(&ev.query_text)
    .bind(normalize_query(&ev.query_text))
    .bind(ev.returned_n)
    .bind(ev.top_score)
    .bind(ev.withheld_contested)
    .bind(ev.latency_ms)
    .execute(&mut *conn)
    .await?;

    // One statement for the served set, not N — a retrieval is on the hot path
    // and this runs on every single one of them.
    if !ev.served.is_empty() {
        let ids: Vec<Uuid> = ev.served.iter().map(|(id, ..)| *id).collect();
        let ranks: Vec<i32> = ev.served.iter().map(|(_, r, _)| *r).collect();
        let scores: Vec<Option<f64>> = ev.served.iter().map(|(.., s)| *s).collect();
        sqlx::query(
            "INSERT INTO retrieval_event_memories (event_id, org_id, memory_id, rank, score)
             SELECT $1, $2, m.id, m.rank, m.score
               FROM UNNEST($3::uuid[], $4::int[], $5::double precision[])
                    AS m(id, rank, score)
             ON CONFLICT DO NOTHING",
        )
        .bind(ev.id)
        .bind(ev.org_id)
        .bind(&ids)
        .bind(&ranks)
        .bind(&scores)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Answered-rate and volume over a window, RLS-scoped like every read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DemandSummary {
    pub events: i64,
    /// Events that returned at least one hit at or above `floor`.
    pub answered: i64,
    /// Events where governance withheld at least one hit — demand pressure on
    /// the review queue, not a retrieval failure.
    pub withheld_events: i64,
    pub p50_latency_ms: Option<i32>,
    pub p95_latency_ms: Option<i32>,
}

impl DemandSummary {
    /// `None` when there is no demand yet — deliberately not 0.0, because "no
    /// questions asked" and "every question failed" must not render the same.
    pub fn answered_rate(&self) -> Option<f64> {
        (self.events > 0).then(|| self.answered as f64 / self.events as f64)
    }
}

pub async fn summary(conn: &mut PgConnection, days: i32, floor: f64) -> Result<DemandSummary> {
    let row = sqlx::query(
        "SELECT count(*)                                            AS events,
                count(*) FILTER (WHERE returned_n > 0
                                   AND top_score >= $2)             AS answered,
                count(*) FILTER (WHERE withheld_contested > 0)      AS withheld_events,
                percentile_disc(0.5) WITHIN GROUP (ORDER BY latency_ms)  AS p50,
                percentile_disc(0.95) WITHIN GROUP (ORDER BY latency_ms) AS p95
           FROM retrieval_events
          WHERE occurred_at > now() - make_interval(days => $1)
            AND arm = 'served'",
    )
    .bind(days)
    .bind(floor)
    .fetch_one(conn)
    .await?;
    Ok(DemandSummary {
        events: row.get("events"),
        answered: row.get("answered"),
        withheld_events: row.get("withheld_events"),
        p50_latency_ms: row.get("p50"),
        p95_latency_ms: row.get("p95"),
    })
}

/// A question the corpus keeps failing to answer.
#[derive(Debug, Clone)]
pub struct UnansweredQuery {
    pub query_norm: String,
    pub asks: i64,
    /// How many distinct projects asked — breadth beats raw count when ranking
    /// what to go acquire, because one chatty repo is not an org-wide gap.
    pub projects: i64,
    pub best_score: Option<f64>,
    pub last_asked: chrono::DateTime<chrono::Utc>,
}

/// The miss list: what agents asked and the corpus could not answer, ranked by
/// breadth then volume. This is the acquisition backlog.
pub async fn top_unanswered(
    conn: &mut PgConnection,
    days: i32,
    floor: f64,
    limit: i64,
) -> Result<Vec<UnansweredQuery>> {
    let rows = sqlx::query(
        "SELECT query_norm,
                count(*)                            AS asks,
                count(DISTINCT project_id)          AS projects,
                max(top_score)                      AS best_score,
                max(occurred_at)                    AS last_asked
           FROM retrieval_events
          WHERE occurred_at > now() - make_interval(days => $1)
            AND arm = 'served'
            AND (returned_n = 0 OR top_score < $2)
          GROUP BY query_norm
          ORDER BY count(DISTINCT project_id) DESC, count(*) DESC, max(occurred_at) DESC
          LIMIT $3",
    )
    .bind(days)
    .bind(floor)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| UnansweredQuery {
            query_norm: r.get("query_norm"),
            asks: r.get("asks"),
            projects: r.get("projects"),
            best_score: r.get("best_score"),
            last_asked: r.get("last_asked"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_folds_case_and_whitespace() {
        assert_eq!(
            normalize_query("  PSP   retry\tidempotency\n"),
            "psp retry idempotency"
        );
        assert_eq!(
            normalize_query("psp retry idempotency"),
            "psp retry idempotency"
        );
    }

    #[test]
    fn normalization_preserves_identifiers() {
        // The gap list is read by a human deciding what to go ingest. If
        // punctuation were stripped, `Store::connect` and `store connect` would
        // collapse into one row and the actionable one would be lost.
        assert_eq!(normalize_query("Store::connect"), "store::connect");
        assert_ne!(
            normalize_query("retry_policy"),
            normalize_query("retry policy")
        );
        assert_eq!(normalize_query("GET /v1/memories"), "get /v1/memories");
    }

    #[test]
    fn answered_rate_distinguishes_no_demand_from_total_failure() {
        assert_eq!(DemandSummary::default().answered_rate(), None);
        let all_missed = DemandSummary {
            events: 10,
            ..Default::default()
        };
        assert_eq!(all_missed.answered_rate(), Some(0.0));
        let half = DemandSummary {
            events: 10,
            answered: 5,
            ..Default::default()
        };
        assert_eq!(half.answered_rate(), Some(0.5));
    }
}
