//! Browse the memory archive: filter, order, page, and facet — in the database.
//!
//! The console used to fetch the whole RLS-scoped corpus and do all of this in
//! the browser. At 1–10k memories that is O(corpus) on the wire and per
//! keystroke. This module is the server-side replacement, and it is the SINGLE
//! source of the query: the REST handler (`console.rs::memories_list`) and the
//! MCP tool (`mcp.rs::memory_list`) both call these fns, so the two surfaces can
//! never disagree about what a filter means.
//!
//! Generalizes the disputes pattern (`feedback.rs`), and keeps its invariants:
//!   * `count` uses the EXACT same filter as `list`, so the badge and the rows
//!     never disagree about the total.
//!   * `facets` are cross-filtered: for dimension D, counted over the search +
//!     every filter EXCEPT D — computed by binding D's own parameter to NULL in
//!     the one shared WHERE clause, so a dimension never constrains its own menu
//!     (the menu can widen back out) while still reflecting the other filters.
//!   * Every filter is bound `($n IS NULL OR col = $n)` — one static SQL string,
//!     one query plan, no injection surface, absent filters cost nothing.
//!   * All reads run inside the caller's RLS-scoped transaction.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::feedback::Facet;

/// What the caller is narrowing to. Every field is optional; `None` means "do
/// not constrain this dimension".
#[derive(Debug, Default, Clone)]
pub struct MemoryFilter {
    /// Full-text query (websearch syntax over content, OR a title substring).
    pub q: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub team_id: Option<Uuid>,
    pub visibility: Option<String>,
    /// Project facet (PROJECT-PLAN PR2): a project id as a string, or the
    /// sentinel `"none"` for org-shared rows (`project_id IS NULL`). A string
    /// (not `Option<Uuid>`) because org-shared is a SELECTABLE bucket, and
    /// "no filter" and "the null bucket" must be distinct wire values.
    pub project: Option<String>,
    /// The archive's time travel: rows VALID at this instant, including ones
    /// since deprecated. `None` = as the corpus stands now.
    pub as_of: Option<DateTime<Utc>>,
}

/// How the page is ordered. `Recent` (created_at DESC) is the default and the
/// only order with an index tuned for it; the valid_* orders are the archive's
/// "sort the Valid column" control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySort {
    Recent,
    ValidFromAsc,
    ValidFromDesc,
    ValidToAsc,
    ValidToDesc,
}

impl MemorySort {
    /// Parse a `sort`+`dir` pair from the query string. Unknown → `Recent`
    /// (never an error: a bad sort is a cosmetic default, not a 400).
    pub fn parse(sort: Option<&str>, dir: Option<&str>) -> Self {
        let asc = dir == Some("asc");
        match sort {
            Some("valid_from") if asc => Self::ValidFromAsc,
            Some("valid_from") => Self::ValidFromDesc,
            Some("valid_to") if asc => Self::ValidToAsc,
            Some("valid_to") => Self::ValidToDesc,
            _ => Self::Recent,
        }
    }

    /// The ORDER BY body. Always tie-broken by `m.id` so paging is stable, and
    /// NULLS LAST so open-ended validity windows do not float to the top.
    fn order_sql(self) -> &'static str {
        match self {
            Self::Recent => "m.created_at DESC, m.id",
            Self::ValidFromAsc => "m.valid_from ASC NULLS LAST, m.id",
            Self::ValidFromDesc => "m.valid_from DESC NULLS LAST, m.id",
            Self::ValidToAsc => "m.valid_to ASC NULLS LAST, m.id",
            Self::ValidToDesc => "m.valid_to DESC NULLS LAST, m.id",
        }
    }
}

/// One archive row. Mirrors the columns the REST `MemoryRow` needs; timestamps
/// stay `DateTime` here and are stringified at the HTTP edge.
#[derive(Debug, Clone)]
pub struct MemoryListRow {
    pub id: Uuid,
    pub title: Option<String>,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub visibility: String,
    pub team: String,
    pub team_id: Uuid,
    /// Project display name; `None` = org-shared.
    pub project: Option<String>,
    pub project_id: Option<Uuid>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub superseded_by: Option<Uuid>,
    pub created_at: Option<DateTime<Utc>>,
    pub confidence: Option<f32>,
}

