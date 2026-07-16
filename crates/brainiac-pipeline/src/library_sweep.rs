//! LB3: the library mining sweep (docs/LIBRARY-PLAN.md).
//!
//! Three deterministic miners over signal the org already produced, each
//! emitting `proposed` standard candidates into the LB2 triage queue:
//!
//!   (a) unclaimed practice divergences — the drift detector found the same
//!       problem solved differently and nobody has bridged it yet;
//!   (b) reinforced practices — canonical `pattern`/`pitfall` memories the
//!       org keeps confirming through feedback (the practice is already
//!       being treated as a rule; the Library makes that official-able);
//!   (c) convention-settling contradictions — a resolved supersession whose
//!       winner is a canonical `decision`/`pattern`: the org already chose.
//!
//! A GENERATOR, NEVER AN AUTHORITY (L2): everything lands as `proposed` with
//! its signal attached as provenance; only a named human adopts. Deliberately
//! heuristic, not LLM: candidates carry the org's own words verbatim, cost
//! nothing per run, and the eval story is a deterministic gate rather than a
//! variance measurement.
//!
//! THE DEDUP RULE (the LB3 hard gate): a signal is skipped when any standard
//! already carries it as provenance — including a REJECTED one inside the
//! dedup window. Rejection is knowledge: a maintainer who said no must not be
//! asked again next week. Past the window a rejected signal may return —
//! practice that keeps generating the same evidence eventually deserves a
//! second look, dated and attributed like the first.
//!
//! Cross-org operator sweep: runs on the RLS-bypassing admin pool (same shape
//! as divergence::scan_all), every query org-scoped explicitly.

use anyhow::Result;
use brainiac_store::library;
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

/// How many helpful verdicts make a practice "reinforced". Two: one reader
/// confirming could be the author's teammate being nice; two independent
/// confirmations is the org leaning on it.
const REINFORCED_MIN_HELPFUL: i64 = 2;

/// Default dedup window (days): how long a rejection keeps its signal out of
/// triage. Overridable per call; the sweep wiring reads BRAINIAC_LIBRARY_DEDUP_DAYS.
pub const DEFAULT_DEDUP_WINDOW_DAYS: i64 = 90;

#[derive(Debug, Default, Clone, Copy)]
pub struct MiningStats {
    pub orgs: usize,
    pub from_divergence: usize,
    pub from_feedback: usize,
    pub from_contradiction: usize,
    /// Signals skipped because a standard (any lifecycle, or a rejection
    /// inside the window) already carries them.
    pub deduped: usize,
}

impl MiningStats {
    pub fn candidates(&self) -> usize {
        self.from_divergence + self.from_feedback + self.from_contradiction
    }
}

