//! Sweep scheduling — the operator surface for the periodic org-intelligence
//! sweeps (practice-divergence + knowledge-health snapshot).
//!
//! Two halves meet here:
//!
//! - **The scheduler** ([`run_due`]) runs inside the worker loop. Once every
//!   [`SCHED_INTERVAL`] it atomically claims every schedule that has come due
//!   (`enabled` + `next_run_at <= now()`), advances each row's clock, and
//!   spawns the sweep so a multi-minute LLM scan never blocks ingest draining.
//!   Each sweep records its own outcome back onto its row when it finishes.
//!
//! - **The endpoints** (`/v1/ops/sweeps`, admin-scoped) let a UI read every
//!   sweep's cadence + last-run status, flip one on/off, retune its cadence,
//!   and trigger a one-shot "run now". They run as `brainiac_app` against the
//!   non-RLS `sweep_schedules` table (global operator config, not org data).
//!
//! The sweeps themselves are cross-org operator actions on the RLS-bypassing
//! admin pool — divergence loops every org (`divergence::scan_all`), the health
//! snapshot loops every org (`console::snapshot_all_orgs`).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use brainiac_gateway::ChatProvider;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use utoipa::ToSchema;

use crate::http::{auth_of, internal, AppState, HttpError};

/// How often the worker checks for due sweeps. Sweeps run on cadences of hours
/// to weeks, so a 20s scheduler poll is negligible overhead and makes a "run
/// now" from the UI take effect within seconds.
pub const SCHED_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);

/// Floor on a configurable cadence — the sweeps are expensive (an LLM call per
/// cross-team cluster); a 5-minute floor stops a fat-fingered UI from turning
/// one into a billing incident.
const MIN_CADENCE_SECS: i64 = 300;

/// A `running` row older than this is treated as crashed (the worker died
/// mid-sweep) and becomes eligible again, so a schedule can't wedge forever.
const RUNNING_STALE: &str = "2 hours";

/// Bound a sweep's recorded detail/error so one row can't grow without limit.
fn clip_detail(s: &str) -> String {
    const MAX: usize = 500;
    if s.chars().count() > MAX {
        s.chars().take(MAX).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

// ── wire types ──────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct SweepSchedule {
    /// 'divergence' | 'health_snapshot'.
    pub kind: String,
    /// Whether the scheduler runs this sweep on its cadence.
    pub enabled: bool,
    /// Seconds between runs when enabled.
    pub cadence_secs: i64,
    /// When the scheduler will next run it (null once disabled and idle).
    pub next_run_at: Option<DateTime<Utc>>,
    /// When it last started.
    pub last_run_at: Option<DateTime<Utc>>,
    /// 'ok' | 'error' | 'running' — null until it has run once.
    pub last_status: Option<String>,
    /// Human summary of the last run ("7 clusters, 1 divergences") or its error.
    pub last_detail: Option<String>,
    /// Wall-clock of the last run.
    pub last_duration_ms: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct SweepsResponse {
    /// Every configured sweep, ordered by kind.
    pub sweeps: Vec<SweepSchedule>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateSweepBody {
    /// Turn the sweep's schedule on or off.
    pub enabled: Option<bool>,
    /// Retune the cadence (seconds; floored at 300).
    pub cadence_secs: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct RunSweepResponse {
    pub kind: String,
    /// The sweep was marked due; the worker picks it up within ~20s.
    pub queued: bool,
    pub next_run_at: Option<DateTime<Utc>>,
}

fn row_to_schedule(r: &sqlx::postgres::PgRow) -> SweepSchedule {
    SweepSchedule {
        kind: r.get("kind"),
        enabled: r.get("enabled"),
        cadence_secs: r.get("cadence_secs"),
        next_run_at: r.get("next_run_at"),
        last_run_at: r.get("last_run_at"),
        last_status: r.get("last_status"),
        last_detail: r.get("last_detail"),
        last_duration_ms: r.get("last_duration_ms"),
    }
}

const SCHEDULE_COLS: &str = "kind, enabled, cadence_secs, next_run_at, last_run_at, \
                             last_status, last_detail, last_duration_ms";

// ── endpoints (admin) ───────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/v1/ops/sweeps",
    tag = "ops",
    description = "Every configured org-intelligence sweep with its cadence and last-run status. Admin scope.",
    responses((status = 200, description = "Sweep schedules", body = SweepsResponse))
)]
pub(crate) async fn sweeps_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<SweepsResponse>, HttpError> {
    auth_of(&state, &headers, "admin").await?;
    let rows = sqlx::query(&format!(
        "SELECT {SCHEDULE_COLS} FROM sweep_schedules ORDER BY kind"
    ))
    .fetch_all(state.store.pool())
    .await
    .map_err(internal)?;
    Ok(Json(SweepsResponse {
        sweeps: rows.iter().map(row_to_schedule).collect(),
    }))
}

#[utoipa::path(
    put,
    path = "/v1/ops/sweeps/{kind}",
    tag = "ops",
    params(("kind" = String, Path, description = "Sweep kind: divergence | health_snapshot")),
    request_body = UpdateSweepBody,
    description = "Enable/disable a sweep or retune its cadence. Enabling schedules it to run on the next scheduler tick. Admin scope.",
    responses(
        (status = 200, description = "Updated schedule", body = SweepSchedule),
        (status = 404, description = "No such sweep kind"),
    )
)]
pub(crate) async fn sweep_update(
    State(state): State<Arc<AppState>>,
    Path(kind): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateSweepBody>,
) -> Result<Json<SweepSchedule>, HttpError> {
    auth_of(&state, &headers, "admin").await?;
    if let Some(c) = body.cadence_secs {
        if c < MIN_CADENCE_SECS {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("cadence_secs must be at least {MIN_CADENCE_SECS}"),
            )
                .into());
        }
    }
    // COALESCE keeps unset fields; enabling from an idle state arms next_run_at
    // so the sweep runs on the next scheduler tick rather than waiting a cadence.
    let row = sqlx::query(&format!(
        "UPDATE sweep_schedules SET
           enabled = COALESCE($2, enabled),
           cadence_secs = COALESCE($3, cadence_secs),
           next_run_at = CASE
             WHEN COALESCE($2, enabled) AND next_run_at IS NULL THEN now()
             ELSE next_run_at END,
           updated_at = now()
         WHERE kind = $1
         RETURNING {SCHEDULE_COLS}"
    ))
    .bind(&kind)
    .bind(body.enabled)
    .bind(body.cadence_secs)
    .fetch_optional(state.store.pool())
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, format!("no sweep kind '{kind}'")))?;
    Ok(Json(row_to_schedule(&row)))
}

