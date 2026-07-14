//! Worker loop: claim `ingest` jobs and run the full stage chain for each
//! source. One job = one source end-to-end in v0 (see lib.rs).

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{MemoryStatus, PolicyDecision};
use brainiac_gateway::{ProviderRouter, Stage};
use brainiac_store::{queue, Store};
use serde_json::json;
use uuid::Uuid;

use futures::stream::StreamExt;

use crate::policy::{PolicyContext, PolicyEngine};
use crate::{compose, contradict, extract, pipeline_principal, resolve};

pub const INGEST_QUEUE: &str = "ingest";

/// Tunables for the drain loop. Every field is env-overridable (see
/// [`WorkerConfig::from_env`]); [`Default`] reproduces the pre-Direction-1
/// hardcoded behaviour so nothing changes for a caller that doesn't opt in.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// How many jobs from one claimed batch run at once (Direction 1). The
    /// SKIP-LOCKED claim already makes each job's own transaction independent,
    /// so this is pure IO-concurrency: while one job awaits an LLM call another
    /// makes progress. Default 4 — a middle ground that overlaps the LLM-bound
    /// stalls of a handful of jobs without opening a connection per job (each
    /// in-flight job holds one pool connection for its chain).
    pub concurrency: usize,
    /// Jobs claimed per [`queue::read`] (one visibility window). Default 8.
    pub batch: i64,
    /// Visibility window a claimed job is hidden for before redelivery.
    /// Default 300s — must comfortably exceed the slowest full chain.
    pub visibility_secs: i64,
    /// First-retry backoff handed to [`queue::fail`]; doubles per attempt and is
    /// capped inside the queue (`queue::BACKOFF_CAP_SECS`). Default 30s.
    pub backoff_base_secs: i64,
}

pub const DEFAULT_CONCURRENCY: usize = 4;
pub const DEFAULT_BATCH: i64 = 8;
pub const DEFAULT_VISIBILITY_SECS: i64 = 300;
pub const DEFAULT_BACKOFF_BASE_SECS: i64 = 30;

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: DEFAULT_CONCURRENCY,
            batch: DEFAULT_BATCH,
            visibility_secs: DEFAULT_VISIBILITY_SECS,
            backoff_base_secs: DEFAULT_BACKOFF_BASE_SECS,
        }
    }
}

impl WorkerConfig {
    /// Read overrides from the environment, falling back to [`Default`] for any
    /// unset/unparseable var:
    /// - `BRAINIAC_WORKER_CONCURRENCY` (default 4, floored at 1)
    /// - `BRAINIAC_WORKER_BATCH` (default 8, floored at 1)
    /// - `BRAINIAC_WORKER_VISIBILITY_SECS` (default 300)
    /// - `BRAINIAC_WORKER_BACKOFF_BASE_SECS` (default 30)
    pub fn from_env() -> Self {
        fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(default)
        }
        let d = Self::default();
        Self {
            concurrency: env_parse("BRAINIAC_WORKER_CONCURRENCY", d.concurrency).max(1),
            batch: env_parse("BRAINIAC_WORKER_BATCH", d.batch).max(1),
            visibility_secs: env_parse("BRAINIAC_WORKER_VISIBILITY_SECS", d.visibility_secs),
            backoff_base_secs: env_parse("BRAINIAC_WORKER_BACKOFF_BASE_SECS", d.backoff_base_secs),
        }
    }
}

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

