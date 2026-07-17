//! brainiac — the single deployable binary (ARCHITECTURE.md §1):
//! `serve` (REST), `worker` (pipeline), `eval` (fixture harness).

use brainiac_server::http;

use std::sync::Arc;

use anyhow::{Context, Result};
use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::rerank::{LexicalOverlapReranker, Reranker};
use brainiac_gateway::QwenEmbedder;
use brainiac_store::Store;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "brainiac", about = "GitOps for organizational AI knowledge")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the REST API (v0 surface; MCP arrives next).
    Serve {
        #[arg(long, default_value = "127.0.0.1:8600")]
        bind: String,
        /// Also drain the pipeline queue in-process. For constrained hosts
        /// (a 1 GB free-tier VM) — one runtime and one pool instead of two.
        #[arg(long)]
        with_worker: bool,
        /// With --with-worker: use the deterministic mock provider.
        #[arg(long)]
        mock: bool,
        /// Stage-5 reranker (ARCHITECTURE.md §4): `none` (default) or
        /// `lexical` (deterministic overlap scorer — the bake-off seam; the
        /// ONNX cross-encoder plugs in here later).
        #[arg(long)]
        reranker: Option<String>,
    },
    /// Run the MCP server on stdio (agent surface).
    Mcp,
    /// Backfill embeddings for a swapped embedder (ARCHITECTURE.md §3 stage 8).
    /// Ensures the target version (auto-creating its HNSW index) and re-embeds
    /// every memory + canonical entity that lacks an embedding in it — a
    /// cross-org OPERATOR sweep. Resumable and idempotent: safe to interrupt and
    /// re-run. Point it at the production database.
    Reembed {
        /// Target embedding backend: `deterministic` (default) or `qwen`.
        #[arg(long)]
        embedder: Option<String>,
    },
    /// Run the pipeline worker loop.
    Worker {
        /// Use the deterministic mock provider (demo/dev only).
        #[arg(long)]
        mock: bool,
    },
    /// Scan every org for PRACTICE DIVERGENCES — the standardization sweep. An
    /// LLM adjudicates cross-team clusters (anchored on shared canonical
    /// entities) into named practices with a recommended standard, stored for
    /// the /v1/analytics/practice-divergence surface. Operator/scheduled sweep;
    /// needs a real provider (QWEN_API_KEY). Point it at the production DB.
    ScanDivergence,
    /// Harvest an OKF knowledge bundle (a repo wiki à la OpenWiki) into the
    /// extraction pipeline: each concept document becomes an `okf` source →
    /// candidate memories → the review gate. Never direct-to-canonical: the
    /// wiki is a witness, not an authority. Idempotent by file content —
    /// re-running ingests only what changed. Brainiac's own published pages
    /// (x_brainiac_* frontmatter) are refused, closing the self-citation loop.
    OkfHarvest {
        /// Org to ingest into.
        #[arg(long)]
        org: uuid::Uuid,
        /// Path to the bundle directory (e.g. a checkout's docs/okf).
        #[arg(long)]
        path: String,
        /// Attribute the sources to a team (org-wide otherwise).
        #[arg(long)]
        team: Option<uuid::Uuid>,
    },
    /// Run an eval profile against a fixture tree. DESTRUCTIVE to the
    /// connected database (re-seeds the tenant) — point it at a dev/eval DB.
    Eval {
        #[arg(long, default_value = "fixtures/v1")]
        fixtures: String,
        #[arg(long, default_value = "retrieval")]
        profile: String,
        /// Bake-off grid (EVAL.md §3.1): run the `retrieval` profile across the
        /// cross-product of AVAILABLE backends (embedders {deterministic, qwen}
        /// × rerankers {none, lexical}) and write ONE decision-table artifact
        /// (JSON + markdown) to `--out`. EXPLORATORY: no baselines or regression
        /// gates are evaluated — it surfaces cross-config trade-offs. Unavailable
        /// backends (qwen without an API key) are listed as skipped-with-reason.
        /// Ignores `--profile`, `--embedder`, `--reranker`, `--baseline`.
        #[arg(long)]
        grid: bool,
        /// Embedding backend: `deterministic` (default) or `qwen`
        /// (DashScope text-embedding-v4; needs QWEN_API_KEY/DASHSCOPE_API_KEY).
        #[arg(long)]
        embedder: Option<String>,
        /// Stage-5 reranker for the `retrieval` profile: `none` (default) or
        /// `lexical` (deterministic overlap scorer — the bake-off seam). Tagged
        /// into the report; the regression gate refuses a cross-reranker
        /// baseline comparison, so recalibrate a per-reranker baseline.
        #[arg(long)]
        reranker: Option<String>,
        #[arg(long)]
        out: Option<String>,
        /// Also write the per-query drill-down (expected vs got per QA/
        /// temporal/leak item, failures first) to this path.
        #[arg(long)]
        diagnostics: Option<String>,
        /// Enforce §3.2 regression gates against this committed baseline
        /// (exit 1 on any breach). CI passes results/baseline.json.
        #[arg(long)]
        baseline: Option<String>,
        /// Recalibrate: write this run's scores as the new baseline. A
        /// deliberate act — commit the diff with a reason.
        #[arg(long)]
        write_baseline: Option<String>,
        /// Run the profile N times (tenant reset between runs) and gate on the
        /// MEAN. Real-model extraction/composition is high-variance (recall
        /// spanned 0.25–0.54 across identical single runs); the mean of N runs
        /// tightens the honest regression band by ~1/√N. `extraction` and
        /// `docs` profiles only; each sample costs a full real-model run.
        #[arg(long, default_value_t = 1)]
        samples: usize,
    },
    /// Fixture-corpus tooling (lint, schema export). Pure filesystem — no
    /// database needed.
    Fixtures {
        #[command(subcommand)]
        cmd: FixturesCmd,
    },
    /// Write the OpenAPI document (the same one `GET /openapi.json` serves).
    /// The console generates its TypeScript types from this file, so it is
    /// committed and regenerated whenever a response shape changes.
    Openapi {
        #[arg(long, default_value = "openapi.json")]
        out: String,
    },
}

