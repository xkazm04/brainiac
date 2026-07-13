//! Worker loop: claim `ingest` jobs and run the full stage chain for each
//! source. One job = one source end-to-end in v0 (see lib.rs).

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{MemoryStatus, PolicyDecision};
use brainiac_gateway::{ProviderRouter, Stage};
use brainiac_store::{queue, Store};
use serde_json::json;
use uuid::Uuid;

use crate::policy::PolicyEngine;
use crate::{contradict, extract, pipeline_principal, resolve};

pub const INGEST_QUEUE: &str = "ingest";
const VISIBILITY_SECS: i64 = 300;

/// First-retry backoff handed to [`queue::fail`]; it doubles per attempt and is
/// capped inside the queue (`queue::BACKOFF_CAP_SECS`). A transient failure
/// retries quickly, a persistent one backs off toward the cap before the
/// attempt budget dead-letters it.
const FAIL_BASE_BACKOFF_SECS: i64 = 30;

/// Enqueue a source for the pipeline.
pub async fn enqueue_source(store: &Store, org_id: Uuid, source_id: Uuid) -> Result<i64> {
    queue::send(
        store.pool(),
        INGEST_QUEUE,
        &json!({ "org_id": org_id, "source_id": source_id }),
    )
    .await
}

#[derive(Debug, Default)]
pub struct TickStats {
    pub jobs: usize,
    pub memories: usize,
    pub auto_promoted: usize,
    pub needs_review: usize,
    pub contradictions_opened: usize,
    /// Extract cost/resilience across the tick: chunks = primary LLM calls
    /// (sources split when long), repairs = extra calls that recovered
    /// malformed JSON, parse_failures = malformed first responses, deduped =
    /// memories skipped as already-present for their source.
    pub chunks: usize,
    pub parse_failures: usize,
    pub repairs: usize,
    pub deduped: usize,
}

/// Per-source stats accumulated while a single job runs, so one pipeline_runs
/// row can record exactly what this run produced (Direction 2). Folded into the
/// tick-wide [`TickStats`] afterwards.
#[derive(Debug, Default)]
struct RunStats {
    memories: usize,
    auto_promoted: usize,
    needs_review: usize,
    contradictions_opened: usize,
    entities_created: usize,
    entities_resolved: usize,
    chunks: usize,
    parse_failures: usize,
    repairs: usize,
    deduped: usize,
    model_ref: Option<String>,
}

fn parse_job_ids(job: &queue::Job) -> Result<(Uuid, Uuid)> {
    let org_id: Uuid = serde_json::from_value(job.payload["org_id"].clone())?;
    let source_id: Uuid = serde_json::from_value(job.payload["source_id"].clone())?;
    Ok((org_id, source_id))
}

/// Cause chain of a run failure, bounded so a run row's `detail` can't grow
/// without limit from a deeply-nested error.
fn summarize_error(e: &anyhow::Error) -> String {
    const MAX: usize = 500;
    let s = format!("{e:#}");
    if s.chars().count() > MAX {
        s.chars().take(MAX).collect::<String>() + "…"
    } else {
        s
    }
}