/// Process up to `cfg.batch` ingest jobs, up to `cfg.concurrency` at a time.
/// Returns per-tick stats; callers loop.
///
/// Concurrency (Direction 1): the claimed batch is drained with
/// `buffer_unordered`, so `cfg.concurrency` jobs are in flight at once. This is
/// IO-concurrency on the single worker task — while one job awaits an LLM call
/// or a DB round-trip, others make progress — which is exactly the bottleneck
/// (the chain is LLM-bound, not CPU-bound). Per-job isolation is total: each job
/// owns its own transaction ([`process_job`] opens a fresh `worker_tx`) and the
/// SKIP-LOCKED claim already guaranteed no two jobs share a row, so one job's
/// `Err` → its own `fail()` and never touches another's transaction. Only an
/// *infrastructure* failure (a queue/DB write in the ack path) propagates out of
/// a job future and aborts the tick — the same jobs would fail the old
/// sequential loop's `?` too.
pub async fn tick(
    store: &Store,
    providers: &ProviderRouter,
    embedder: &dyn Embedder,
    embedding_version: i32,
    cfg: &WorkerConfig,
) -> Result<TickStats> {
    let jobs = queue::read(store.pool(), INGEST_QUEUE, cfg.batch, cfg.visibility_secs).await?;
    let concurrency = cfg.concurrency.max(1);

    // Each job future resolves to Result<RunStats>: Ok carries what the run
    // produced (folded below), Err is an infrastructure failure that aborts the
    // whole tick. A job's *own* processing error is handled inside the future
    // (recorded + fail()) and yields Ok(RunStats) — it never aborts the tick or
    // disturbs a sibling. Ordering of completion is irrelevant: the fold is
    // commutative and TickStats is built only here, after every future settles.
    let outcomes: Vec<Result<RunStats>> =
        futures::stream::iter(jobs.into_iter().map(|job| {
            process_claimed_job(store, providers, embedder, embedding_version, cfg, job)
        }))
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut stats = TickStats::default();
    for outcome in outcomes {
        let run = outcome?;
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

/// One pass of the compose loop (§8.2): recompose every page whose memories
/// moved, and write each result as a revision.
///
/// Deliberately NOT on the job queue in KB1. Dirty pages are already a durable,
/// idempotent work list in the database (`documents.dirty_at`) — enqueueing a
/// job per dirty page would add a second, weaker source of truth about what
/// needs composing, and a lost or duplicated message would either strand a
/// stale page or burn tokens recomposing a clean one. The queue earns its keep
/// when compose becomes multi-worker; today the invariant is worth more.
///
/// Each page composes in its own transaction: one page's bad binding or
/// provider error must not roll back its neighbours' good revisions.
pub async fn compose_tick(
    store: &Store,
    providers: &ProviderRouter,
    embedder: &dyn Embedder,
    embedding_version: i32,
    org_id: Uuid,
    limit: i64,
) -> Result<ComposeStats> {
    let mut stats = ComposeStats::default();

    // Read the work list under worker authority (it must see team pages too).
    let worker = pipeline_principal(org_id);
    let dirty = {
        let mut tx = store.worker_tx(&worker).await?;
        let d = brainiac_store::documents::dirty_documents(&mut tx, limit).await?;
        tx.commit().await?;
        d
    };

    for doc in dirty {
        // Visibility cap (§8.2): an ORG page composes as a principal with no
        // team memberships, so RLS itself makes team-private memories
        // unreachable — the leak invariant is enforced by the same code path a
        // user query takes, not by a check we could forget. Team pages need the
        // worker scope to see their own team's memories and are capped in code
        // (compose::admits).
        let principal = match doc.visibility {
            brainiac_core::Visibility::Org => compose::compose_principal(org_id),
            _ => worker.clone(),
        };
        let mut tx = match doc.visibility {
            brainiac_core::Visibility::Org => store.scoped_tx(&principal).await?,
            _ => store.worker_tx(&principal).await?,
        };

        let outcome = compose::compose_document(
            &mut tx,
            store.pool(),
            providers.for_stage(Stage::Compose),
            embedder,
            embedding_version,
            &doc,
            "memory_change",
        )
        .await;

        match outcome {
            Ok(out) => {
                brainiac_store::documents::insert_revision(
                    &mut tx,
                    &brainiac_store::documents::NewRevision {
                        id: Uuid::new_v4(),
                        document_id: doc.id,
                        org_id,
                        content_md: out.content_md,
                        composed_from: out.composed_from,
                        trigger: out.trigger,
                        policy_decision: out.policy,
                    },
                )
                .await?;
                tx.commit().await?;
                match out.policy {
                    brainiac_core::RevisionPolicy::AutoPublished => stats.auto_published += 1,
                    _ => stats.needs_review += 1,
                }
                stats.composed += 1;
                tracing::info!(
                    document = %doc.slug,
                    policy = out.policy.as_str(),
                    reason = %out.policy_reason,
                    "page recomposed"
                );
            }
            Err(e) => {
                // The page stays dirty: a failed compose must retry, never
                // silently leave a stale page looking fresh.
                tracing::error!(document = %doc.slug, error = %e, "compose failed");
                stats.failed += 1;
            }
        }
    }
    Ok(stats)
}

#[derive(Debug, Default, Clone)]
pub struct ComposeStats {
    pub composed: usize,
    pub auto_published: usize,
    pub needs_review: usize,
    pub failed: usize,
}

/// Run one claimed job end-to-end and ack it. Returns the run's stats on
/// success OR on an adjudicated job failure (both are normal tick outcomes);
/// returns `Err` only when the ack-path infrastructure (run-row write, queue
/// complete/fail) itself fails, which aborts the tick.
async fn process_claimed_job(
    store: &Store,
    providers: &ProviderRouter,
    embedder: &dyn Embedder,
    embedding_version: i32,
    cfg: &WorkerConfig,
    job: queue::Job,
) -> Result<RunStats> {
    // Identity + run id up front: a pipeline_runs row is written for this
    // source whether the job succeeds or fails, and it must be org-scoped.
    let run_id = Uuid::new_v4();
    let started_at = chrono::Utc::now();
    let mut run = RunStats::default();

    match parse_job_ids(&job) {
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
                    queue::fail(store.pool(), &job, cfg.backoff_base_secs).await?;
                }
            }
        }
        Err(e) => {
            // Unparseable payload: no org to scope a run row to, so none is
            // written — just fail the job through the attempt-aware path.
            tracing::error!(job = job.id, error = %e, "ingest job payload unparseable");
            queue::fail(store.pool(), &job, cfg.backoff_base_secs).await?;
        }
    }

    Ok(run)
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

        // Pass the just-opened contradiction count into the policy so a conflicting
        // memory is held for review instead of auto-promoted into retrieval.
        let ctx = PolicyContext {
            open_contradictions: c.opened,
        };
        let (decision, rule) = engine.evaluate(m, MemoryStatus::Candidate, &ctx);
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
