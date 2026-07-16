//! Memory feedback (migrations/0004, 0005): the retrieval loop's return
//! channel and its triage side.
//!
//! Agents and operators record helpful / wrong / outdated verdicts on the
//! memories they were served. A `helpful` verdict asserts nothing to fix; a
//! `wrong` or `outdated` verdict is a **claim against the corpus** that stays
//! OPEN until a maintainer answers it — by re-verifying the memory,
//! deprecating it, or dismissing the report. [`flagged`] is that queue.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};
use std::collections::HashMap;
use uuid::Uuid;

pub const VERDICTS: [&str; 3] = ["helpful", "wrong", "outdated"];
/// Verdicts that assert something is wrong with the memory, so they open a
/// claim a maintainer must close.
pub const NEGATIVE_VERDICTS: [&str; 2] = ["wrong", "outdated"];
pub const RESOLUTIONS: [&str; 3] = ["reverified", "deprecated", "dismissed"];

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    memory_id: Uuid,
    user_id: Uuid,
    verdict: &str,
    note: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO memory_feedback (id, org_id, memory_id, user_id, verdict, note)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(org_id)
    .bind(memory_id)
    .bind(user_id)
    .bind(verdict)
    .bind(note)
    .execute(conn)
    .await?;
    Ok(())
}

/// Verdict counts for one memory (RLS-scoped like every read).
pub async fn summary(conn: &mut PgConnection, memory_id: Uuid) -> Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        "SELECT verdict, count(*) AS n FROM memory_feedback
         WHERE memory_id = $1 GROUP BY verdict ORDER BY verdict",
    )
    .bind(memory_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<String, _>("verdict"), r.get::<i64, _>("n")))
        .collect())
}

/// Trust signal attached to a served memory: what readers reported back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trust {
    pub helpful: i64,
    pub wrong: i64,
    pub outdated: i64,
    /// Unresolved wrong/outdated claims — the "someone disputes this" flag.
    pub open_claims: i64,
}

impl Trust {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn disputed(&self) -> bool {
        self.open_claims > 0
    }
}

/// Batch trust lookup for a result set — one query for N memories, so
/// attaching trust to search results never becomes an N+1.
pub async fn trust_for(
    conn: &mut PgConnection,
    memory_ids: &[Uuid],
) -> Result<HashMap<Uuid, Trust>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT memory_id,
                count(*) FILTER (WHERE verdict = 'helpful')  AS helpful,
                count(*) FILTER (WHERE verdict = 'wrong')    AS wrong,
                count(*) FILTER (WHERE verdict = 'outdated') AS outdated,
                count(*) FILTER (WHERE resolved_at IS NULL
                                   AND verdict IN ('wrong', 'outdated')) AS open_claims
         FROM memory_feedback
         WHERE memory_id = ANY($1)
         GROUP BY memory_id",
    )
    .bind(memory_ids)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("memory_id"),
                Trust {
                    helpful: r.get("helpful"),
                    wrong: r.get("wrong"),
                    outdated: r.get("outdated"),
                    open_claims: r.get("open_claims"),
                },
            )
        })
        .collect())
}

/// One open claim, carrying the reporter who filed it.
///
/// A bare tally is not evidence. "Five people say this is wrong" is a very
/// different fact depending on whether it is five engineers on the owning team
/// or one agent firing the same verdict five times, and that distinction
/// decides whether an org memory gets permanently deprecated. The reporter is
/// therefore part of the claim, not a detail behind a second lookup.
///
/// Age, not a timestamp: the whole payload speaks in seconds-since (see
/// [`FlaggedMemory::oldest_claim_secs`]), which is what the bench renders and
/// what keeps the wire free of session-timezone dependence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimReport {
    /// `wrong` (never true) or `outdated` (stopped being true).
    pub verdict: String,
    /// What the reporter wrote, if anything — the verdict alone is a signal.
    pub note: Option<String>,
    pub reporter_id: Uuid,
    /// Null when the org holds no email for the reporter (`users.email` is
    /// nullable — agent principals routinely have none).
    pub reporter_email: Option<String>,
    /// The reporter holds a seat on the memory's owning team. False for
    /// org-wide (teamless) memories, which have no owning team to belong to.
    pub reporter_on_owning_team: bool,
    /// How long ago this claim was filed, in seconds.
    pub age_secs: i64,
}

