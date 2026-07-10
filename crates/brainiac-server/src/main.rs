//! brainiac — the single deployable binary (ARCHITECTURE.md §1):
//! `serve` (REST), `worker` (pipeline), `eval` (fixture harness).

use brainiac_server::http;

use anyhow::{Context, Result};
use brainiac_core::embed::{DeterministicEmbedder, Embedder};
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
        #[arg(long)]
        out: Option<String>,
    },
}

fn database_url() -> Result<String> {
    std::env::var("DATABASE_URL").context("DATABASE_URL must be set")
}

#[tokio::main]
async fn main() -> Result<()> {
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
            out,
        } => eval(&fixtures, &profile, out.as_deref()).await,
    }
}

async fn serve(bind: &str) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = DeterministicEmbedder::default();
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
    let embedder = DeterministicEmbedder::default();
    let state = brainiac_server::mcp::McpState::from_env(store, embedder).await?;
    brainiac_server::mcp::serve_stdio(std::sync::Arc::new(state)).await
}

async fn worker(mock: bool) -> Result<()> {
    let url = database_url()?;
    brainiac_store::migrate(&url).await?;
    let store = Store::connect(&url).await?;
    let embedder = DeterministicEmbedder::default();

    let provider: Box<dyn brainiac_gateway::ChatProvider> = if mock {
        tracing::warn!("worker running with the MOCK provider — dev/demo only");
        Box::new(brainiac_gateway::MockProvider::new(|_| {
            r#"{"memories":[]}"#.to_string()
        }))
    } else {
        Box::new(
            brainiac_gateway::QwenProvider::from_env()
                .context("set DASHSCOPE_API_KEY (or pass --mock for dev)")?,
        )
    };

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

    tracing::info!(provider = %provider.model_ref(), "brainiac worker started");
    loop {
        let stats =
            brainiac_pipeline::worker::tick(&store, provider.as_ref(), &embedder, version, 8)
                .await?;
        if stats.jobs == 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        } else {
            tracing::info!(?stats, "pipeline tick");
        }
    }
}

async fn eval(fixtures_dir: &str, profile: &str, out: Option<&str>) -> Result<()> {
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
    let embedder = DeterministicEmbedder::default();
    let seeded = brainiac_eval::seed::seed_gold(&store, &fx, &embedder).await?;
    let report =
        brainiac_eval::retrieval_profile::run(&store, &fx, &embedder, seeded.embedding_version)
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

    let failures = report.gate_failures();
    if !failures.is_empty() {
        eprintln!("HARD GATES FAILED:\n{}", failures.join("\n"));
        std::process::exit(1);
    }
    Ok(())
}