#[derive(Subcommand)]
enum FixturesCmd {
    /// Validate a fixture tree; emit structured diagnostics. Exit 1 on any
    /// error-severity finding.
    Lint {
        #[arg(long, default_value = "fixtures/v1")]
        fixtures: String,
        /// Output format: text | json | github (workflow annotations).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Export JSON Schemas for the fixture YAML files (editor validation)
    /// from the loader's own serde structs.
    Schema {
        #[arg(long, default_value = "fixtures/schema")]
        out: String,
    },
}

fn database_url() -> Result<String> {
    std::env::var("DATABASE_URL").context("DATABASE_URL must be set")
}

/// Pick the embedding backend. `name` (CLI) wins over BRAINIAC_EMBEDDER (env);
/// default is the zero-dependency deterministic embedder.
fn embedder_select(name: Option<&str>) -> Result<Arc<dyn Embedder>> {
    let choice = match name {
        Some(n) => n.to_string(),
        None => std::env::var("BRAINIAC_EMBEDDER").unwrap_or_else(|_| "deterministic".into()),
    };
    match choice.as_str() {
        "deterministic" => Ok(Arc::new(DeterministicEmbedder::default())),
        "qwen" => {
            let e = QwenEmbedder::from_env()
                .context("embedder=qwen needs QWEN_API_KEY or DASHSCOPE_API_KEY")?;
            Ok(Arc::new(e))
        }
        other => anyhow::bail!("unknown embedder `{other}` (deterministic|qwen)"),
    }
}

/// Pick the stage-5 reranker, mirroring [`embedder_select`]. `name` (CLI) wins
/// over BRAINIAC_RERANKER (env); default `none` = no reranker (retrieval is
/// byte-identical to the pre-stage-5 path). `lexical` is the deterministic
/// model-free seam; the ONNX cross-encoder registers here later.
fn reranker_select(name: Option<&str>) -> Result<Option<Arc<dyn Reranker>>> {
    let choice = match name {
        Some(n) => n.to_string(),
        None => std::env::var("BRAINIAC_RERANKER").unwrap_or_else(|_| "none".into()),
    };
    match choice.as_str() {
        "none" => Ok(None),
        "lexical" => Ok(Some(Arc::new(LexicalOverlapReranker))),
        other => anyhow::bail!("unknown reranker `{other}` (none|lexical)"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Local dev convenience: pick up DATABASE_URL / QWEN_API_KEY from .env.
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Serve {
            bind,
            with_worker,
            mock,
            reranker,
        } => serve(&bind, with_worker, mock, reranker.as_deref()).await,
        Command::Mcp => mcp().await,
        Command::Reembed { embedder } => reembed(embedder.as_deref()).await,
        Command::Worker { mock } => worker(mock).await,
        Command::ScanDivergence => scan_divergence().await,
        Command::OkfHarvest { org, path, team } => okf_harvest(org, &path, team).await,
        Command::Eval {
            fixtures,
            profile,
            grid,
            embedder,
            reranker,
            out,
            diagnostics,
            baseline,
            write_baseline,
            samples,
        } => {
            eval(
                &fixtures,
                &profile,
                grid,
                embedder.as_deref(),
                reranker.as_deref(),
                out.as_deref(),
                diagnostics.as_deref(),
                baseline.as_deref(),
                write_baseline.as_deref(),
                samples.max(1),
            )
            .await
        }
        Command::Openapi { out } => openapi_dump(&out),
        Command::Fixtures { cmd } => match cmd {
            FixturesCmd::Lint { fixtures, format } => fixtures_lint(&fixtures, &format),
            FixturesCmd::Schema { out } => fixtures_schema(&out),
        },
    }
}

fn fixtures_lint(root: &str, format: &str) -> Result<()> {
    use brainiac_fixtures::validate::{Diagnostic, Severity};
    anyhow::ensure!(
        matches!(format, "text" | "json" | "github"),
        "unknown format `{format}` (text|json|github)"
    );
    let diags: Vec<Diagnostic> = match brainiac_fixtures::loader::load_unvalidated(root) {
        Ok(fx) => brainiac_fixtures::validate::lint(&fx),
        // A parse/IO failure is itself one diagnostic — lint output stays
        // machine-readable even when the tree doesn't deserialize.
        Err(e) => vec![Diagnostic {
            rule: "parse",
            severity: Severity::Error,
            file: String::new(),
            item: "(load)".into(),
            message: format!("{e:#}"),
        }],
    };
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&diags)?),
        "github" => {
            for d in &diags {
                let file = if d.file.is_empty() {
                    root.to_string()
                } else {
                    format!("{root}/{}", d.file)
                };
                println!(
                    "::error title=fixtures {}::{} {}: {}",
                    d.rule, file, d.item, d.message
                );
            }
        }
        _ => {
            for d in &diags {
                println!("{d}");
            }
        }
    }
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    eprintln!(
        "fixtures lint: {} finding(s), {errors} error(s) in {root}",
        diags.len()
    );
    if errors > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// OKF bundle → extraction pipeline (see `brainiac_pipeline::okf_ingest`).
/// Runs under the pipeline principal: sources are org rows, and the review
/// gate — not this command — decides what the org ends up believing.
async fn okf_harvest(org: uuid::Uuid, path: &str, team: Option<uuid::Uuid>) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let principal = brainiac_pipeline::pipeline_principal(org);
    let stats = brainiac_pipeline::okf_ingest::harvest(
        &store,
        &principal,
        team,
        std::path::Path::new(path),
        None,
    )
    .await?;
    println!(
        "harvested {path}: {} file(s) — {} ingested, {} unchanged, {} own page(s) refused, {} invalid",
        stats.files, stats.ingested, stats.unchanged, stats.own_pages, stats.invalid
    );
    println!(
        "extraction runs on the worker (`brainiac worker`, or serve --with-worker); \
         candidates land in the review queue, never straight into the corpus."
    );
    Ok(())
}