#[utoipa::path(
    post,
    path = "/v1/ops/sweeps/{kind}/run",
    tag = "ops",
    params(("kind" = String, Path, description = "Sweep kind: divergence | health_snapshot")),
    description = "Trigger a one-shot run now — marks the sweep due so the worker picks it up within ~20s. Works whether or not the sweep is enabled. Admin scope.",
    responses(
        (status = 200, description = "Run queued", body = RunSweepResponse),
        (status = 404, description = "No such sweep kind"),
    )
)]
pub(crate) async fn sweep_run(
    State(state): State<Arc<AppState>>,
    Path(kind): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RunSweepResponse>, HttpError> {
    auth_of(&state, &headers, "admin").await?;
    // Arm next_run_at without touching `enabled`: a disabled sweep runs once and
    // (see claim SQL) leaves next_run_at NULL afterwards; an enabled one reslots.
    let row = sqlx::query(
        "UPDATE sweep_schedules SET next_run_at = now(), updated_at = now()
         WHERE kind = $1 RETURNING kind, next_run_at",
    )
    .bind(&kind)
    .fetch_optional(state.store.pool())
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, format!("no sweep kind '{kind}'")))?;
    Ok(Json(RunSweepResponse {
        kind: row.get("kind"),
        queued: true,
        next_run_at: row.get("next_run_at"),
    }))
}

// ── scheduler (worker) ──────────────────────────────────────────────────

/// Atomically claim every due sweep and spawn it. Called by the worker loop
/// every [`SCHED_INTERVAL`]. Returns how many sweeps were dispatched.
///
/// Claiming and clock-advance happen in one UPDATE so two scheduler ticks (or a
/// second worker) can't double-dispatch the same sweep: the row flips to
/// `running` and its `next_run_at` moves in the same statement that returns it.
/// The spawned task owns clones of the admin pool + provider so a long LLM scan
/// runs off the worker's critical path.
pub async fn run_due(admin: &PgPool, provider: Arc<dyn ChatProvider>) -> anyhow::Result<usize> {
    let claimed: Vec<String> = sqlx::query_scalar(&format!(
        "UPDATE sweep_schedules SET
           last_status = 'running',
           last_run_at = now(),
           next_run_at = CASE WHEN enabled THEN now() + make_interval(secs => cadence_secs) ELSE NULL END,
           updated_at = now()
         WHERE kind IN (
           SELECT kind FROM sweep_schedules
           WHERE next_run_at IS NOT NULL AND next_run_at <= now()
             AND (last_status IS DISTINCT FROM 'running'
                  OR last_run_at < now() - interval '{RUNNING_STALE}')
         )
         RETURNING kind"
    ))
    .fetch_all(admin)
    .await?;

    for kind in &claimed {
        let admin = admin.clone();
        let provider = provider.clone();
        let kind = kind.clone();
        tokio::spawn(async move { execute(admin, provider, kind).await });
    }
    Ok(claimed.len())
}

/// Run one claimed sweep to completion and record its outcome on its row.
async fn execute(admin: PgPool, provider: Arc<dyn ChatProvider>, kind: String) {
    let start = tokio::time::Instant::now();
    let outcome: anyhow::Result<String> = match kind.as_str() {
        "divergence" => brainiac_pipeline::divergence::scan_all(&admin, provider.as_ref())
            .await
            .map(|s| format!("{} clusters, {} divergences", s.clusters, s.divergences)),
        "health_snapshot" => crate::console::snapshot_all_orgs(&admin)
            .await
            .map(|(_, detail)| detail),
        other => Err(anyhow::anyhow!("unknown sweep kind '{other}'")),
    };
    let ms = start.elapsed().as_millis() as i64;
    let (status, detail) = match &outcome {
        Ok(d) => ("ok", clip_detail(d)),
        Err(e) => ("error", clip_detail(&format!("{e:#}"))),
    };
    if let Err(e) = record_result(&admin, &kind, status, &detail, ms).await {
        tracing::error!(kind = %kind, error = %e, "failed to record sweep result");
    } else if status == "ok" {
        tracing::info!(kind = %kind, detail = %detail, ms, "sweep finished");
    } else {
        tracing::error!(kind = %kind, detail = %detail, ms, "sweep failed");
    }
}

async fn record_result(
    admin: &PgPool,
    kind: &str,
    status: &str,
    detail: &str,
    duration_ms: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE sweep_schedules
         SET last_status = $2, last_detail = $3, last_duration_ms = $4, updated_at = now()
         WHERE kind = $1",
    )
    .bind(kind)
    .bind(status)
    .bind(detail)
    .bind(duration_ms)
    .execute(admin)
    .await?;
    Ok(())
}
