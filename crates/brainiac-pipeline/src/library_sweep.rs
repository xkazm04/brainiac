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

/// The dedup window every caller should use — the weekly sweep AND the
/// event-triggered mine (see [`mine_contradiction_winner`]) read the SAME
/// env var so a rejection is honored identically regardless of which path
/// found the signal first.
pub fn dedup_window_days() -> i64 {
    std::env::var("BRAINIAC_LIBRARY_DEDUP_DAYS")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|d| *d > 0)
        .unwrap_or(DEFAULT_DEDUP_WINDOW_DAYS)
}

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

/// A contradiction's winning memory, as much as the miner needs of it — bundled
/// so [`propose_contradiction_winner`] takes one argument instead of four.
struct ContradictionWinner<'a> {
    memory_id: Uuid,
    title: Option<&'a str>,
    content: &'a str,
    detail_md: Option<String>,
}

/// Turn one contradiction winner into a `proposed` standard candidate, unless
/// it is deduped. Shared by the org-wide sweep ([`mine_contradictions`]) and
/// the event-triggered targeted mine ([`mine_contradiction_winner`]) so the
/// two paths can never diverge in what counts as a candidate.
async fn propose_contradiction_winner(
    conn: &mut PgConnection,
    org_id: Uuid,
    winner: ContradictionWinner<'_>,
    resolution_note: Option<&str>,
    window_days: i64,
) -> Result<Option<Uuid>> {
    if blocked(conn, winner.memory_id, window_days).await? {
        return Ok(None);
    }
    let slug = unique_slug(
        conn,
        org_id,
        winner.title.unwrap_or(winner.content),
        winner.memory_id,
    )
    .await?;
    let standard = library::NewStandard {
        id: Uuid::new_v4(),
        org_id,
        origin: brainiac_core::StandardOrigin::Sweep,
        stack: "general".into(),
        category: "convention".into(),
        slug,
        statement: winner.content.to_string(),
        rationale: Some(match resolution_note {
            Some(n) => format!("Settled a contradiction: {n}"),
            None => "Settled a contradiction — the org already chose this side.".into(),
        }),
        detail_md: winner.detail_md,
        enforcement: Default::default(),
        provenance: vec![(
            brainiac_core::StandardProvenanceKind::Memory,
            winner.memory_id,
        )],
        author: None,
    };
    library::insert_standard(conn, &standard).await?;
    Ok(Some(standard.id))
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
        let title: Option<String> = r.get("title");
        let content: String = r.get("content");
        let note: Option<String> = r.get("resolution_note");
        let detail_md: Option<String> = r.get("detail_md");
        match propose_contradiction_winner(
            conn,
            org_id,
            ContradictionWinner {
                memory_id,
                title: title.as_deref(),
                content: &content,
                detail_md,
            },
            note.as_deref(),
            window_days,
        )
        .await?
        {
            Some(_) => stats.from_contradiction += 1,
            None => stats.deduped += 1,
        }
    }
    Ok(())
}

/// The event-triggered twin of miner (c): mine ONLY the cluster a just-settled
/// contradiction named, the moment it settles, instead of waiting for the
/// (disabled-by-default, weekly) sweep to rediscover it. Reuses the exact
/// same qualification query, dedup window, and insert path as
/// [`mine_contradictions`] — restricted with `AND w.id = $2` to the one
/// winner the caller already knows, so this is cheap (one indexed lookup)
/// and never touches another cluster.
///
/// Call this right after flipping a contradiction to `resolved_supersede`
/// (see `console::resolve_contradiction`), inside the same transaction: the
/// candidate becomes visible in the SAME commit as the settlement, and if the
/// winner doesn't qualify (wrong kind, not canonical, already superseded) or
/// its signal is already spoken for (blocked by the dedup window, e.g. a
/// human already ratified it, or an earlier settlement of the same memory)
/// this is a cheap no-op returning `Ok(None)` — never a parallel sweep.
pub async fn mine_contradiction_winner(
    conn: &mut PgConnection,
    org_id: Uuid,
    winner_id: Uuid,
    resolution_note: Option<&str>,
    window_days: i64,
) -> Result<Option<Uuid>> {
    let Some(r) = sqlx::query(
        "SELECT title, content, detail_md
         FROM memories
         WHERE id = $1 AND org_id = $2
           AND status = 'canonical' AND superseded_by IS NULL AND deleted_at IS NULL
           AND kind IN ('decision', 'pattern')",
    )
    .bind(winner_id)
    .bind(org_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let title: Option<String> = r.get("title");
    let content: String = r.get("content");
    let detail_md: Option<String> = r.get("detail_md");
    propose_contradiction_winner(
        conn,
        org_id,
        ContradictionWinner {
            memory_id: winner_id,
            title: title.as_deref(),
            content: &content,
            detail_md,
        },
        resolution_note,
        window_days,
    )
    .await
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