fn openapi_dump(out: &str) -> Result<()> {
    use utoipa::OpenApi;
    let doc = brainiac_server::openapi::ApiDoc::openapi();
    let json = doc.to_pretty_json()?;
    if let Some(parent) = std::path::Path::new(out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(
        out,
        json + "
",
    )?;
    eprintln!(
        "openapi: {} paths, {} schemas -> {out}",
        doc.paths.paths.len(),
        doc.components.as_ref().map_or(0, |c| c.schemas.len())
    );
    Ok(())
}

fn fixtures_schema(out: &str) -> Result<()> {
    let written = brainiac_fixtures::export::export_schemas(std::path::Path::new(out))?;
    eprintln!("fixtures schema: wrote {} file(s) to {out}", written.len());
    for f in written {
        eprintln!("  {f}");
    }
    Ok(())
}

async fn serve(
    bind: &str,
    with_worker: bool,
    mock: bool,
    reranker_name: Option<&str>,
) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = embedder_select(None)?;
    let reranker = reranker_select(reranker_name)?;

    // One shutdown signal shared by the server's graceful-shutdown future and
    // the in-process worker: ctrl_c flips it, both drain and exit cleanly.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Single-process mode for constrained hosts (a 1 GB free-tier VM can't
    // afford a second runtime + connection pool). The worker loop is just a
    // task on the same runtime; it shares the store and the embedder, and gets
    // the same shutdown signal so it finishes its in-flight batch on ctrl_c.
    let worker_handle = if with_worker {
        let store = store.clone();
        let embedder = Arc::clone(&embedder);
        let shutdown_rx = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = worker_loop(store, embedder, mock, shutdown_rx).await {
                tracing::error!(error = %e, "in-process worker stopped");
            }
        }))
    } else {
        None
    };

    if let Some(r) = &reranker {
        tracing::info!(reranker = r.model_name(), "stage-5 reranker enabled");
    }
    let app = http::router(store, embedder, reranker).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, with_worker, "brainiac REST listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("shutdown signal received; draining");
            let _ = shutdown_tx.send(true);
        })
        .await?;
    // Server has stopped accepting; let the worker finish its in-flight batch.
    if let Some(handle) = worker_handle {
        let _ = handle.await;
    }
    Ok(())
}

async fn mcp() -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = embedder_select(None)?;
    let state = brainiac_server::mcp::McpState::from_env(store, embedder).await?;
    brainiac_server::mcp::serve_stdio(std::sync::Arc::new(state)).await
}

/// Reembed backfill (ARCHITECTURE.md §3 stage 8). Runs on the admin
/// (RLS-bypassing) pool because it is a cross-org operator sweep; it writes only
/// derived embeddings. Resumable + idempotent.
async fn scan_divergence() -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let pool = brainiac_store::admin_pool(&url).await?;
    let provider = brainiac_gateway::QwenProvider::from_env().context(
        "scan-divergence adjudicates with a real provider — set QWEN_API_KEY (or DASHSCOPE_API_KEY)",
    )?;
    let stats = brainiac_pipeline::divergence::scan_all(&pool, &provider).await?;
    pool.close().await;
    tracing::info!(
        clusters = stats.clusters,
        divergences = stats.divergences,
        "practice-divergence scan finished"
    );
    Ok(())
}

async fn reembed(embedder_name: Option<&str>) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let pool = brainiac_store::admin_pool(&url).await?;
    let embedder = embedder_select(embedder_name)?;
    let batch = brainiac_pipeline::reembed::batch_from_env();
    let stats = brainiac_pipeline::reembed::reembed(&pool, embedder.as_ref(), batch).await?;
    pool.close().await;
    tracing::info!(
        version = stats.version_id,
        memories = stats.memories,
        canonicals = stats.canonicals,
        batches = stats.batches,
        "reembed backfill finished"
    );
    Ok(())
}

async fn worker(mock: bool) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = embedder_select(None)?;
    // Standalone: own the signal → shutdown wiring the server otherwise provides.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received; draining");
        let _ = shutdown_tx.send(true);
    });
    worker_loop(store, embedder, mock, shutdown_rx).await
}

/// Resolve when the process is asked to stop — SIGINT **or** SIGTERM.
///
/// `tokio::signal::ctrl_c()` is SIGINT only, but every container orchestrator
/// (Cloud Run, k8s, systemd `stop`) sends SIGTERM on deploy/scale-down and never
/// SIGINT. A ctrl_c-only future therefore left the whole graceful-shutdown path as
/// dead code in the primary deploy target: the process ran until the orchestrator's
/// grace timer elapsed and was then SIGKILLed, dropping in-flight requests (a
/// search, a token mint, a /v1/memories write mid-commit) on EVERY rollout — and
/// `serve --with-worker` never reached `worker_handle.await`, so the worker was
/// never told to finish its batch.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(e) => {
                // Registering the handler failed: degrade to SIGINT rather than
                // never shutting down at all.
                tracing::warn!(error = %e, "cannot listen for SIGTERM; ctrl_c only");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Adaptive idle polling bounds (Direction 1): when the queue is empty the loop
/// sleeps `IDLE_MIN` and doubles the wait per consecutive empty tick up to
/// `IDLE_MAX`, resetting to `IDLE_MIN` the moment a tick does work. This keeps
/// latency low when jobs are flowing (a freshly-enqueued source is picked up in
/// ~500ms) while an idle worker settles to one poll every few seconds instead
/// of hammering the DB twice a second forever.
const WORKER_IDLE_MIN: std::time::Duration = std::time::Duration::from_millis(500);
const WORKER_IDLE_MAX: std::time::Duration = std::time::Duration::from_secs(5);
/// Self-heal backoff bounds for a failing tick (DB hiccup, provider outage):
/// first retry waits BASE, doubling per consecutive failure up to CAP.
const WORKER_SELFHEAL_BASE: std::time::Duration = std::time::Duration::from_secs(1);
const WORKER_SELFHEAL_CAP: std::time::Duration = std::time::Duration::from_secs(30);

/// Sleep for `dur`, returning early with `true` the moment shutdown is
/// signalled. Used for every wait in the loop so ctrl_c is honoured promptly.
async fn sleep_or_shutdown(
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
    dur: std::time::Duration,
) -> bool {
    if *shutdown.borrow() {
        return true;
    }
    tokio::select! {
        _ = tokio::time::sleep(dur) => false,
        // The channel only ever flips false -> true, so a change means shutdown.
        _ = shutdown.changed() => true,
    }
}