/// Process up to `batch` ingest jobs. Returns per-tick stats; callers loop.
pub async fn tick(
    store: &Store,
    providers: &ProviderRouter,
    embedder: &dyn Embedder,
    embedding_version: i32,
    batch: i64,
) -> Result<TickStats> {
    let mut stats = TickStats::default();
    let jobs = queue::read(store.pool(), INGEST_QUEUE, batch, VISIBILITY_SECS).await?;
    for job in jobs {
        // Identity + run id up front: a pipeline_runs row is written for this
        // source whether the job succeeds or fails, and it must be org-scoped.
        let parsed = parse_job_ids(&job);
        let run_id = Uuid::new_v4();
        let started_at = chrono::Utc::now();
        let mut run = RunStats::default();

        match parsed {
            Ok((org_id, source_id)) => {
                match process_job(
                    store,
                    providers,
                    embedder,
                    embedding_version,
                    org_id,
                    source_id,
                    run_id,
                    &mut run,
                )
                .await
                {
                    Ok(()) => {
                        // Run record written AFTER the job's own transaction
                        // commits (see write_pipeline_run docs for the atomicity
                        // choice): the memories exist and carry this run_id via
                        // provenance, and now the run row records the outcome.
                        write_pipeline_run(
                            store, org_id, source_id, run_id, started_at, "ok", None, &run,
                        )
                        .await?;
                        queue::complete(store.pool(), &job).await?;
                    }
                    Err(e) => {
                        tracing::error!(job = job.id, error = %e, "ingest job failed");
                        // The job tx rolled back (its memories/provenance are
                        // gone), but we still record a failed run row with the
                        // error summary — the whole point of writing the row
                        // outside the job transaction.
                        write_pipeline_run(
                            store,
                            org_id,
                            source_id,
                            run_id,
                            started_at,
                            "failed",
                            Some(&summarize_error(&e)),
                            &run,
                        )
                        .await?;
                        queue::fail(store.pool(), &job, FAIL_BASE_BACKOFF_SECS).await?;
                    }
                }
            }
            Err(e) => {
                // Unparseable payload: no org to scope a run row to, so none is
                // written — just fail the job through the attempt-aware path.
                tracing::error!(job = job.id, error = %e, "ingest job payload unparseable");
                queue::fail(store.pool(), &job, FAIL_BASE_BACKOFF_SECS).await?;
            }
        }

        stats.memories += run.memories;
        stats.auto_promoted += run.auto_promoted;
        stats.needs_review += run.needs_review;
        stats.contradictions_opened += run.contradictions_opened;
        stats.chunks += run.chunks;
        stats.parse_failures += run.parse_failures;
        stats.repairs += run.repairs;
        stats.deduped += run.deduped;
        stats.jobs += 1;
    }
    Ok(stats)
}

