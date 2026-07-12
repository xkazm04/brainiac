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
        Command::Serve { bind } => serve(&bind).await,
        Command::Mcp => mcp().await,
        Command::Worker { mock } => worker(mock).await,
        Command::Eval {
            fixtures,
            profile,
            embedder,
            out,
            diagnostics,
        } => {
            eval(
                &fixtures,
                &profile,
                embedder.as_deref(),
                out.as_deref(),
                diagnostics.as_deref(),
            )
            .await
        }
    }
}

async fn serve(bind: &str) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = embedder_select(None)?;
    let app = http::router(store, embedder).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "brainiac REST listening");
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
    Ok(())
}