/// The pipeline drain loop. Runs standalone (`brainiac worker`) or as a task
/// inside `serve --with-worker`.
///
/// Direction 1 hardening: a failing tick (a DB hiccup, a provider outage) no
/// longer propagates `?` and kills the worker — under `serve --with-worker`
/// that left the REST server up while the worker was silently dead. Instead the
/// loop logs, backs off exponentially, and retries, self-healing when the
/// dependency recovers. On ctrl_c it finishes the in-flight batch, then exits.
async fn worker_loop(
    store: Store,
    embedder: Arc<dyn Embedder>,
    mock: bool,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let default_provider: Arc<dyn brainiac_gateway::ChatProvider> = if mock {
        tracing::warn!("worker running with the MOCK provider — dev/demo only");
        Arc::new(brainiac_gateway::MockProvider::new(|_| {
            r#"{"memories":[]}"#.to_string()
        }))
    } else {
        Arc::new(
            brainiac_gateway::QwenProvider::from_env()
                .context("set DASHSCOPE_API_KEY (or pass --mock for dev)")?,
        )
    };
    // The org-intelligence sweeps (divergence adjudication) adjudicate with the
    // default provider; keep a handle before it's moved into the router.
    let sweep_provider = default_provider.clone();
    // Per-stage overrides (BRAINIAC_MODEL_EXTRACT / _RESOLVE / _CONTRADICT)
    // let extraction run a stronger model than adjudication.
    let providers = brainiac_gateway::ProviderRouter::from_env(default_provider)?;

    // Admin (RLS-bypassing) pool for the scheduled cross-org sweeps — they loop
    // every org, exactly like reembed, so they cannot run on the app-role pool.
    let sweep_admin = brainiac_store::admin_pool(&database_url()?).await?;
    // The KB tick is likewise cross-org: it must enumerate every org that has
    // pages, which no single tenant-scoped principal can do.
    let kb_admin = brainiac_store::admin_pool(&database_url()?).await?;
    // Stagger the first scheduler check so a just-booted worker drains any
    // backlog before it also fires sweeps; then every SCHED_INTERVAL.
    let mut last_sched_check = tokio::time::Instant::now();

    let version = {
        let principal = brainiac_pipeline::pipeline_principal(uuid::Uuid::nil());
        let mut tx = store.scoped_tx(&principal).await?;
        let v = brainiac_store::memories::ensure_embedding_version(
            &mut tx,
            embedder.model_name(),
            embedder.dim() as i32,
        )
        .await?;
        tx.commit().await?;
        v
    };

    let cfg = brainiac_pipeline::worker::WorkerConfig::from_env();
    tracing::info!(providers = %providers.describe(), embedder = embedder.model_name(), ?cfg, "brainiac worker started");
    let mut consecutive_failures: u32 = 0;
    let mut idle_backoff = WORKER_IDLE_MIN;
    loop {
        if *shutdown.borrow() {
            break;
        }
        // Let the in-flight batch run to completion before honouring shutdown —
        // we don't cancel a tick mid-source, we just stop starting new ones.
        match brainiac_pipeline::worker::tick(&store, &providers, embedder.as_ref(), version, &cfg)
            .await
        {
            Ok(stats) => {
                consecutive_failures = 0;
                if stats.jobs == 0 {
                    if sleep_or_shutdown(&mut shutdown, idle_backoff).await {
                        break;
                    }
                    // Back off geometrically while the queue stays empty.
                    idle_backoff = (idle_backoff * 2).min(WORKER_IDLE_MAX);
                } else {
                    // Work found — snap back to a tight poll for low latency.
                    idle_backoff = WORKER_IDLE_MIN;
                    tracing::info!(?stats, "pipeline tick");
                }
            }
            Err(e) => {
                consecutive_failures += 1;
                let shift = (consecutive_failures - 1).min(5);
                let backoff = (WORKER_SELFHEAL_BASE * (1 << shift)).min(WORKER_SELFHEAL_CAP);
                // Bound the log noise of a sustained outage: first few, then
                // every 10th — enough to prove liveness without flooding.
                if consecutive_failures <= 3 || consecutive_failures.is_multiple_of(10) {
                    tracing::error!(
                        error = %e,
                        consecutive_failures,
                        backoff_secs = backoff.as_secs(),
                        "worker tick failed; backing off and retrying (self-heal)"
                    );
                }
                if sleep_or_shutdown(&mut shutdown, backoff).await {
                    break;
                }
            }
        }

        // The knowledge base maintains itself (§8): recompose every page whose
        // memories moved, and scaffold entity pages for canonical entities that
        // have crossed the threshold. Runs AFTER ingest in the same loop, on
        // purpose — a page must never recompose from a half-ingested corpus, and
        // dirty pages are a durable work list, so falling behind for one tick
        // costs nothing but a little freshness.
        //
        // Both are no-ops for an org with no pages: a dirty-page lookup that
        // matches nothing, and a threshold query that returns nothing. An org
        // that has not turned the KB layer on pays a query per tick and nothing
        // else.
        match compose_sweep(&store, &providers, embedder.as_ref(), version, &kb_admin).await {
            Ok(stats) if stats.composed > 0 || stats.scaffolded > 0 => {
                tracing::info!(?stats, "knowledge base tick");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "knowledge-base tick failed"),
        }

        // Sweep scheduler: at most once per SCHED_INTERVAL, dispatch any due
        // org-intelligence sweeps. Claiming is one atomic UPDATE and each due
        // sweep runs on its own spawned task, so this never delays ingest.
        if last_sched_check.elapsed() >= brainiac_server::sweeps::SCHED_INTERVAL {
            last_sched_check = tokio::time::Instant::now();
            match brainiac_server::sweeps::run_due(&sweep_admin, sweep_provider.clone()).await {
                Ok(n) if n > 0 => tracing::info!(dispatched = n, "sweep scheduler ran due sweeps"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "sweep scheduler check failed"),
            }
        }
    }
    sweep_admin.close().await;
    kb_admin.close().await;
    tracing::info!("brainiac worker shut down gracefully");
    Ok(())
}

#[derive(Debug, Default)]
struct KbStats {
    composed: usize,
    auto_published: usize,
    needs_review: usize,
    scaffolded: usize,
    /// Pages pushed to an external target (git / Confluence).
    published: usize,
    /// Pages held back by the health circuit breaker — the number an operator
    /// should be alarmed by if it stays non-zero.
    publish_blocked: usize,
}