/// Is this signal already spoken for? True when any standard carries it as
/// provenance — unconditionally for live standards, and inside the window for
/// rejected ones.
async fn blocked(conn: &mut PgConnection, ref_id: Uuid, window_days: i64) -> Result<bool> {
    let row = sqlx::query(
        "SELECT 1 AS hit FROM standard_provenance sp
         JOIN standards s ON s.id = sp.standard_id
         WHERE sp.ref_id = $1
           AND (s.lifecycle <> 'rejected'
                OR s.updated_at > now() - make_interval(days => $2::int))
         LIMIT 1",
    )
    .bind(ref_id)
    .bind(window_days)
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

/// A slug for a mined candidate: from the memory's title when a human wrote
/// one, else the statement's first words. Collisions with unrelated rules get
/// a short suffix from the signal id — same policy as the divergence bridge.
async fn unique_slug(
    conn: &mut PgConnection,
    org_id: Uuid,
    base: &str,
    ref_id: Uuid,
) -> Result<String> {
    let base = library::slugify(base);
    let short: String = base.split('-').take(6).collect::<Vec<_>>().join("-");
    let taken = sqlx::query("SELECT 1 AS one FROM standards WHERE org_id = $1 AND slug = $2")
        .bind(org_id)
        .bind(&short)
        .fetch_optional(&mut *conn)
        .await?
        .is_some();
    Ok(if taken {
        format!("{short}-{}", &ref_id.simple().to_string()[..8])
    } else {
        short
    })
}

/// Miner (a): every divergence no standard has claimed yet.
async fn mine_divergences(
    conn: &mut PgConnection,
    org_id: Uuid,
    window_days: i64,
    stats: &mut MiningStats,
) -> Result<()> {
    let ids: Vec<Uuid> = sqlx::query("SELECT id FROM practice_divergences WHERE org_id = $1")
        .bind(org_id)
        .fetch_all(&mut *conn)
        .await?
        .iter()
        .map(|r| r.get("id"))
        .collect();
    for id in ids {
        if blocked(conn, id, window_days).await? {
            stats.deduped += 1;
            continue;
        }
        if library::propose_from_divergence(conn, id, None)
            .await?
            .is_some()
        {
            stats.from_divergence += 1;
        }
    }
    Ok(())
}

/// Miner (b): canonical pattern/pitfall memories the org keeps confirming.
async fn mine_reinforced(
    conn: &mut PgConnection,
    org_id: Uuid,
    window_days: i64,
    stats: &mut MiningStats,
) -> Result<()> {
    let rows = sqlx::query(
        "SELECT m.id, m.kind, m.title, m.content, m.detail_md, count(f.id) AS helpful
         FROM memories m
         JOIN memory_feedback f ON f.memory_id = m.id AND f.verdict = 'helpful'
         WHERE m.org_id = $1 AND m.status = 'canonical' AND m.deleted_at IS NULL
           AND m.kind IN ('pattern', 'pitfall')
         GROUP BY m.id HAVING count(f.id) >= $2",
    )
    .bind(org_id)
    .bind(REINFORCED_MIN_HELPFUL)
    .fetch_all(&mut *conn)
    .await?;

    for r in rows {
        let memory_id: Uuid = r.get("id");
        if blocked(conn, memory_id, window_days).await? {
            stats.deduped += 1;
            continue;
        }
        let kind: String = r.get("kind");
        let title: Option<String> = r.get("title");
        let content: String = r.get("content");
        let helpful: i64 = r.get("helpful");
        let slug = unique_slug(
            conn,
            org_id,
            title.as_deref().unwrap_or(&content),
            memory_id,
        )
        .await?;
        library::insert_standard(
            conn,
            &library::NewStandard {
                id: Uuid::new_v4(),
                org_id,
                origin: brainiac_core::StandardOrigin::Sweep,
                stack: "general".into(),
                category: kind, // pattern | pitfall — the memory's own taxonomy
                slug,
                statement: content,
                rationale: Some(format!(
                    "Reinforced practice: confirmed helpful {helpful} times by readers — the org already treats this as a rule."
                )),
                detail_md: r.get("detail_md"),
                enforcement: Default::default(),
                provenance: vec![(brainiac_core::StandardProvenanceKind::Memory, memory_id)],
                author: None,
            },
        )
        .await?;
        stats.from_feedback += 1;
    }
    Ok(())
}

/// Miner (c): resolved supersessions whose winner states a convention.
async fn mine_contradictions(
    conn: &mut PgConnection,
    org_id: Uuid,
    window_days: i64,
    stats: &mut MiningStats,
) -> Result<()> {
    // The winner of a supersession is the side still canonical and not
    // superseded; only decision/pattern winners state a convention worth
    // proposing (a fact winning a fact-fight is just the truth, not a rule).
    let rows = sqlx::query(
        "SELECT w.id, w.title, w.content, w.detail_md, w.kind, c.resolution_note
         FROM contradictions c
         JOIN memories w ON w.id IN (c.memory_a, c.memory_b)
         WHERE c.org_id = $1 AND c.status = 'resolved_supersede'
           AND w.status = 'canonical' AND w.superseded_by IS NULL AND w.deleted_at IS NULL
           AND w.kind IN ('decision', 'pattern')",
    )
    .bind(org_id)
    .fetch_all(&mut *conn)
    .await?;

    for r in rows {
        let memory_id: Uuid = r.get("id");
        if blocked(conn, memory_id, window_days).await? {
            stats.deduped += 1;
            continue;
        }
        let title: Option<String> = r.get("title");
        let content: String = r.get("content");
        let note: Option<String> = r.get("resolution_note");
        let slug = unique_slug(
            conn,
            org_id,
            title.as_deref().unwrap_or(&content),
            memory_id,
        )
        .await?;
        library::insert_standard(
            conn,
            &library::NewStandard {
                id: Uuid::new_v4(),
                org_id,
                origin: brainiac_core::StandardOrigin::Sweep,
                stack: "general".into(),
                category: "convention".into(),
                slug,
                statement: content,
                rationale: Some(match note {
                    Some(n) => format!("Settled a contradiction: {n}"),
                    None => "Settled a contradiction — the org already chose this side.".into(),
                }),
                detail_md: r.get("detail_md"),
                enforcement: Default::default(),
                provenance: vec![(brainiac_core::StandardProvenanceKind::Memory, memory_id)],
                author: None,
            },
        )
        .await?;
        stats.from_contradiction += 1;
    }
    Ok(())
}

/// Mine one org. Order matters only for within-run dedup: each insert writes
/// provenance, and every later miner's `blocked` check sees it — so a memory
/// that is both reinforced and a contradiction winner yields ONE candidate.
pub async fn mine_org(
    conn: &mut PgConnection,
    org_id: Uuid,
    window_days: i64,
) -> Result<MiningStats> {
    let mut stats = MiningStats::default();
    mine_divergences(conn, org_id, window_days, &mut stats).await?;
    mine_reinforced(conn, org_id, window_days, &mut stats).await?;
    mine_contradictions(conn, org_id, window_days, &mut stats).await?;
    Ok(stats)
}

/// The sweep entrypoint: every org, one transaction per org so a failing org
/// rolls back cleanly without holding the others hostage.
pub async fn mine_all(pool: &PgPool, window_days: i64) -> Result<MiningStats> {
    let orgs: Vec<Uuid> = sqlx::query("SELECT id FROM orgs")
        .fetch_all(pool)
        .await?
        .iter()
        .map(|r| r.get("id"))
        .collect();
    let mut total = MiningStats::default();
    for org in orgs {
        let mut tx = pool.begin().await?;
        let s = mine_org(&mut tx, org, window_days).await?;
        tx.commit().await?;
        total.orgs += 1;
        total.from_divergence += s.from_divergence;
        total.from_feedback += s.from_feedback;
        total.from_contradiction += s.from_contradiction;
        total.deduped += s.deduped;
    }
    Ok(total)
}
