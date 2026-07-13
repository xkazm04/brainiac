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
        match process_job(
            store,
            providers,
            embedder,
            embedding_version,
            &job,
            &mut stats,
        )
        .await
        {
            Ok(()) => queue::complete(store.pool(), &job).await?,
            Err(e) => {
                tracing::error!(job = job.id, error = %e, "ingest job failed");
                queue::fail(store.pool(), &job, FAIL_BASE_BACKOFF_SECS).await?;
            }
        }
        stats.jobs += 1;
    }
    Ok(stats)
}

async fn process_job(
    store: &Store,
    providers: &ProviderRouter,
    embedder: &dyn Embedder,
    embedding_version: i32,
    job: &queue::Job,
    stats: &mut TickStats,
) -> Result<()> {
    let org_id: Uuid = serde_json::from_value(job.payload["org_id"].clone())?;
    let source_id: Uuid = serde_json::from_value(job.payload["source_id"].clone())?;
    // Worker authority: later stages (contradict, promote) must read back the
    // team-visible memories the extract stage just wrote for any team of the
    // org — worker_tx sets the audited app.worker read scope (org + team
    // tiers, never private; migrations/0002_worker_read.sql).
    let principal = pipeline_principal(org_id);
    let mut tx = store.worker_tx(&principal).await?;

    let (team_id, raw_text) = brainiac_store::governance::get_source_text(&mut tx, source_id)
        .await?
        .context("source not found")?;

    // extract (+ embed inline)
    let extracted = extract::run_extract(
        &mut tx,
        providers.for_stage(Stage::Extract),
        embedder,
        embedding_version,
        org_id,
        team_id,
        source_id,
        &raw_text,
    )
    .await?;
    stats.memories += extracted.memories_written;
    stats.chunks += extracted.chunks;
    stats.parse_failures += extracted.parse_failures;
    stats.repairs += extracted.repairs;
    stats.deduped += extracted.deduped;
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
        stats.contradictions_opened += c.opened;

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
                stats.auto_promoted += 1;
            }
            _ => stats.needs_review += 1,
        }
    }

    tx.commit().await?;
    Ok(())
}