/// Who put the disputed memory in the corpus. A `wrong` claim against an LLM
/// extraction and one against a memory a human wrote by hand are not the same
/// claim, and the maintainer could not tell them apart before this was joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlaggedProvenance {
    pub actor_kind: String,
    pub actor_id: String,
    pub model_ref: Option<String>,
}

/// A memory with unresolved claims against it — one row per memory, not per
/// verdict, so the queue reads as "N memories need an answer".
#[derive(Debug, Clone)]
pub struct FlaggedMemory {
    pub memory_id: Uuid,
    pub title: Option<String>,
    pub content: String,
    pub kind: String,
    pub status: String,
    pub team_id: Option<Uuid>,
    /// The owning team's NAME. `team_id` alone is a UUID on a maintainer's
    /// screen, which is not information.
    pub team: Option<String>,
    pub confidence: Option<f32>,
    pub valid_to: Option<DateTime<Utc>>,
    pub provenance: Option<FlaggedProvenance>,
    pub wrong: i64,
    pub outdated: i64,
    /// DISTINCT reporters behind the open claims. `reporters < wrong+outdated`
    /// means somebody reported the same memory more than once.
    pub reporters: i64,
    /// The open claims, most recent first, capped at [`REPORT_CAP`].
    pub reports: Vec<ClaimReport>,
    /// Age of the OLDEST open claim — how long the dispute has stood.
    pub oldest_claim_secs: i64,
}

/// How many individual claims travel with a queue row. The counts and
/// `reporters` are exact over ALL open claims; this caps only the itemised
/// list, so a memory with 400 claims does not ship 400 notes to a bench that
/// shows five.
pub const REPORT_CAP: usize = 5;

/// The decay band a disputed memory falls in — how much of its validity window
/// is left. ONE definition, in SQL, used by the queue filter, the count and the
/// facets alike; the console renders these names and never recomputes them from
/// `valid_to`, because two implementations of "past its window" is two answers
/// to the same question.
///
/// `none` (no TTL) is its OWN band, not the right-hand edge. Collapsing it into
/// "never decays" is what let a memory with no expiry and a memory good for two
/// more years render at the identical spot.
pub const BANDS: [&str; 6] = ["past", "d30", "d90", "d180", "far", "none"];

const BAND_SQL: &str = "CASE
    WHEN m.valid_to IS NULL                          THEN 'none'
    WHEN m.valid_to <  now()                         THEN 'past'
    WHEN m.valid_to <  now() + interval '30 days'    THEN 'd30'
    WHEN m.valid_to <  now() + interval '90 days'    THEN 'd90'
    WHEN m.valid_to <  now() + interval '180 days'   THEN 'd180'
    ELSE 'far' END";

/// What the maintainer narrowed the backlog to.
///
/// These are SERVER-side on purpose. Filtering a page client-side answers
/// "which of these 50 match?" while the operator reads it as "which of the
/// 1,000 match?" — the same class of lie as sorting a server-paginated window.
#[derive(Debug, Clone, Default)]
pub struct FlaggedFilter {
    /// Memory kind (`fact`, `decision`, `howto`, …).
    pub kind: Option<String>,
    /// Owning team. Org-wide (teamless) memories match no team.
    pub team_id: Option<Uuid>,
    /// Only disputes whose OLDEST open claim has stood at least this long.
    pub min_age_hours: Option<i64>,
    /// Only memories carrying at least this many open claims.
    pub min_claims: Option<i64>,
    /// One of [`BANDS`].
    pub band: Option<String>,
}