/// Where the console lives, so a published page can link its citations back to
/// the governed memory behind each claim. A wiki page whose sources point
/// nowhere is just another wiki page.
fn console_url() -> String {
    std::env::var("BRAINIAC_CONSOLE_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into())
}

/// One knowledge-base pass across every org that has any (ARCHITECTURE §8).
///
/// Scaffolding runs BEFORE composition so a page created this tick composes in
/// the same tick rather than sitting empty until the next one — an empty page is
/// the single worst thing a wiki can show a reader who came looking for an
/// answer.
///
/// Orgs are enumerated from the pages/entities that exist, not from a list of
/// tenants: an org with no KB does no work here beyond the two lookups.
async fn compose_sweep(
    store: &Store,
    providers: &brainiac_gateway::ProviderRouter,
    embedder: &dyn Embedder,
    version: i32,
    admin: &sqlx::PgPool,
) -> Result<KbStats> {
    let mut stats = KbStats::default();
    let console_url = console_url();

    // Every org with compose work: a page to recompose, a canonical entity to
    // scaffold an entity page from, or adopted standards to project into a
    // standards page (the last clause fixes F-9 — see the function's doc).
    let org_ids = brainiac_pipeline::compose::orgs_with_compose_work(admin).await?;

    for org_id in org_ids {
        let principal = brainiac_pipeline::pipeline_principal(org_id);

        // Scaffold under worker authority: deciding whether an entity has earned
        // a page requires seeing every team's memories.
        let mut tx = store.worker_tx(&principal).await?;
        match brainiac_pipeline::compose::scaffold_entity_pages(&mut tx, org_id, 5).await {
            Ok(created) => {
                stats.scaffolded += created.len();
                tx.commit().await?;
            }
            Err(e) => {
                tracing::warn!(org = %org_id, error = %e, "entity-page scaffolding failed");
                // Drop the tx; composition of existing pages still runs below.
            }
        }

        // The weekly digest: create it once the corpus has earned one, and
        // re-dirty it when its newest revision ages past the refresh cadence —
        // a time-windowed page goes stale by time passing, and no
        // memory-change trigger fires for an item aging out of the window.
        let mut tx = store.worker_tx(&principal).await?;
        let digest = async {
            let created = brainiac_pipeline::compose::scaffold_digest(&mut tx, org_id).await?;
            let refreshed = brainiac_pipeline::compose::refresh_digests(&mut tx, org_id).await?;
            anyhow::Ok((created, refreshed))
        }
        .await;
        match digest {
            Ok((created, _refreshed)) => {
                stats.scaffolded += usize::from(created.is_some());
                tx.commit().await?;
            }
            Err(e) => {
                tracing::warn!(org = %org_id, error = %e, "digest upkeep failed");
            }
        }

        // Standards pages (LIBRARY-PLAN L8): a stack whose adopted rules have
        // earned a page gets one. Deliberately after the digest and before the
        // compose tick, so a page scaffolded this pass renders in the same
        // pass — a projection needs no model call, so there is nothing to
        // spare it from.
        let mut tx = store.worker_tx(&principal).await?;
        match brainiac_pipeline::standards_page::scaffold_standards_pages(&mut tx, org_id, 5).await
        {
            Ok(created) => {
                stats.scaffolded += created.len();
                tx.commit().await?;
            }
            Err(e) => {
                tracing::warn!(org = %org_id, error = %e, "standards-page scaffolding failed");
            }
        }

        let c = brainiac_pipeline::worker::compose_tick(
            store, providers, embedder, version, org_id, 20,
        )
        .await?;
        stats.composed += c.composed;
        stats.auto_published += c.auto_published;
        stats.needs_review += c.needs_review;

        // Publish outward (KB3) — only if the org opted in, only org-visible
        // pages, and only while the health circuit breaker allows it. Every one
        // of those checks lives in brainiac-publish; a failure to publish must
        // never take down the ingest loop, so it is logged and swallowed here.
        match brainiac_publish::publish_org(store, org_id, &console_url).await {
            Ok(p) => {
                stats.published += p.pushed;
                stats.publish_blocked += p.blocked;
            }
            Err(e) => tracing::warn!(org = %org_id, error = %e, "knowledge-base publish failed"),
        }
    }
    Ok(stats)
}

/// The `docs` profile (EVAL.md §2.6): compose the gold pages with a REAL
/// provider and score the result. Like `extraction`, it measures what an actual
/// model does, so it REQUIRES a real provider rather than silently scoring a
/// mock — a mock composer cites perfectly by construction and would report a
/// safety that was never tested.
///
/// Three of its findings are absolute build failures, not scores: a leaked
/// forbidden memory, an altered pinned section, a page that failed to pick up a
/// superseding belief. Those are the promises the wiki is sold on.
#[allow(clippy::too_many_arguments)] // an eval entrypoint wired once from the CLI dispatch
async fn eval_docs(
    store: &Store,
    admin: &sqlx::PgPool,
    fx: &brainiac_fixtures::Fixtures,
    embedder: &dyn Embedder,
    out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
    samples: usize,
) -> Result<()> {
    use brainiac_eval::docs_profile::{
        hard_failures, regression_failures, regression_failures_mean, run, DocsBaseline,
    };

    let default: Arc<dyn brainiac_gateway::ChatProvider> =
        brainiac_gateway::QwenProvider::from_env()
            .map(|p| Arc::new(p) as Arc<dyn brainiac_gateway::ChatProvider>)
            .context(
                "the `docs` profile measures REAL composition quality and needs a real provider — \
             set QWEN_API_KEY (or DASHSCOPE_API_KEY). A mock composer cites perfectly by \
             construction, so scoring one would report a safety it never tested.",
            )?;
    let providers = brainiac_gateway::ProviderRouter::from_env(default)?;

    // Multi-sample: tenant reset between runs (seeding is INSERT-based, so a
    // second run on a dirty tenant would collide on ids rather than resample).
    // Hard gates apply to EVERY sample; only the soft rates are averaged.
    let mut runs = Vec::with_capacity(samples);
    for i in 0..samples {
        if i > 0 {
            reset_tenant(admin).await?;
        }
        let r = run(store, fx, embedder, &providers).await?;
        tracing::info!(
            sample = i + 1,
            of = samples,
            coverage = r.coverage,
            hallucination_rate = r.hallucination_rate,
            "docs sample"
        );
        let hard = hard_failures(&r);
        if !hard.is_empty() {
            eprintln!(
                "DOCS HARD GATES FAILED on sample {} of {}:\n{}",
                i + 1,
                samples,
                hard.join("\n")
            );
            std::process::exit(1);
        }
        runs.push(r);
    }

    if samples > 1 {
        let n = runs.len() as f64;
        let mean_cov = runs.iter().map(|r| r.coverage).sum::<f64>() / n;
        let mean_hall = runs.iter().map(|r| r.hallucination_rate).sum::<f64>() / n;
        let json = serde_json::to_string_pretty(&serde_json::json!({
            "samples": runs.len(),
            "mean_coverage": mean_cov,
            "mean_hallucination_rate": mean_hall,
            "runs": runs,
        }))?;
        match out {
            Some(path) => {
                if let Some(parent) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, &json)?;
                tracing::info!(path, "multi-sample docs report written");
            }
            None => println!("{json}"),
        }
        tracing::info!(
            samples = runs.len(),
            mean_coverage = mean_cov,
            mean_hallucination = mean_hall,
            "composition quality (mean of samples; hard gates held on every sample)"
        );
        if let Some(path) = write_baseline_path {
            let mut baseline = DocsBaseline::from_report(&runs[0]);
            baseline.coverage = mean_cov;
            baseline.hallucination_rate = mean_hall;
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
            tracing::info!(
                path,
                "docs baseline recalibrated from {} samples",
                runs.len()
            );
        }
        if let Some(path) = baseline_path {
            let baseline: DocsBaseline = serde_json::from_str(
                &std::fs::read_to_string(path)
                    .with_context(|| format!("reading baseline {path}"))?,
            )
            .context("parsing baseline")?;
            let regressions = regression_failures_mean(runs.len(), mean_cov, mean_hall, &baseline);
            if !regressions.is_empty() {
                eprintln!(
                    "DOCS GATES FAILED (mean, baseline {path}):\n{}",
                    regressions.join("\n")
                );
                std::process::exit(1);
            }
            tracing::info!(path, "docs regression gates passed on the mean");
        }
        return Ok(());
    }

    let report = runs.pop().expect("at least one sample");

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "docs report written");
        }
        None => println!("{json}"),
    }
    tracing::info!(
        provider = %report.provider,
        coverage = report.coverage,
        hallucination_rate = report.hallucination_rate,
        leaks = report.leaks.len(),
        pin_violations = report.pin_violations.len(),
        staleness_failures = report.staleness_failures.len(),
        auto_published_hallucinations = report.auto_published_hallucinations,
        "composition quality"
    );

    if let Some(path) = write_baseline_path {
        let baseline = DocsBaseline::from_report(&report);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(
            path,
            "docs baseline recalibrated — commit the diff with a reason"
        );
    }

    if let Some(path) = baseline_path {
        let baseline: DocsBaseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "DOCS GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "docs regression gates passed");
    }
    Ok(())
}

