//! Reembed backfill — the model-swap driver (ARCHITECTURE.md §3 stage 8:
//! "insert new embedding_versions row, backfill, flip active").
//!
//! Swapping embedders strands every existing memory unsearchable in the new
//! vector space until its content is re-embedded under the new version. This
//! driver closes that gap: it ensures the target version exists (which now
//! auto-creates its per-dimension HNSW index via migration 0012), then walks
//! every memory — and every canonical entity, which the resolve stage depends
//! on — that LACKS an embedding in that version and backfills it in batches
//! using the embedder's `embed_batch`.
//!
//! Operational contract:
//! - Cross-org. reembed is an OPERATOR action; it runs on the RLS-bypassing
//!   admin pool ([`brainiac_store::admin_pool`]) so it sweeps every tenant.
//!   It writes only DERIVED embeddings, so the bypass discloses nothing.
//! - Resumable + idempotent. The "missing embedding" query IS the resume
//!   point, and each batch autocommits (no wrapping transaction), so an
//!   interrupted run simply continues where it stopped on the next invocation,
//!   and a completed corpus is a no-op.

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_store::{entities, memories};
use sqlx::PgPool;

/// Default rows fetched-and-embedded per batch. Override with
/// `BRAINIAC_REEMBED_BATCH`. The remote embedder further chunks each batch to
/// its own per-request cap (Qwen: 10), so this is just the DB/​memory window.
pub const DEFAULT_BATCH: usize = 64;

#[derive(Debug, Clone, Copy, Default)]
pub struct ReembedStats {
    /// The target embedding version (ensured/created).
    pub version_id: i32,
    /// Memory embeddings written this run.
    pub memories: usize,
    /// Canonical-entity embeddings written this run.
    pub canonicals: usize,
    /// Batches issued (both phases).
    pub batches: usize,
}

/// Read `BRAINIAC_REEMBED_BATCH`, falling back to [`DEFAULT_BATCH`]; a zero or
/// unparseable value falls back too.
pub fn batch_from_env() -> usize {
    std::env::var("BRAINIAC_REEMBED_BATCH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_BATCH)
}

/// Backfill embeddings for `embedder`'s version across the whole corpus. `pool`
/// MUST be the admin (RLS-bypassing) pool for a cross-org operator sweep.
pub async fn reembed(pool: &PgPool, embedder: &dyn Embedder, batch: usize) -> Result<ReembedStats> {
    let batch = batch.max(1);
    let mut conn = pool.acquire().await.context("acquire admin connection")?;

    // Ensure the target version + its HNSW index (0012) before writing rows.
    let version_id =
        memories::ensure_embedding_version(&mut conn, embedder.model_name(), embedder.dim() as i32)
            .await
            .context("ensure target embedding version")?;
    let mut stats = ReembedStats {
        version_id,
        ..Default::default()
    };

    tracing::info!(
        version_id,
        model = embedder.model_name(),
        dim = embedder.dim(),
        batch,
        "reembed backfill starting"
    );

    // ── memories ──────────────────────────────────────────────────────────
    loop {
        let rows = memories::missing_embedding(&mut conn, version_id, batch as i64)
            .await
            .context("fetch memories missing embedding")?;
        if rows.is_empty() {
            break;
        }
        let texts: Vec<&str> = rows.iter().map(|(_, c)| c.as_str()).collect();
        let vecs = embedder
            .embed_batch(&texts)
            .await
            .context("embed_batch (memories)")?;
        anyhow::ensure!(
            vecs.len() == rows.len(),
            "embed_batch returned {} vectors for {} inputs",
            vecs.len(),
            rows.len()
        );
        for ((id, _), v) in rows.iter().zip(&vecs) {
            memories::upsert_embedding(&mut conn, *id, version_id, v).await?;
        }
        stats.memories += rows.len();
        stats.batches += 1;
        tracing::info!(
            done = stats.memories,
            this_batch = rows.len(),
            "reembed: memories batch committed"
        );
    }

    // ── canonical entities (the resolve path reads these) ─────────────────
    loop {
        let rows = entities::all_canonicals_missing_embedding(&mut conn, version_id, batch as i64)
            .await
            .context("fetch canonicals missing embedding")?;
        if rows.is_empty() {
            break;
        }
        let texts: Vec<&str> = rows.iter().map(|(_, name)| name.as_str()).collect();
        let vecs = embedder
            .embed_batch(&texts)
            .await
            .context("embed_batch (canonicals)")?;
        anyhow::ensure!(
            vecs.len() == rows.len(),
            "embed_batch returned {} vectors for {} inputs",
            vecs.len(),
            rows.len()
        );
        for ((id, _), v) in rows.iter().zip(&vecs) {
            entities::upsert_canonical_embedding(&mut conn, *id, version_id, v).await?;
        }
        stats.canonicals += rows.len();
        stats.batches += 1;
        tracing::info!(
            done = stats.canonicals,
            this_batch = rows.len(),
            "reembed: canonicals batch committed"
        );
    }

    tracing::info!(
        version_id,
        memories = stats.memories,
        canonicals = stats.canonicals,
        batches = stats.batches,
        "reembed backfill complete"
    );
    Ok(stats)
}