impl FlaggedFilter {
    /// Rejects a band name the SQL would silently match nothing for — a typo'd
    /// filter must be an error, not an empty queue that reads as "all clear".
    pub fn band_is_valid(&self) -> bool {
        self.band.as_deref().is_none_or(|b| BANDS.contains(&b))
    }
}

/// The triage queue: memories carrying unresolved wrong/outdated claims,
/// most-disputed first, then oldest. RLS-scoped — a maintainer only ever
/// sees claims against memories they can read. `offset` pages beyond the first
/// window ([`flagged_count`] reports the total matching the same filter).
///
/// This asks a maintainer to permanently deprecate an org memory, so it carries
/// what that decision needs: WHO reported it (`users`, org-filtered explicitly —
/// that table carries no RLS policy of its own, same as `memories_list`), whether
/// they sit on the owning team (`team_members`, readable org-wide by policy), who
/// authored the memory (`provenance`), how sure the corpus was (`confidence`), and
/// the team's NAME rather than its UUID.
///
/// All of it rides the ONE grouped query. Every added table is either 1:1 with a
/// claim (`users`, keyed by id) or 1:1 with the memory (`teams`, `provenance`), and
/// `team_members` is keyed by its full PK `(team_id, user_id)` — so none of them can
/// multiply a row and inflate the counts, and none of them costs a round trip per
/// row. Widening this join is only safe under that rule; check it before adding one.
///
/// The filter is bound, never interpolated: each clause is
/// `($n IS NULL OR <col> = $n)`, so the SQL is one static string with no
/// injection surface and one query plan, and an absent filter costs nothing.
pub async fn flagged(
    conn: &mut PgConnection,
    filter: &FlaggedFilter,
    limit: i64,
    offset: i64,
) -> Result<Vec<FlaggedMemory>> {
    let sql = format!(
        "SELECT f.memory_id, m.title, m.content, m.kind, m.status::text AS status,
                m.team_id, m.valid_to, m.confidence,
                t.name AS team,
                pv.actor_kind, pv.actor_id, pv.model_ref,
                count(*) FILTER (WHERE f.verdict = 'wrong')    AS wrong,
                count(*) FILTER (WHERE f.verdict = 'outdated') AS outdated,
                count(DISTINCT f.user_id) AS reporters,
                COALESCE(
                    json_agg(json_build_object(
                        'verdict', f.verdict,
                        'note', f.note,
                        'reporter_id', f.user_id,
                        'reporter_email', u.email,
                        'reporter_on_owning_team', tm.user_id IS NOT NULL,
                        'age_secs', EXTRACT(EPOCH FROM now() - f.created_at)::bigint
                    ) ORDER BY f.created_at DESC),
                    '[]'::json
                ) AS reports,
                EXTRACT(EPOCH FROM now() - min(f.created_at))::bigint AS oldest_claim_secs
         FROM memory_feedback f
         JOIN memories m ON m.id = f.memory_id
         LEFT JOIN teams t ON t.id = m.team_id AND t.org_id = f.org_id
         LEFT JOIN users u ON u.id = f.user_id AND u.org_id = f.org_id
         LEFT JOIN team_members tm ON tm.user_id = f.user_id AND tm.team_id = m.team_id
         LEFT JOIN provenance pv ON pv.id = m.provenance_id
         WHERE f.resolved_at IS NULL
           AND f.verdict IN ('wrong', 'outdated')
           AND ($3::text IS NULL OR m.kind = $3)
           AND ($4::uuid IS NULL OR m.team_id = $4)
           AND ($5::text IS NULL OR ({BAND_SQL}) = $5)
         GROUP BY f.memory_id, m.title, m.content, m.kind, m.status, m.team_id, m.valid_to,
                  m.confidence, t.name, pv.actor_kind, pv.actor_id, pv.model_ref
         HAVING ($6::bigint IS NULL OR count(*) >= $6)
            AND ($7::bigint IS NULL
                 OR min(f.created_at) <= now() - make_interval(hours => $7::int))
         ORDER BY (count(*) FILTER (WHERE f.verdict = 'wrong')) DESC,
                  count(*) DESC,
                  min(f.created_at) ASC
         LIMIT $1 OFFSET $2"
    );
    let rows = sqlx::query(&sql)
        .bind(limit.clamp(1, 200))
        .bind(offset.max(0))
        .bind(filter.kind.as_deref())
        .bind(filter.team_id)
        .bind(filter.band.as_deref())
        .bind(filter.min_claims)
        .bind(filter.min_age_hours)
        .fetch_all(conn)
        .await?;
    rows.iter()
        .map(|r| {
            let mut reports: Vec<ClaimReport> =
                serde_json::from_value(r.get::<serde_json::Value, _>("reports"))?;
            reports.truncate(REPORT_CAP);
            Ok(FlaggedMemory {
                memory_id: r.get("memory_id"),
                title: r.get("title"),
                content: r.get("content"),
                kind: r.get("kind"),
                status: r.get("status"),
                team_id: r.get("team_id"),
                team: r.get("team"),
                confidence: r.get("confidence"),
                valid_to: r.get("valid_to"),
                // Whole-object null, never a half-populated record: a memory
                // with no provenance row has no actor to name.
                provenance: r.get::<Option<String>, _>("actor_kind").map(|actor_kind| {
                    FlaggedProvenance {
                        actor_kind,
                        actor_id: r.get("actor_id"),
                        model_ref: r.get("model_ref"),
                    }
                }),
                wrong: r.get("wrong"),
                outdated: r.get("outdated"),
                reporters: r.get("reporters"),
                reports,
                oldest_claim_secs: r.get("oldest_claim_secs"),
            })
        })
        .collect()
}

