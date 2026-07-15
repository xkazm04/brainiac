//! The alert sweep — the moment "nothing went red" stops being possible.
//!
//! The UAT's central negative finding was not a crash; it was silence. The
//! review queue stalled, the corpus kept serving the backlog as truth, and no
//! surface anywhere CHANGED STATE. Knowledge Health made the failure visible to
//! whoever opens the dashboard — this sweep makes it visible to people who
//! don't, by pushing breaches to a webhook the operator configures.
//!
//! Design choices, each a refusal of a bigger design:
//!
//! - **One operator webhook, not per-org channels.** `BRAINIAC_ALERT_WEBHOOK_URL`
//!   is set by whoever runs the binary. The payload is grouped per org, so a
//!   multi-tenant operator still sees who is on fire. Per-org routing is a later
//!   increment that should be pulled by a customer, not pushed by us.
//! - **Generic JSON with a `text` field.** Slack's incoming webhooks accept
//!   exactly `{"text": …}`, and anything that isn't Slack still gets structured
//!   `breaches` alongside. One payload, no per-vendor adapters.
//! - **Cadence is the debounce.** A breach that persists re-alerts once per
//!   sweep cadence (default 6h when enabled) — deliberate: a stalled review
//!   queue that stays stalled SHOULD keep paging, and an operator who wants
//!   less noise turns the cadence down in the sweeps UI, visibly, rather than
//!   us silently deduplicating a standing failure into one forgotten message.
//! - **No webhook configured + alertable breach = a loud log, not a silent
//!   skip.** The sweep's own last_detail records what it WOULD have sent.
//!
//! The breach conditions read the SAME org-true health computation the
//! leadership report renders (`console::compute_health_core`) — an alert that
//! disagreed with the dashboard would be worse than no alert.

use anyhow::{Context, Result};
use brainiac_core::health;
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A page dirty longer than this is a broken promise, not a queue.
/// (Mirrors the Knowledge Health attention item's critical threshold.)
const PROPAGATION_CRITICAL_SECS: i64 = 24 * 3600;

#[derive(Debug, Serialize)]
pub struct Breach {
    pub org: String,
    /// governance | currency | contradiction | propagation
    pub kind: String,
    pub headline: String,
}

/// Evaluate every org and push breaches to the operator webhook, if configured.
/// Returns the sweep detail line.
pub async fn alert_sweep(admin: &PgPool) -> Result<String> {
    let orgs = sqlx::query("SELECT id, name FROM orgs")
        .fetch_all(admin)
        .await?;

    let mut breaches: Vec<Breach> = Vec::new();
    for row in &orgs {
        let org_id: Uuid = row.get("id");
        let org_name: String = row.get("name");
        let mut tx = admin.begin().await?;
        let core = crate::console::compute_health_core(&mut tx, Some(org_id))
            .await
            .map_err(|e| anyhow::anyhow!("health core for {org_name}: {}", e.message))?;

        // KB propagation: the oldest page still serving a superseded belief.
        let kb = sqlx::query(
            "SELECT count(*) FILTER (WHERE dirty_at IS NOT NULL) AS dirty,
                    COALESCE(EXTRACT(EPOCH FROM now() - min(dirty_at)), 0)::bigint AS oldest_dirty
             FROM documents WHERE org_id = $1",
        )
        .bind(org_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        let pages_dirty: i64 = kb.get("dirty");
        let oldest_dirty: i64 = kb.get("oldest_dirty");

        if core.oldest > health::REVIEW_SLO_SECS {
            breaches.push(Breach {
                org: org_name.clone(),
                kind: "governance".into(),
                headline: format!(
                    "review queue stalled: oldest promotion has waited {} (SLO 48h), backlog {}",
                    human_age(core.oldest),
                    core.backlog
                ),
            });
        }
        if core.currency < health::PUBLISH_MIN_CURRENCY {
            breaches.push(Breach {
                org: org_name.clone(),
                kind: "currency".into(),
                headline: format!(
                    "corpus currency {} is below the publish floor {} — {} of {} beliefs are stale",
                    core.currency,
                    health::PUBLISH_MIN_CURRENCY,
                    core.stale,
                    core.total
                ),
            });
        }
        if core.cross_contra > 0 {
            breaches.push(Breach {
                org: org_name.clone(),
                kind: "contradiction".into(),
                headline: format!(
                    "{} open cross-team contradiction(s) — two teams are acting on incompatible truths",
                    core.cross_contra
                ),
            });
        }
        if pages_dirty > 0 && oldest_dirty > PROPAGATION_CRITICAL_SECS {
            breaches.push(Breach {
                org: org_name,
                kind: "propagation".into(),
                headline: format!(
                    "{pages_dirty} knowledge-base page(s) not recomposed for {} — the wiki is rotting",
                    human_age(oldest_dirty)
                ),
            });
        }
    }

    if breaches.is_empty() {
        return Ok(format!("{} orgs checked, no breaches", orgs.len()));
    }

    let text = render_text(&breaches);
    match std::env::var("BRAINIAC_ALERT_WEBHOOK_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
    {
        Some(url) => {
            post_webhook(&url, &text, &breaches).await?;
            Ok(format!(
                "{} orgs checked, {} breach(es) sent to webhook",
                orgs.len(),
                breaches.len()
            ))
        }
        None => {
            // No channel configured: the sweep must not pretend it alerted.
            // Loud log + honest detail, so the sweeps UI shows the gap.
            tracing::error!(breaches = breaches.len(), %text, "ALERTS with no webhook configured (set BRAINIAC_ALERT_WEBHOOK_URL)");
            Ok(format!(
                "{} breach(es) found but NO WEBHOOK CONFIGURED — set BRAINIAC_ALERT_WEBHOOK_URL. First: {}",
                breaches.len(),
                breaches[0].headline
            ))
        }
    }
}

fn render_text(breaches: &[Breach]) -> String {
    let mut out = String::from("⚠️ Brainiac knowledge alerts\n");
    for b in breaches {
        out.push_str(&format!("• [{}] {}: {}\n", b.kind, b.org, b.headline));
    }
    out
}

async fn post_webhook(url: &str, text: &str, breaches: &[Breach]) -> Result<()> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let res = client
        .post(url)
        .json(&serde_json::json!({ "text": text, "breaches": breaches }))
        .send()
        .await
        .context("posting alert webhook")?;
    anyhow::ensure!(
        res.status().is_success(),
        "alert webhook rejected the payload: {}",
        res.status()
    );
    Ok(())
}

fn human_age(secs: i64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    if d > 0 {
        format!("{d}d {h}h")
    } else {
        format!("{h}h {}m", (secs % 3_600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_payload_names_every_breach() {
        let text = render_text(&[
            Breach {
                org: "meridian".into(),
                kind: "governance".into(),
                headline:
                    "review queue stalled: oldest promotion has waited 3d 2h (SLO 48h), backlog 9"
                        .into(),
            },
            Breach {
                org: "meridian".into(),
                kind: "propagation".into(),
                headline: "2 knowledge-base page(s) not recomposed for 1d 5h — the wiki is rotting"
                    .into(),
            },
        ]);
        assert!(text.contains("[governance] meridian"));
        assert!(text.contains("[propagation] meridian"));
        assert!(text.contains("3d 2h"));
    }

    #[test]
    fn ages_read_like_a_human_wrote_them() {
        assert_eq!(human_age(3 * 86_400 + 2 * 3_600), "3d 2h");
        assert_eq!(human_age(90 * 60), "1h 30m");
    }
}