/// The five facet dimensions of the archive, cross-filtered.
#[derive(Debug, Clone, Default)]
pub struct MemoryFacets {
    pub kinds: Vec<Facet>,
    pub statuses: Vec<Facet>,
    pub teams: Vec<Facet>,
    pub visibilities: Vec<Facet>,
    /// Value is the project id (or `"none"`), label the project name (or
    /// `"org-shared"`). The null bucket is a first-class option: org-shared
    /// knowledge is a tier, not missing data (PROJECT-PLAN principle 1).
    pub projects: Vec<Facet>,
}

/// One row of the as-of skeleton: enough to place a memory on the time axis and
/// decide whether it was true at an instant, and nothing else. The whole
/// visible corpus of these is a few tens of bytes per row, so the archive can
/// hold it in the browser and scrub instantly while the heavy content pages
/// server-side.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValiditySkel {
    pub id: Uuid,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub status: String,
}

/// The shared WHERE. Bind order is FIXED and every caller matches it:
///   $1 q, $2 kind, $3 status, $4 team_id, $5 visibility, $6 as_of, $7 project.
/// A facet query for dimension D passes NULL for D's own bind (see `facets`),
/// which is why every clause is `($n IS NULL OR …)` — a NULL simply drops out.
/// `$7` is text, not uuid: `"none"` selects the org-shared bucket
/// (`project_id IS NULL`), any other value matches the id exactly.
const FILTER: &str = "
    WHERE ($1::text IS NULL OR (
             m.content_fts @@ websearch_to_tsquery('english', $1)
          OR m.content_fts @@ websearch_to_tsquery('simple',  $1)
          OR m.title ILIKE '%' || $1 || '%'))
      AND ($2::text IS NULL OR m.kind = $2)
      AND ($3::text IS NULL OR m.status::text = $3)
      AND ($4::uuid IS NULL OR m.team_id = $4)
      AND ($5::text IS NULL OR m.visibility::text = $5)
      AND ($6::timestamptz IS NULL OR
            ((m.valid_from IS NULL OR m.valid_from <= $6)
             AND (m.valid_to IS NULL OR m.valid_to > $6)))
      AND ($7::text IS NULL OR
            CASE WHEN $7 = 'none' THEN m.project_id IS NULL
                 ELSE m.project_id::text = $7 END)";

/// Bind the seven filter parameters in the fixed order. `mask_out` nulls one
/// dimension's bind so a facet query leaves that dimension unconstrained.
fn bind_filter<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    f: &'q MemoryFilter,
    mask_out: Dim,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query
        .bind(f.q.as_deref())
        .bind(if mask_out == Dim::Kind { None } else { f.kind.as_deref() })
        .bind(if mask_out == Dim::Status { None } else { f.status.as_deref() })
        .bind(if mask_out == Dim::Team { None } else { f.team_id })
        .bind(if mask_out == Dim::Visibility { None } else { f.visibility.as_deref() })
        .bind(f.as_of)
        .bind(if mask_out == Dim::Project { None } else { f.project.as_deref() })
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Dim {
    None,
    Kind,
    Status,
    Team,
    Visibility,
    Project,
}