/// Count of memories with open claims MATCHING `filter` — the real backlog
/// behind a page window, and the console badge / analytics tile when the filter
/// is [`FlaggedFilter::default`]. Wraps the same per-memory grouping (and the
/// same HAVING) [`flagged`] uses, so "23 disputed" and the rows a maintainer
/// pages through can never disagree about what a claim is.
pub async fn flagged_count(conn: &mut PgConnection, filter: &FlaggedFilter) -> Result<i64> {
    let sql = format!(
        "SELECT count(*) AS n FROM (
            SELECT f.memory_id
            FROM memory_feedback f
            JOIN memories m ON m.id = f.memory_id
            WHERE f.resolved_at IS NULL
              AND f.verdict IN ('wrong', 'outdated')
              AND ($1::text IS NULL OR m.kind = $1)
              AND ($2::uuid IS NULL OR m.team_id = $2)
              AND ($3::text IS NULL OR ({BAND_SQL}) = $3)
            GROUP BY f.memory_id
            HAVING ($4::bigint IS NULL OR count(*) >= $4)
               AND ($5::bigint IS NULL
                    OR min(f.created_at) <= now() - make_interval(hours => $5::int))
        ) q"
    );
    let row = sqlx::query(&sql)
        .bind(filter.kind.as_deref())
        .bind(filter.team_id)
        .bind(filter.band.as_deref())
        .bind(filter.min_claims)
        .bind(filter.min_age_hours)
        .fetch_one(conn)
        .await?;
    Ok(row.get("n"))
}

/// One facet value and how many disputed memories carry it — enough to build a
/// filter control that shows the operator what narrowing by each option would
/// leave, so they never pick a filter that empties the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facet {
    pub value: String,
    /// Team facets carry the display name; kind/band facets repeat `value`.
    pub label: String,
    pub count: i64,
}