/// The `extraction` profile: score a REAL provider's extraction against gold with
/// semantic matching. Unlike `pipeline` (gold MockProvider), this measures actual
/// LLM extraction quality, so it REQUIRES a real provider — it errors clearly
/// without one rather than silently scoring a mock. Runs the extract stage on
/// Qwen (per-stage overrides honoured), tags the report with the extract model.
#[allow(clippy::too_many_arguments)] // an eval entrypoint wired once from the CLI dispatch
async fn eval_extraction(
    store: &Store,
    admin: &sqlx::PgPool,
    fx: &brainiac_fixtures::Fixtures,
    embedder: &dyn Embedder,
    out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
    samples: usize,
) -> Result<()> {
    use brainiac_eval::extraction_profile::{
        aggregate, regression_failures, regression_failures_multi, run, ExtractionBaseline,
    };

    let default: Arc<dyn brainiac_gateway::ChatProvider> = brainiac_gateway::QwenProvider::from_env()
        .map(|p| Arc::new(p) as Arc<dyn brainiac_gateway::ChatProvider>)
        .context(
            "the `extraction` profile measures REAL extraction quality and needs a real provider — \
             set QWEN_API_KEY (or DASHSCOPE_API_KEY). It is a nightly/on-demand per-provider run, \
             not a per-commit gate; use the `pipeline` profile for the deterministic plumbing check.",
        )?;
    let providers = brainiac_gateway::ProviderRouter::from_env(default)?;

    // Multi-sample: tenant reset between runs, because extraction is idempotent
    // per source — without the reset every sample after the first would dedupe
    // to zero new memories and report perfect, meaningless stability.
    let mut runs = Vec::with_capacity(samples);
    for i in 0..samples {
        if i > 0 {
            reset_tenant(admin).await?;
        }
        let r = run(store, admin, fx, embedder, &providers).await?;
        tracing::info!(
            sample = i + 1,
            of = samples,
            recall = r.recall,
            precision = r.precision,
            "extraction sample"
        );
        runs.push(r);
    }

    if samples > 1 {
        let agg = aggregate(runs);
        let json = serde_json::to_string_pretty(&agg)?;
        match out {
            Some(path) => {
                if let Some(parent) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, &json)?;
                tracing::info!(path, "multi-sample extraction report written");
            }
            None => println!("{json}"),
        }
        tracing::info!(
            samples = agg.samples,
            mean_recall = agg.mean_recall,
            mean_precision = agg.mean_precision,
            recall_spread = format!("{:.2}–{:.2}", agg.min_recall, agg.max_recall),
            gate_delta = agg.gate_delta,
            "extraction quality (mean of samples)"
        );
        if let Some(path) = write_baseline_path {
            let baseline = ExtractionBaseline::from_multi(&agg);
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
            tracing::info!(
                path,
                "extraction baseline recalibrated from {} samples",
                agg.samples
            );
        }
        if let Some(path) = baseline_path {
            let baseline: ExtractionBaseline = serde_json::from_str(
                &std::fs::read_to_string(path)
                    .with_context(|| format!("reading baseline {path}"))?,
            )
            .context("parsing baseline")?;
            let regressions = regression_failures_multi(&agg, &baseline);
            if !regressions.is_empty() {
                eprintln!(
                    "REGRESSION GATES FAILED (mean of {} samples, baseline {path}):\n{}",
                    agg.samples,
                    regressions.join("\n")
                );
                std::process::exit(1);
            }
            tracing::info!(path, "extraction regression gates passed on the mean");
        }
        return Ok(());
    }

    let report = runs.pop().expect("at least one sample");

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "extraction report written");
        }
        None => println!("{json}"),
    }
    tracing::info!(
        provider = %report.provider,
        recall = report.recall,
        precision = report.precision,
        micro_f1 = report.micro_f1,
        misses = report.misses.len(),
        "extraction quality"
    );

    if let Some(path) = write_baseline_path {
        let baseline = ExtractionBaseline::from_report(&report);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(
            path,
            "extraction baseline recalibrated — commit the diff with a reason"
        );
    }

    if let Some(path) = baseline_path {
        let baseline: ExtractionBaseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "REGRESSION GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "extraction regression gates passed");
    }
    Ok(())
}