/// One page of the archive, filtered and ordered.
pub async fn list(
    conn: &mut PgConnection,
    f: &MemoryFilter,
    sort: MemorySort,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemoryListRow>> {
    let sql = format!(
        "SELECT m.id, m.title, m.content, m.kind, m.status::text AS status,
                m.visibility::text AS visibility, t.name AS team, m.team_id,
                p.name AS project, m.project_id,
                m.valid_from, m.valid_to, m.superseded_by, m.created_at, m.confidence
         FROM memories m
         JOIN teams t ON t.id = m.team_id
         LEFT JOIN projects p ON p.id = m.project_id
         {FILTER}
         ORDER BY {}
         LIMIT $8 OFFSET $9",
        sort.order_sql()
    );
    let rows = bind_filter(sqlx::query(&sql), f, Dim::None)
        .bind(limit)
        .bind(offset)
        .fetch_all(conn)
        .await?;
    Ok(rows
        .iter()
        .map(|r| MemoryListRow {
            id: r.get("id"),
            title: r.get("title"),
            content: r.get("content"),
            kind: r.get("kind"),
            status: r.get("status"),
            visibility: r.get("visibility"),
            team: r.get("team"),
            team_id: r.get("team_id"),
            project: r.get("project"),
            project_id: r.get("project_id"),
            valid_from: r.get("valid_from"),
            valid_to: r.get("valid_to"),
            superseded_by: r.get("superseded_by"),
            created_at: r.get("created_at"),
            confidence: r.get("confidence"),
        })
        .collect())
}

/// The filtered total — the real depth behind the page. Uses the EXACT same
/// FILTER, so it can never disagree with `list` about what matches.
pub async fn count(conn: &mut PgConnection, f: &MemoryFilter) -> Result<i64> {
    let sql = format!("SELECT count(*) AS n FROM memories m {FILTER}");
    let n: i64 = bind_filter(sqlx::query(&sql), f, Dim::None)
        .fetch_one(conn)
        .await?
        .get("n");
    Ok(n)
}

/// The facet menu, cross-filtered. Four small grouped passes — one per
/// dimension — each nulling its own bind so its menu reflects the OTHER filters
/// but stays free to widen back out. Team facets carry the team name; the rest
/// label with the value itself.
pub async fn facets(conn: &mut PgConnection, f: &MemoryFilter) -> Result<MemoryFacets> {
    async fn simple(
        conn: &mut PgConnection,
        f: &MemoryFilter,
        col: &str,
        dim: Dim,
    ) -> Result<Vec<Facet>> {
        let sql = format!(
            "SELECT {col} AS value, count(*) AS n
             FROM memories m {FILTER}
             GROUP BY {col} ORDER BY n DESC, value ASC"
        );
        let rows = bind_filter(sqlx::query(&sql), f, dim).fetch_all(conn).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let value: String = r.get("value");
                Facet {
                    label: value.clone(),
                    value,
                    count: r.get("n"),
                }
            })
            .collect())
    }

    let kinds = simple(conn, f, "m.kind", Dim::Kind).await?;
    let statuses = simple(conn, f, "m.status::text", Dim::Status).await?;
    let visibilities = simple(conn, f, "m.visibility::text", Dim::Visibility).await?;

    // Teams need the name for the label, so this one joins.
    let team_sql = format!(
        "SELECT m.team_id AS value, t.name AS label, count(*) AS n
         FROM memories m JOIN teams t ON t.id = m.team_id
         {FILTER}
         GROUP BY m.team_id, t.name ORDER BY n DESC, t.name ASC"
    );
    let team_rows = bind_filter(sqlx::query(&team_sql), f, Dim::Team)
        .fetch_all(&mut *conn)
        .await?;
    let teams = team_rows
        .iter()
        .map(|r| Facet {
            value: r.get::<Uuid, _>("value").to_string(),
            label: r.get("label"),
            count: r.get("n"),
        })
        .collect();

    // Projects: LEFT JOIN so the org-shared bucket (project_id NULL) is a
    // first-class option, valued `"none"` and labeled `org-shared`.
    let project_sql = format!(
        "SELECT coalesce(m.project_id::text, 'none') AS value,
                coalesce(p.name, 'org-shared') AS label, count(*) AS n
         FROM memories m LEFT JOIN projects p ON p.id = m.project_id
         {FILTER}
         GROUP BY 1, 2 ORDER BY n DESC, label ASC"
    );
    let project_rows = bind_filter(sqlx::query(&project_sql), f, Dim::Project)
        .fetch_all(conn)
        .await?;
    let projects = project_rows
        .iter()
        .map(|r| Facet {
            value: r.get("value"),
            label: r.get("label"),
            count: r.get("n"),
        })
        .collect();

    Ok(MemoryFacets {
        kinds,
        statuses,
        teams,
        visibilities,
        projects,
    })
}

/// The as-of skeleton: `{id, valid_from, valid_to, status}` for every memory
/// matching the filter EXCEPT as_of (time travel is applied in the browser over
/// this skeleton, so the skeleton itself must span the whole timeline). Ordered
/// by id for a stable payload. Deliberately tiny — no content, no title.
pub async fn validity_skeleton(
    conn: &mut PgConnection,
    f: &MemoryFilter,
) -> Result<Vec<ValiditySkel>> {
    // as_of is $6; passing NULL there returns the full timeline. Reuse the
    // shared bind with as_of forced off by cloning the filter without it.
    let base = MemoryFilter {
        as_of: None,
        ..f.clone()
    };
    let sql = format!(
        "SELECT m.id, m.valid_from, m.valid_to, m.status::text AS status
         FROM memories m {FILTER}
         ORDER BY m.id"
    );
    let rows = bind_filter(sqlx::query(&sql), &base, Dim::None)
        .fetch_all(conn)
        .await?;
    Ok(rows
        .iter()
        .map(|r| ValiditySkel {
            id: r.get("id"),
            valid_from: r.get("valid_from"),
            valid_to: r.get("valid_to"),
            status: r.get("status"),
        })
        .collect())
}
