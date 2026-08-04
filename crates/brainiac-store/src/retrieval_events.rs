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
    /// One verbatim sample of how it was actually typed (the most recent ask).
    /// `query_norm` is the aggregate key; this is what a human reads, and what
    /// the gap scan re-probes the corpus with.
    pub query_sample: String,
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
                max(occurred_at)                    AS last_asked,
                -- The most recent verbatim phrasing, for a human to read and
                -- for the gap scan to re-probe with.
                (array_agg(query_text ORDER BY occurred_at DESC))[1] AS query_sample
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
            query_sample: r.get("query_sample"),
            asks: r.get("asks"),
            projects: r.get("projects"),
            best_score: r.get("best_score"),
            last_asked: r.get("last_asked"),
        })
        .collect())
}

/// Knowledge gaps (migration 0049) — the miss list turned into WORK.
///
/// Everything above this line is the append-only ledger: what agents asked and
/// what they got, INSERT/SELECT only, by grant. This module is the mutable
/// artifact DERIVED from it — one row per distinct unanswered question, with a
/// state of its own. The ledger is not touched; nothing here writes to it.
///
/// The lifecycle is deliberately the one the repo already uses for
/// machine-minted governance artifacts:
///
/// - **minted by a scan**, like `practice_divergences` — and, like divergences
///   after 0042, with a stable identity (`org_id, query_norm`) so re-running
///   the scan refreshes the row instead of minting a twin;
/// - **closed automatically** when the corpus can answer the question, which is
///   what makes resolution observable without anyone reporting it;
/// - **dismissed by a human**, like a contradiction — the one judgement a
///   machine should not make ("this question is noise, stop tracking it").
///
/// The ordering is 0043's, unchanged: breadth across projects, then volume.
/// One chatty repo asking a malformed query twenty times still ranks below a
/// question three different projects hit once each. That instinct is already
/// encoded; this module leans on it rather than inventing a second heuristic.
pub mod gaps {
    use anyhow::{Context, Result};
    use brainiac_core::embed::Embedder;
    use chrono::{DateTime, Utc};
    use sqlx::{PgConnection, PgPool, Row};
    use uuid::Uuid;

    /// How many OPEN gaps one scan re-probes. Each probe is a full hybrid
    /// retrieval, so this bounds the scan's cost; the backlog is ranked, and a
    /// gap below this depth simply waits for the next scan.
    const REPROBE_LIMIT: i64 = 200;

    /// A question the org has not answered, as a piece of work.
    #[derive(Debug, Clone)]
    pub struct KnowledgeGap {
        pub id: Uuid,
        pub query_norm: String,
        pub query_sample: String,
        /// `open` | `answered` | `dismissed`.
        pub status: String,
        pub asks: i64,
        pub projects: i64,
        pub best_score: Option<f64>,
        pub first_seen_at: DateTime<Utc>,
        pub last_asked_at: DateTime<Utc>,
        pub resolved_at: Option<DateTime<Utc>>,
        /// The memory that closed it — the observable proof the loop closed.
        pub resolved_memory_id: Option<Uuid>,
        pub resolution_note: Option<String>,
    }