/// Wipe the eval tenant back to empty — the reset between multi-sample runs.
/// The extraction pipeline is idempotent per source (redelivery dedup), so
/// WITHOUT this a second sample would extract nothing and report a perfect,
/// meaningless zero-variance score.
/// The `drift` profile (Level 2 MVP): score the docs-drift detector against
/// the synthetic stale-docs corpus. DB-free by design — the instrument under
/// test is claim-vs-corpus classification, and the corpus fits in memory. The
/// hard gate is the false alarm: a gold-aligned claim flagged as drift means
/// the detector attacks correct docs, which makes the feature unshippable.
async fn eval_drift(
    fx: &brainiac_fixtures::Fixtures,
    embedder: &dyn Embedder,
    out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
) -> Result<()> {
    use brainiac_eval::drift_profile::{hard_failures, regression_failures, run, DriftBaseline};

    let report = run(&fx.memories, &fx.drift, embedder).await?;

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "drift report written");
        }
        None => println!("{json}"),
    }
    tracing::info!(
        gold_drifted = report.gold_drifted,
        drift_recall = report.drift_recall,
        drift_precision = report.drift_precision,
        proposal_accuracy = report.proposal_accuracy,
        false_alarms = report.false_alarms.len(),
        "drift detection quality"
    );

    let hard = hard_failures(&report);
    if !hard.is_empty() {
        eprintln!("DRIFT HARD GATES FAILED:\n{}", hard.join("\n"));
        std::process::exit(1);
    }

    if let Some(path) = write_baseline_path {
        let baseline = DriftBaseline::from_report(&report);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(path, "drift baseline recalibrated");
    }
    if let Some(path) = baseline_path {
        let baseline: DriftBaseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "DRIFT GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "drift regression gates passed");
    }
    Ok(())
}

async fn reset_tenant(admin: &sqlx::PgPool) -> Result<()> {
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  document_reads, document_dependencies, document_publications, document_revisions,
                  document_sections, documents, publish_targets,
                  queue.jobs, queue.archive CASCADE",
    )
    .execute(admin)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn eval(
    fixtures_dir: &str,
    profile: &str,
    grid: bool,
    embedder_name: Option<&str>,
    reranker_name: Option<&str>,
    out: Option<&str>,
    diagnostics_out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
    samples: usize,
) -> Result<()> {
    anyhow::ensure!(
        matches!(
            profile,
            "retrieval"
                | "resolution"
                | "pipeline"
                | "contradiction"
                | "extraction"
                | "docs"
                | "drift"
        ),
        "v0 CLI supports profile=retrieval|resolution|pipeline|contradiction|extraction|docs|drift"
    );

    // The drift profile is deliberately DB-free — instrument calibration over
    // the fixture corpus itself — so it dispatches before any database setup
    // and runs anywhere the fixtures do.
    if profile == "drift" {
        let fx = brainiac_fixtures::load(fixtures_dir).context("loading fixtures")?;
        let embedder = embedder_select(embedder_name)?;
        return eval_drift(
            &fx,
            embedder.as_ref(),
            out,
            baseline_path,
            write_baseline_path,
        )
        .await;
    }

    let url = database_url()?;
    brainiac_store::migrate(&url).await?;

    // Fresh tenant slate (eval DBs are disposable by contract — see --help).
    // The queue tables are truncated too so the `pipeline` profile starts from
    // an empty ingest queue.
    let admin = sqlx::PgPool::connect(&url).await?;
    reset_tenant(&admin).await?;

    let store = Store::connect(&url).await?;
    let fx = brainiac_fixtures::load(fixtures_dir).context("loading fixtures")?;

    // Bake-off grid (§3.1): its own driver builds the backend axes and runs the
    // retrieval profile per config on a fresh tenant. Exploratory — no gates.
    if grid {
        return eval_grid(&store, &admin, &fx, out).await;
    }

    let embedder = embedder_select(embedder_name)?;
    tracing::info!(embedder = embedder.model_name(), profile, "running eval");

    if profile == "resolution" {
        return eval_resolution(
            &store,
            &fx,
            embedder.as_ref(),
            out,
            baseline_path,
            write_baseline_path,
        )
        .await;
    }

    if profile == "pipeline" {
        return eval_pipeline(
            &store,
            &admin,
            &fx,
            embedder.as_ref(),
            out,
            baseline_path,
            write_baseline_path,
        )
        .await;
    }

    if profile == "docs" {
        return eval_docs(
            &store,
            &admin,
            &fx,
            embedder.as_ref(),
            out,
            baseline_path,
            write_baseline_path,
            samples,
        )
        .await;
    }

    if profile == "extraction" {
        return eval_extraction(
            &store,
            &admin,
            &fx,
            embedder.as_ref(),
            out,
            baseline_path,
            write_baseline_path,
            samples,
        )
        .await;
    }

    if profile == "contradiction" {
        return eval_contradiction(
            &store,
            &fx,
            embedder.as_ref(),
            out,
            baseline_path,
            write_baseline_path,
        )
        .await;
    }

    // Stage-5 reranker axis (retrieval profile only): tagged into the report so
    // the regression gate can refuse a cross-reranker baseline comparison.
    let reranker = reranker_select(reranker_name)?;
    if let Some(r) = &reranker {
        tracing::info!(
            reranker = r.model_name(),
            "retrieval eval with stage-5 reranker"
        );
    }
    let seeded = brainiac_eval::seed::seed_gold(&store, &fx, embedder.as_ref()).await?;
    let (report, diagnostics) = brainiac_eval::retrieval_profile::run(
        &store,
        &fx,
        embedder.as_ref(),
        reranker.as_deref(),
        seeded.embedding_version,
    )
    .await?;

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "report written");
        }
        None => println!("{json}"),
    }
    if let Some(path) = diagnostics_out {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&diagnostics)?)?;
        let failing = diagnostics.queries.iter().filter(|q| !q.pass).count();
        tracing::info!(
            path,
            queries = diagnostics.queries.len(),
            failing,
            "per-query diagnostics written (failures first)"
        );
    }

    let failures = report.gate_failures();
    if !failures.is_empty() {
        eprintln!("HARD GATES FAILED:\n{}", failures.join("\n"));
        std::process::exit(1);
    }

    if let Some(path) = write_baseline_path {
        let baseline = brainiac_eval::gates::Baseline::from_report(&report)?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(
            path,
            "baseline recalibrated — commit the diff with a reason"
        );
    }

    if let Some(path) = baseline_path {
        let baseline: brainiac_eval::gates::Baseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = brainiac_eval::gates::regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "REGRESSION GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "regression gates passed");
    }
    Ok(())
}

