//! Publish targets, the publication ledger, and the health circuit breaker
//! (KB3; migration 0020).

use anyhow::Result;
use brainiac_core::health;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishTarget {
    pub id: Uuid,
    pub org_id: Uuid,
    /// `git` | `okf` | `confluence`
    pub kind: String,
    pub config: serde_json::Value,
    /// The NAME of the env var holding the credential — never the credential.
    pub secret_ref: Option<String>,
    pub enabled: bool,
}

fn row_to_target(r: &sqlx::postgres::PgRow) -> PublishTarget {
    PublishTarget {
        id: r.get("id"),
        org_id: r.get("org_id"),
        kind: r.get("kind"),
        config: r.get("config"),
        secret_ref: r.get("secret_ref"),
        enabled: r.get("enabled"),
    }
}

pub async fn insert_target(conn: &mut PgConnection, t: &PublishTarget) -> Result<()> {
    sqlx::query(
        "INSERT INTO publish_targets (id, org_id, kind, config, secret_ref, enabled)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(t.id)
    .bind(t.org_id)
    .bind(&t.kind)
    .bind(&t.config)
    .bind(&t.secret_ref)
    .bind(t.enabled)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn enabled_targets(conn: &mut PgConnection) -> Result<Vec<PublishTarget>> {
    let rows = sqlx::query(
        "SELECT id, org_id, kind, config, secret_ref, enabled
         FROM publish_targets WHERE enabled ORDER BY created_at",
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.iter().map(row_to_target).collect())
}

/// Is the KB layer switched on for this org? Off by default (KB-PLAN D6): a
/// feature that turns itself on inside someone's Confluence is not a feature.
pub async fn kb_enabled(conn: &mut PgConnection, org_id: Uuid) -> Result<bool> {
    let row = sqlx::query("SELECT kb_enabled FROM orgs WHERE id = $1")
        .bind(org_id)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(|r| r.get::<bool, _>("kb_enabled")).unwrap_or(false))
}

pub async fn set_kb_enabled(conn: &mut PgConnection, org_id: Uuid, on: bool) -> Result<()> {
    sqlx::query("UPDATE orgs SET kb_enabled = $2 WHERE id = $1")
        .bind(org_id)
        .bind(on)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Publication {
    pub document_id: Uuid,
    pub target_id: Uuid,
    pub revision_id: Uuid,
    pub external_ref: Option<String>,
    pub published_at: DateTime<Utc>,
}

/// What is live in the external system right now, if anything.
pub async fn publication(
    conn: &mut PgConnection,
    document_id: Uuid,
    target_id: Uuid,
) -> Result<Option<Publication>> {
    let row = sqlx::query(
        "SELECT document_id, target_id, revision_id, external_ref, published_at
         FROM document_publications WHERE document_id = $1 AND target_id = $2",
    )
    .bind(document_id)
    .bind(target_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| Publication {
        document_id: r.get("document_id"),
        target_id: r.get("target_id"),
        revision_id: r.get("revision_id"),
        external_ref: r.get("external_ref"),
        published_at: r.get("published_at"),
    }))
}

pub async fn record_publication(
    conn: &mut PgConnection,
    org_id: Uuid,
    document_id: Uuid,
    target_id: Uuid,
    revision_id: Uuid,
    external_ref: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO document_publications
            (document_id, target_id, org_id, revision_id, external_ref, published_at)
         VALUES ($1,$2,$3,$4,$5, now())
         ON CONFLICT (document_id, target_id) DO UPDATE
           SET revision_id = EXCLUDED.revision_id,
               external_ref = COALESCE(EXCLUDED.external_ref, document_publications.external_ref),
               published_at = now()",
    )
    .bind(document_id)
    .bind(target_id)
    .bind(org_id)
    .bind(revision_id)
    .bind(external_ref)
    .execute(conn)
    .await?;
    Ok(())
}

// ── the circuit breaker (KB-PLAN D7) ────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PublishGate {
    pub currency: i64,
    pub governance: i64,
    /// `None` = publish. `Some(reason)` = hold, and say why.
    pub blocked: Option<String>,
}

/// Should this org be allowed to keep pushing its knowledge outward right now?
///
/// The pillar formulas come from `brainiac_core::health` — the SAME functions the
/// leadership report renders. This is the point of KB3: the health score stops
/// being a dashboard nobody acts on and becomes the thing that decides whether a
/// degrading corpus keeps broadcasting itself into the company wiki at machine
/// speed. When it trips, pages hold their last published revision rather than
/// propagating stale belief. Silence beats confident staleness.
pub async fn publish_gate(conn: &mut PgConnection, org_id: Uuid) -> Result<PublishGate> {
    let corpus = sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE status = 'deprecated'
                                   OR (valid_to IS NOT NULL AND valid_to < now())) AS stale
         FROM memories WHERE status <> 'rejected' AND org_id = $1",
    )
    .bind(org_id)
    .fetch_one(&mut *conn)
    .await?;
    let total: i64 = corpus.get("total");
    let stale: i64 = corpus.get("stale");

    let gov = sqlx::query(
        "SELECT count(*) AS pending,
                COALESCE(EXTRACT(EPOCH FROM now() - min(created_at)), 0)::bigint AS oldest
         FROM promotions
         WHERE policy_decision = 'needs_review' AND reviewed_at IS NULL AND org_id = $1",
    )
    .bind(org_id)
    .fetch_one(&mut *conn)
    .await?;
    let backlog: i64 = gov.get("pending");
    let oldest: i64 = gov.get("oldest");

    let currency = health::currency_pillar(total, stale);
    let governance = health::governance_pillar(backlog, oldest);
    Ok(PublishGate {
        currency,
        governance,
        blocked: health::publish_block_reason(currency, governance),
    })
}