/// The facet breakdown of the FULL unfiltered backlog: disputed-memory counts
/// by kind, by owning team, and by decay band. Computed per memory (a memory
/// with four claims is one disputed memory, counted once), in three small
/// grouped passes over the same claim set the queue reads. RLS-scoped.
pub async fn flagged_facets(
    conn: &mut PgConnection,
) -> Result<(Vec<Facet>, Vec<Facet>, Vec<Facet>)> {
    // Distinct disputed memories once, with the columns every facet groups on —
    // so a memory is counted a single time per facet no matter its claim count.
    let rows = sqlx::query(&format!(
        "SELECT m.kind, m.team_id, t.name AS team, ({BAND_SQL}) AS band
         FROM memory_feedback f
         JOIN memories m ON m.id = f.memory_id
         LEFT JOIN teams t ON t.id = m.team_id AND t.org_id = f.org_id
         WHERE f.resolved_at IS NULL AND f.verdict IN ('wrong', 'outdated')
         GROUP BY f.memory_id, m.kind, m.team_id, t.name, ({BAND_SQL})"
    ))
    .fetch_all(conn)
    .await?;

    let mut kinds: HashMap<String, i64> = HashMap::new();
    let mut teams: HashMap<(Option<Uuid>, Option<String>), i64> = HashMap::new();
    let mut bands: HashMap<String, i64> = HashMap::new();
    for r in &rows {
        *kinds.entry(r.get("kind")).or_default() += 1;
        *teams.entry((r.get("team_id"), r.get("team"))).or_default() += 1;
        *bands.entry(r.get("band")).or_default() += 1;
    }

    let mut kind_facets: Vec<Facet> = kinds
        .into_iter()
        .map(|(value, count)| Facet {
            label: value.clone(),
            value,
            count,
        })
        .collect();
    kind_facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));

    let mut team_facets: Vec<Facet> = teams
        .into_iter()
        .map(|((id, name), count)| Facet {
            value: id.map(|u| u.to_string()).unwrap_or_default(),
            label: name.unwrap_or_else(|| "org-wide".to_string()),
            count,
        })
        .collect();
    team_facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));

    // Bands keep the decay order, not a count order — the axis has a direction.
    let band_facets: Vec<Facet> = BANDS
        .iter()
        .filter_map(|b| {
            bands.get(*b).map(|&count| Facet {
                value: (*b).to_string(),
                label: (*b).to_string(),
                count,
            })
        })
        .collect();

    Ok((kind_facets, team_facets, band_facets))
}

/// How many claims against this memory are still open. Callers answering a
/// dispute check this BEFORE mutating the corpus: zero open claims means there
/// is nothing to answer (a concurrent maintainer already answered it), and a
/// destructive resolution applied on top of that would be a decision nobody
/// asked for, reported as a success. Serialize on the memory row (`FOR UPDATE`)
/// before calling, or the count can go stale under you.
pub async fn open_claim_count(conn: &mut PgConnection, memory_id: Uuid) -> Result<i64> {
    let row = sqlx::query(
        "SELECT count(*) AS n FROM memory_feedback
         WHERE memory_id = $1 AND resolved_at IS NULL AND verdict IN ('wrong', 'outdated')",
    )
    .bind(memory_id)
    .fetch_one(conn)
    .await?;
    Ok(row.get("n"))
}

/// Close every open claim against a memory with the maintainer's answer, and
/// the rationale behind it (`note`, 0026 — kept apart from the reporter's own
/// `note`, which is the claim being answered). Returns how many claims were
/// closed (0 = nothing was open).
///
/// `resolved_at` is a single `now()` for the whole call, which is what lets the
/// audit feed group the N closed claims back into the ONE decision that closed
/// them.
pub async fn resolve_claims(
    conn: &mut PgConnection,
    memory_id: Uuid,
    resolver: Uuid,
    resolution: &str,
    note: Option<&str>,
) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE memory_feedback
         SET resolution = $3, resolved_by = $2, resolved_at = now(), resolution_note = $4
         WHERE memory_id = $1
           AND resolved_at IS NULL
           AND verdict IN ('wrong', 'outdated')",
    )
    .bind(memory_id)
    .bind(resolver)
    .bind(resolution)
    .bind(note)
    .execute(conn)
    .await?;
    Ok(res.rows_affected())
}