/// The bake-off grid (EVAL.md §3.1): run the `retrieval` profile across the
/// cross-product of available backends and write ONE decision-table artifact —
/// `<stem>.json` (all reports keyed by config) and `<stem>.md` (the rendered
/// table). `--out` is treated as an extension-less stem (a trailing `.json`/`.md`
/// is stripped); the default is `results/grid/<date>-grid`. Exploratory: no gates.
async fn eval_grid(
    store: &Store,
    admin: &sqlx::PgPool,
    fx: &brainiac_fixtures::Fixtures,
    out: Option<&str>,
) -> Result<()> {
    let artifact = brainiac_eval::grid::run(store, admin, fx).await?;

    // Resolve the stem: strip a trailing .json/.md so `--out foo.json` and
    // `--out foo` both land the pair at `foo.{json,md}`.
    let stem = match out {
        Some(p) => {
            let path = std::path::Path::new(p);
            match path.extension().and_then(|e| e.to_str()) {
                Some("json") | Some("md") => path.with_extension("").to_string_lossy().into_owned(),
                _ => p.to_string(),
            }
        }
        None => brainiac_eval::grid::default_out_stem(),
    };
    if let Some(parent) = std::path::Path::new(&stem).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let json_path = format!("{stem}.json");
    let md_path = format!("{stem}.md");
    std::fs::write(&json_path, serde_json::to_string_pretty(&artifact)?)?;
    std::fs::write(&md_path, artifact.to_markdown())?;
    tracing::info!(
        json = json_path,
        md = md_path,
        cells = artifact.cells.len(),
        skipped = artifact.skipped.len(),
        "bake-off grid written (exploratory — no gates evaluated)"
    );
    for s in &artifact.skipped {
        tracing::info!(config = s.config, reason = s.reason, "grid config skipped");
    }
    Ok(())
}

/// The `resolution` profile (EVAL.md §2.2/§3.2): seed the gold RAW entities,
/// run the resolve stage over them with an oracle adjudicator, score the
/// predicted clustering, and enforce the hard `false_merges == 0` gate plus the
/// optional B³/pairwise F1 regression gate. Store/fx/embedder are already set
/// up and the tenant truncated by the caller.
async fn eval_resolution(
    store: &Store,
    fx: &brainiac_fixtures::Fixtures,
    embedder: &dyn Embedder,
    out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
) -> Result<()> {
    use brainiac_eval::resolution_profile::{regression_failures, run, ResolutionBaseline};

    brainiac_eval::seed::seed_resolution(store, fx).await?;
    let report = run(store, fx, embedder).await?;

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "resolution report written");
        }
        None => println!("{json}"),
    }

    // HARD GATE first: a false merge is zero-tolerance.
    let failures = report.gate_failures();
    if !failures.is_empty() {
        eprintln!("HARD GATES FAILED:\n{}", failures.join("\n"));
        std::process::exit(1);
    }

    if let Some(path) = write_baseline_path {
        let baseline = ResolutionBaseline::from_report(&report);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(
            path,
            "resolution baseline recalibrated — commit the diff with a reason"
        );
    }

    if let Some(path) = baseline_path {
        let baseline: ResolutionBaseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "REGRESSION GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "resolution regression gates passed");
    }
    Ok(())
}

/// The `pipeline` profile (EVAL.md §2.1/§3): drive the REAL worker chain over
/// the seed transcripts with a deterministic gold mock, score the extracted
/// memories against gold (content-level P/R/micro-F1), and enforce the soft
/// micro-F1 regression gate (cross-config comparison refused). `store`/`admin`/
/// `fx`/`embedder` are set up and the tenant + queue truncated by the caller.
async fn eval_pipeline(
    store: &Store,
    admin: &sqlx::PgPool,
    fx: &brainiac_fixtures::Fixtures,
    embedder: &dyn Embedder,
    out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
) -> Result<()> {
    use brainiac_eval::pipeline_profile::{regression_failures, run, PipelineBaseline};

    let report = run(store, admin, fx, embedder).await?;

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "pipeline report written");
        }
        None => println!("{json}"),
    }

    // No hard gate on this profile (see PipelineReport::gate_failures) — the
    // zero-tolerance false-merge invariant is owned by the resolution profile.
    let failures = report.gate_failures();
    if !failures.is_empty() {
        eprintln!("HARD GATES FAILED:\n{}", failures.join("\n"));
        std::process::exit(1);
    }

    if let Some(path) = write_baseline_path {
        let baseline = PipelineBaseline::from_report(&report);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(
            path,
            "pipeline baseline recalibrated — commit the diff with a reason"
        );
    }

    if let Some(path) = baseline_path {
        let baseline: PipelineBaseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "REGRESSION GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "pipeline regression gates passed");
    }
    Ok(())
}

/// The `contradiction` profile (EVAL.md §2.3/§3): seed the gold contradiction
/// pairs into isolated orgs, drive the REAL contradict stage with a gold-oracle
/// verdict mock, and score detection recall/precision, false-positive rate, and
/// supersede-direction accuracy. Soft regression gate only (cross-config
/// comparison refused). `store`/`fx`/`embedder` are set up and the tenant
/// truncated by the caller.
async fn eval_contradiction(
    store: &Store,
    fx: &brainiac_fixtures::Fixtures,
    embedder: &dyn Embedder,
    out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
) -> Result<()> {
    use brainiac_eval::contradiction_profile::{regression_failures, run, ContradictionBaseline};

    let report = run(store, fx, embedder).await?;

    let json = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => {
            if let Some(parent) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
            tracing::info!(path, "contradiction report written");
        }
        None => println!("{json}"),
    }

    // No hard gate (see ContradictionReport::gate_failures) — over-flagging is a
    // soft quality regression, not a zero-tolerance invariant.
    let failures = report.gate_failures();
    if !failures.is_empty() {
        eprintln!("HARD GATES FAILED:\n{}", failures.join("\n"));
        std::process::exit(1);
    }

    if let Some(path) = write_baseline_path {
        let baseline = ContradictionBaseline::from_report(&report);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&baseline)?)?;
        tracing::info!(
            path,
            "contradiction baseline recalibrated — commit the diff with a reason"
        );
    }

    if let Some(path) = baseline_path {
        let baseline: ContradictionBaseline = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading baseline {path}"))?,
        )
        .context("parsing baseline")?;
        let regressions = regression_failures(&report, &baseline);
        if !regressions.is_empty() {
            eprintln!(
                "REGRESSION GATES FAILED (baseline {path}):\n{}",
                regressions.join("\n")
            );
            std::process::exit(1);
        }
        tracing::info!(path, "contradiction regression gates passed");
    }
    Ok(())
}
