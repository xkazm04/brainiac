//! brainiac — the single deployable binary (ARCHITECTURE.md §1):
//! `serve` (REST), `worker` (pipeline), `eval` (fixture harness).

use brainiac_server::http;

use std::sync::Arc;

use anyhow::{Context, Result};
use brainiac_core::embed::{DeterministicEmbedder, Embedder};
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
    },
    /// Run the MCP server on stdio (agent surface).
    Mcp,
    /// Run the pipeline worker loop.
    Worker {
        /// Use the deterministic mock provider (demo/dev only).
        #[arg(long)]
        mock: bool,
    },
    /// Run an eval profile against a fixture tree. DESTRUCTIVE to the
    /// connected database (re-seeds the tenant) — point it at a dev/eval DB.
    Eval {
        #[arg(long, default_value = "fixtures/v1")]
        fixtures: String,
        #[arg(long, default_value = "retrieval")]
        profile: String,
        /// Embedding backend: `deterministic` (default) or `qwen`
        /// (DashScope text-embedding-v4; needs QWEN_API_KEY/DASHSCOPE_API_KEY).
        #[arg(long)]
        embedder: Option<String>,
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
        } => serve(&bind, with_worker, mock).await,
        Command::Mcp => mcp().await,
        Command::Worker { mock } => worker(mock).await,
        Command::Eval {
            fixtures,
            profile,
            embedder,
            out,
            diagnostics,
            baseline,
            write_baseline,
        } => {
            eval(
                &fixtures,
                &profile,
                embedder.as_deref(),
                out.as_deref(),
                diagnostics.as_deref(),
                baseline.as_deref(),
                write_baseline.as_deref(),
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

async fn serve(bind: &str, with_worker: bool, mock: bool) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = embedder_select(None)?;

    // Single-process mode for constrained hosts (a 1 GB free-tier VM can't
    // afford a second runtime + connection pool). The worker loop is just a
    // task on the same runtime; it shares the store and the embedder.
    if with_worker {
        let store = store.clone();
        let embedder = Arc::clone(&embedder);
        tokio::spawn(async move {
            if let Err(e) = worker_loop(store, embedder, mock).await {
                tracing::error!(error = %e, "in-process worker stopped");
            }
        });
    }

    let app = http::router(store, embedder).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, with_worker, "brainiac REST listening");
    axum::serve(listener, app).await?;
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

async fn worker(mock: bool) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = embedder_select(None)?;
    worker_loop(store, embedder, mock).await
}

/// The pipeline drain loop. Runs standalone (`brainiac worker`) or as a task
/// inside `serve --with-worker`.
async fn worker_loop(store: Store, embedder: Arc<dyn Embedder>, mock: bool) -> Result<()> {
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
    // Per-stage overrides (BRAINIAC_MODEL_EXTRACT / _RESOLVE / _CONTRADICT)
    // let extraction run a stronger model than adjudication.
    let providers = brainiac_gateway::ProviderRouter::from_env(default_provider)?;

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

    tracing::info!(providers = %providers.describe(), embedder = embedder.model_name(), "brainiac worker started");
    loop {
        let stats =
            brainiac_pipeline::worker::tick(&store, &providers, embedder.as_ref(), version, 8)
                .await?;
        if stats.jobs == 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        } else {
            tracing::info!(?stats, "pipeline tick");
        }
    }
}

async fn eval(
    fixtures_dir: &str,
    profile: &str,
    embedder_name: Option<&str>,
    out: Option<&str>,
    diagnostics_out: Option<&str>,
    baseline_path: Option<&str>,
    write_baseline_path: Option<&str>,
) -> Result<()> {
    anyhow::ensure!(profile == "retrieval", "v0 CLI supports profile=retrieval");
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;

    // Fresh tenant slate (eval DBs are disposable by contract — see --help).
    let admin = sqlx::PgPool::connect(&url).await?;
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs CASCADE",
    )
    .execute(&admin)
    .await?;

    let store = Store::connect(&url).await?;
    let fx = brainiac_fixtures::load(fixtures_dir).context("loading fixtures")?;
    let embedder = embedder_select(embedder_name)?;
    tracing::info!(
        embedder = embedder.model_name(),
        "running retrieval profile"
    );
    let seeded = brainiac_eval::seed::seed_gold(&store, &fx, embedder.as_ref()).await?;
    let (report, diagnostics) = brainiac_eval::retrieval_profile::run(
        &store,
        &fx,
        embedder.as_ref(),
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