    /// What one scan did. Every question the scan considered lands in exactly
    /// one of these buckets, so the numbers add up to the miss list's length
    /// plus the re-probed open gaps.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct GapScanOutcome {
        /// Gaps minted for the first time.
        pub opened: usize,
        /// Existing open gaps whose evidence (asks/projects/best score) moved.
        pub refreshed: usize,
        /// Open gaps the corpus can now answer — closed as `answered`.
        pub closed: usize,
        /// Misses NOT minted because the corpus can already answer them: the
        /// ledger recorded the failure before the memory that answers it
        /// existed. This is the dedupe against what the org already knows.
        pub skipped_answerable: usize,
        /// Misses NOT minted because a maintainer already dismissed them, or
        /// because they are already closed. Terminal states stay terminal.
        pub suppressed: usize,
    }

    const COLS: &str = "id, query_norm, query_sample, status, asks, projects, best_score, \
                        first_seen_at, last_asked_at, resolved_at, resolved_memory_id, \
                        resolution_note";

    fn row_to_gap(r: &sqlx::postgres::PgRow) -> KnowledgeGap {
        KnowledgeGap {
            id: r.get("id"),
            query_norm: r.get("query_norm"),
            query_sample: r.get("query_sample"),
            status: r.get("status"),
            asks: r.get::<i32, _>("asks") as i64,
            projects: r.get::<i32, _>("projects") as i64,
            best_score: r.get("best_score"),
            first_seen_at: r.get("first_seen_at"),
            last_asked_at: r.get("last_asked_at"),
            resolved_at: r.get("resolved_at"),
            resolved_memory_id: r.get("resolved_memory_id"),
            resolution_note: r.get("resolution_note"),
        }
    }

    /// The backlog, RLS-scoped like every read. `status = None` returns every
    /// state; ordering is breadth-then-volume for open work, newest-resolved
    /// first for the rest.
    pub async fn list(
        conn: &mut PgConnection,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<KnowledgeGap>> {
        let sql = format!(
            "SELECT {COLS}
               FROM knowledge_gaps
              WHERE ($1::text IS NULL OR status = $1)
              ORDER BY (status = 'open') DESC,
                       projects DESC, asks DESC, last_asked_at DESC
              LIMIT $2"
        );
        let rows = sqlx::query(&sql)
            .bind(status)
            .bind(limit)
            .fetch_all(conn)
            .await
            .context("list knowledge gaps")?;
        Ok(rows.iter().map(row_to_gap).collect())
    }

    pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<KnowledgeGap>> {
        let sql = format!("SELECT {COLS} FROM knowledge_gaps WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(conn)
            .await
            .context("get knowledge gap")?;
        Ok(row.as_ref().map(row_to_gap))
    }

    /// A maintainer's judgement that a question is not worth acquiring. The one
    /// decision the scan must not make for itself, and terminal — the scan
    /// never re-mints a dismissed gap, so a malformed one-off asked forever
    /// stays out of the backlog forever.
    ///
    /// Returns `false` if the gap does not exist, is not visible, or is no
    /// longer open (someone else got there first).
    pub async fn dismiss(
        conn: &mut PgConnection,
        id: Uuid,
        by: Uuid,
        note: Option<&str>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE knowledge_gaps
                SET status = 'dismissed', resolved_at = now(), resolved_by = $2,
                    resolution_note = $3, updated_at = now()
              WHERE id = $1 AND status = 'open'",
        )
        .bind(id)
        .bind(by)
        .bind(note)
        .execute(conn)
        .await
        .context("dismiss knowledge gap")?;
        Ok(done.rows_affected() == 1)
    }

    /// Can the corpus answer this question NOW? Returns the winning memory and
    /// its score, or `None`.
    ///
    /// This is one real hybrid retrieval — the same engine the agents use, not
    /// a lookalike — so "answerable" here means exactly what it means to them.
    ///
    /// It runs under the SCANNER's RLS scope, which is a genuine limitation
    /// worth stating rather than glossing: a console maintainer sees more than a
    /// project-scoped agent, so a gap can be closed by a memory the asking agent
    /// still cannot read. Closing on the narrower scope would be worse (the gap
    /// would never close for anyone) and re-probing per asker would mean
    /// resurrecting the per-person telemetry 0043 exists to make unrepresentable.
    /// The honest framing is the one the artifact uses: `answered` means the ORG
    /// can answer it.
    async fn probe(
        conn: &mut PgConnection,
        pool: &PgPool,
        embedder: &dyn Embedder,
        embedding_version: i32,
        query: &str,
        floor: f64,
    ) -> Result<Option<(Uuid, f64)>> {
        let req = crate::retrieval::RetrievalRequest {
            query: query.to_string(),
            k: 3,
            as_of: None,
            filters: Default::default(),
        };
        let hits = crate::retrieval::search(conn, pool, embedder, embedding_version, &req)
            .await
            .with_context(|| format!("probe corpus for gap query `{query}`"))?;
        Ok(hits
            .into_iter()
            .find(|h| h.score >= floor)
            .map(|h| (h.memory.id, h.score)))
    }

    /// Sweep the ledger's miss list into the backlog, and close the gaps the
    /// corpus has since learned to answer.
    ///
    /// Two halves, in this order:
    ///
    /// 1. **Close the loop.** Every OPEN gap is re-probed. This half is what
    ///    makes resolution observable, and it deliberately does NOT wait for
    ///    the question to be asked again: someone writes the missing memory,
    ///    the next scan notices, and the gap closes itself. Without it a gap
    ///    would only ever close if an agent happened to re-ask.
    /// 2. **Mint the new work**, from `top_unanswered` — but only for questions
    ///    the corpus still cannot answer (the ledger records a failure at the
    ///    time it happened; the corpus has moved since). Deduped against itself
    ///    by the `(org_id, query_norm)` identity, and against terminal states by
    ///    the `status = 'open'` guard on the upsert.
    ///
    /// Runs inside the caller's RLS-scoped transaction. The caller commits.
    // Same shape (and same reason) as `retrieval::engine`: the retrieval
    // dependencies — connection, pool, embedder, version — travel together and
    // bundling them into a struct here would only move the arity elsewhere.
    #[allow(clippy::too_many_arguments)]
    pub async fn scan(
        conn: &mut PgConnection,
        pool: &PgPool,
        embedder: &dyn Embedder,
        embedding_version: i32,
        org_id: Uuid,
        days: i32,
        floor: f64,
        limit: i64,
    ) -> Result<GapScanOutcome> {
        let mut out = GapScanOutcome::default();

        // (1) Close the loop.
        let open = list(conn, Some("open"), REPROBE_LIMIT).await?;
        for g in &open {
            let Some((memory_id, score)) = probe(
                conn,
                pool,
                embedder,
                embedding_version,
                &g.query_sample,
                floor,
            )
            .await?
            else {
                continue;
            };
            let done = sqlx::query(
                "UPDATE knowledge_gaps
                    SET status = 'answered', resolved_at = now(),
                        resolved_memory_id = $2, resolution_note = $3, updated_at = now()
                  WHERE id = $1 AND status = 'open'",
            )
            .bind(g.id)
            .bind(memory_id)
            .bind(format!("answerable: top hit scores {score:.6}"))
            .execute(&mut *conn)
            .await
            .context("close answered knowledge gap")?;
            if done.rows_affected() == 1 {
                out.closed += 1;
            }
        }

        // (2) Mint the new work.
        let misses = super::top_unanswered(conn, days, floor, limit).await?;
        for m in misses {
            if probe(
                conn,
                pool,
                embedder,
                embedding_version,
                &m.query_sample,
                floor,
            )
            .await?
            .is_some()
            {
                out.skipped_answerable += 1;
                continue;
            }
            // `xmax = 0` is Postgres' idiom for "this upsert INSERTed"; the
            // `status = 'open'` guard makes a conflict on a terminal row return
            // no row at all, which is the third outcome (suppressed).
            let row = sqlx::query(
                "INSERT INTO knowledge_gaps
                     (org_id, query_norm, query_sample, asks, projects, best_score, last_asked_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (org_id, query_norm) DO UPDATE
                    SET query_sample = EXCLUDED.query_sample,
                        asks         = EXCLUDED.asks,
                        projects     = EXCLUDED.projects,
                        best_score   = EXCLUDED.best_score,
                        last_asked_at = EXCLUDED.last_asked_at,
                        updated_at   = now()
                  WHERE knowledge_gaps.status = 'open'
                 RETURNING (xmax = 0) AS inserted",
            )
            .bind(org_id)
            .bind(&m.query_norm)
            .bind(&m.query_sample)
            .bind(i32::try_from(m.asks).unwrap_or(i32::MAX))
            .bind(i32::try_from(m.projects).unwrap_or(i32::MAX))
            .bind(m.best_score)
            .bind(m.last_asked)
            .fetch_optional(&mut *conn)
            .await
            .context("upsert knowledge gap")?;
            match row {
                Some(r) if r.get::<bool, _>("inserted") => out.opened += 1,
                Some(_) => out.refreshed += 1,
                None => out.suppressed += 1,
            }
        }
        Ok(out)
    }
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