/// Write the pipeline_runs record for one processed source.
///
/// Atomicity choice: the row is written in its OWN short scoped transaction,
/// immediately AFTER the job's transaction has settled — not inside it. Writing
/// it inside the job tx would mean a FAILED job (which rolls back) leaves no run
/// row at all; we specifically want a failed job to record a run row carrying
/// its error summary. The narrow cost is that a crash between the job commit and
/// this write could leave a committed job without its observability row; that is
/// acceptable for an audit record (the memories still exist and still link the
/// run_id through provenance). pipeline_runs is org-scoped by RLS, so this uses
/// a scoped_tx for the org.
#[allow(clippy::too_many_arguments)]
async fn write_pipeline_run(
    store: &Store,
    org_id: Uuid,
    source_id: Uuid,
    run_id: Uuid,
    started_at: chrono::DateTime<chrono::Utc>,
    status: &str,
    detail: Option<&str>,
    run: &RunStats,
) -> Result<()> {
    let principal = pipeline_principal(org_id);
    let mut tx = store.scoped_tx(&principal).await?;
    sqlx::query(
        "INSERT INTO pipeline_runs
            (id, org_id, stage, status, detail, started_at, finished_at, source_id, model_ref,
             memories_written, entities_created, entities_resolved, contradictions_opened,
             auto_promoted, needs_review, chunks, llm_calls, repairs, parse_failures, deduped)
         VALUES ($1,$2,'pipeline',$3,$4,$5,now(),$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
    )
    .bind(run_id)
    .bind(org_id)
    .bind(status)
    .bind(detail)
    .bind(started_at)
    .bind(source_id)
    .bind(run.model_ref.as_deref())
    .bind(run.memories as i32)
    .bind(run.entities_created as i32)
    .bind(run.entities_resolved as i32)
    .bind(run.contradictions_opened as i32)
    .bind(run.auto_promoted as i32)
    .bind(run.needs_review as i32)
    .bind(run.chunks as i32)
    .bind((run.chunks + run.repairs) as i32)
    .bind(run.repairs as i32)
    .bind(run.parse_failures as i32)
    .bind(run.deduped as i32)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn process_job(
    store: &Store,
    providers: &ProviderRouter,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    source_id: Uuid,
    run_id: Uuid,
    run: &mut RunStats,
) -> Result<()> {
    // Worker authority: later stages (contradict, promote) must read back the
    // team-visible memories the extract stage just wrote for any team of the
    // org — worker_tx sets the audited app.worker read scope (org + team
    // tiers, never private; migrations/0002_worker_read.sql).
    let principal = pipeline_principal(org_id);
    let mut tx = store.worker_tx(&principal).await?;

    let (team_id, raw_text) = brainiac_store::governance::get_source_text(&mut tx, source_id)
        .await?
        .context("source not found")?;

    // extract (+ embed inline). run_id threads onto the provenance row so every
    // memory written here links back to this run.
    let extracted = extract::run_extract(
        &mut tx,
        providers.for_stage(Stage::Extract),
        embedder,
        embedding_version,
        org_id,
        team_id,
        source_id,
        &raw_text,
        Some(run_id),
    )
    .await?;
    run.memories += extracted.memories_written;
    run.entities_created += extracted.entities_created.len();
    run.chunks += extracted.chunks;
    run.parse_failures += extracted.parse_failures;
    run.repairs += extracted.repairs;
    run.deduped += extracted.deduped;
    if run.model_ref.is_none() {
        run.model_ref = extracted.model_ref.clone();
    }
    // Cost + resilience per source: number of chunks (= primary calls) and
    // total LLM calls (chunks + repairs), plus memories deduped away (retry
    // redelivery / chunk overlap).
    tracing::info!(
        source = %source_id,
        chunks = extracted.chunks,
        llm_calls = extracted.chunks + extracted.repairs,
        memories = extracted.memories_written,
        deduped = extracted.deduped,
        parse_failures = extracted.parse_failures,
        "source extracted"
    );

    // resolve every NEW entity this source introduced
    for entity_id in &extracted.entities_created {
        use sqlx::Row;
        let row = sqlx::query("SELECT name, kind, aliases FROM entities WHERE id = $1")
            .bind(entity_id)
            .fetch_one(&mut *tx)
            .await?;
        let name: String = row.get("name");
        let kind: String = row.get("kind");
        let aliases: Vec<String> = row.get("aliases");
        resolve::resolve_entity(
            &mut tx,
            providers.for_stage(Stage::Resolve),
            embedder,
            embedding_version,
            org_id,
            *entity_id,
            &name,
            &kind,
            &aliases,
        )
        .await?;
        run.entities_resolved += 1;
    }

    // contradict + promote per new memory
    let new_memories = brainiac_store::memories::get_by_ids(&mut tx, &extracted.memory_ids).await?;
    let engine = PolicyEngine;
    for m in &new_memories {
        let c = contradict::run_contradict(
            &mut tx,
            providers.for_stage(Stage::Contradict),
            embedder,
            embedding_version,
            org_id,
            m,
        )
        .await?;
        run.contradictions_opened += c.opened;

        let (decision, rule) = engine.evaluate(m, MemoryStatus::Candidate);
        brainiac_store::governance::insert_promotion(
            &mut tx,
            org_id,
            m.id,
            m.status,
            MemoryStatus::Candidate,
            decision,
            rule,
        )
        .await?;
        match decision {
            PolicyDecision::AutoApproved => {
                brainiac_store::governance::set_memory_status(
                    &mut tx,
                    m.id,
                    MemoryStatus::Candidate,
                )
                .await?;
                run.auto_promoted += 1;
            }
            _ => run.needs_review += 1,
        }
    }

    tx.commit().await?;
    Ok(())
}
