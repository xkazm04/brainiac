//! Extraction call cache (migration 0048), against a real Postgres.
//!
//! The claim under test is narrow and checkable: an identical redacted chunk
//! must skip the model call entirely, and the cache must NEVER outlive the
//! prompt or the model it was produced under. The counting provider makes both
//! auditable — the assertions are on model calls, not on wall-clock "speedups".

use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::Principal;
use brainiac_gateway::{ChatProvider, ChatRequest, ChatResponse, MockProvider};
use brainiac_pipeline::extract;
use brainiac_store::{governance, memories, orgs, Store};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

const ORG: Uuid = Uuid::from_bytes([91u8; 16]);
const TEAM: Uuid = Uuid::from_bytes([92u8; 16]);

fn principal() -> Principal {
    Principal {
        org_id: ORG,
        user_id: Uuid::from_bytes([93u8; 16]),
        team_ids: vec![TEAM],
        project_id: None,
    }
}

/// Wraps a provider so the cache key's model component can be changed without
/// changing anything else, and counts every call that actually reaches a model.
struct CountingProvider {
    inner: MockProvider,
    model: String,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl ChatProvider for CountingProvider {
    fn model_ref(&self) -> String {
        self.model.clone()
    }
    async fn complete(&self, req: &ChatRequest) -> anyhow::Result<ChatResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.complete(req).await
    }
}

const MEM: &str = r#"{"memories":[{"kind":"pitfall",
    "content":"a per-request idempotency key lets a psp-gateway retry storm double-charge",
    "confidence":0.9,"entities":[{"name":"psp-gateway","kind":"service"}]}]}"#;

fn provider(model: &str) -> (CountingProvider, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        CountingProvider {
            inner: MockProvider::new(|_: &ChatRequest| MEM.to_string()),
            model: model.to_string(),
            calls: calls.clone(),
        },
        calls,
    )
}

async fn fresh(url: &str) -> (Store, sqlx::PgPool, i32) {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  extraction_cache, queue.jobs, queue.archive CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(url).await.expect("connect");
    let embedder = DeterministicEmbedder::default();
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");
    orgs::upsert_org(&mut tx, ORG, "meridian")
        .await
        .expect("org");
    orgs::upsert_team(&mut tx, TEAM, ORG, "platform")
        .await
        .expect("team");
    let version =
        memories::ensure_embedding_version(&mut tx, embedder.model_name(), embedder.dim() as i32)
            .await
            .expect("version");
    tx.commit().await.expect("commit");
    (store, admin, version)
}

async fn seed_source(store: &Store, id: Uuid, text: &str) {
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");
    governance::insert_source(
        &mut tx,
        id,
        ORG,
        Some(TEAM),
        "session_transcript",
        text,
        None,
        None,
    )
    .await
    .expect("source");
    tx.commit().await.expect("commit");
}

fn transcript(tag: &str) -> String {
    format!(
        "engineer: {tag} we capped psp-gateway retries at three because the \
         double-charge incident traced back to a retry storm with a per-request \
         idempotency key. lead: agreed, the key moves onto the charge id. "
    )
    .repeat(4)
}

async fn run(
    store: &Store,
    p: &dyn ChatProvider,
    version: i32,
    source_id: Uuid,
    text: &str,
) -> extract::ExtractStats {
    let embedder = DeterministicEmbedder::default();
    let mut tx = store.worker_tx(&principal()).await.expect("tx");
    let stats = extract::run_extract(
        &mut tx,
        p,
        &embedder,
        version,
        ORG,
        Some(TEAM),
        source_id,
        "session_transcript",
        text,
        None,
        None,
    )
    .await
    .expect("extract");
    tx.commit().await.expect("commit");
    stats
}

#[tokio::test]
async fn a_redelivered_job_makes_zero_model_calls() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — extraction cache test needs Postgres");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let (store, _admin, version) = fresh(&url).await;
    let text = transcript("A");
    let source_id = Uuid::from_bytes([94u8; 16]);
    seed_source(&store, source_id, &text).await;

    let (p, calls) = provider("qwen:qwen-max");
    let first = run(&store, &p, version, source_id, &text).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "cold: one model call");
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.memories_written, 1);

    // Redelivery is routine, not exotic: 30s backoff, a 300s visibility window,
    // SKIP-LOCKED. It used to re-pay the full price to produce rows the
    // post-call dedup then threw away.
    let second = run(&store, &p, version, source_id, &text).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "redelivery must make ZERO further model calls"
    );
    assert_eq!(second.cache_hits, 1);
    assert_eq!(second.llm_calls, 0);
    // The post-call dedup still does its job on top — no duplicate row.
    assert_eq!(second.memories_written, 0);
    assert_eq!(second.deduped, 1);
}

#[tokio::test]
async fn the_cache_never_survives_a_model_change() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — extraction cache test needs Postgres");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let (store, _admin, version) = fresh(&url).await;
    let text = transcript("B");
    seed_source(&store, Uuid::from_bytes([95u8; 16]), &text).await;
    seed_source(&store, Uuid::from_bytes([96u8; 16]), &text).await;

    let (max, max_calls) = provider("qwen:qwen-max");
    run(&store, &max, version, Uuid::from_bytes([95u8; 16]), &text).await;
    assert_eq!(max_calls.load(Ordering::SeqCst), 1);

    // Same org, same chunk, DIFFERENT extract model (BRAINIAC_MODEL_EXTRACT is
    // a real per-stage override). Serving the old extraction here would be the
    // cache's worst failure: silent, stale, and invisible in the output.
    let (turbo, turbo_calls) = provider("qwen:qwen-plus");
    let stats = run(&store, &turbo, version, Uuid::from_bytes([96u8; 16]), &text).await;
    assert_eq!(
        turbo_calls.load(Ordering::SeqCst),
        1,
        "a model change must MISS the cache"
    );
    assert_eq!(stats.cache_hits, 0);
}

#[tokio::test]
async fn no_raw_secret_is_retained_in_a_cache_row() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — extraction cache test needs Postgres");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let (store, admin, version) = fresh(&url).await;

    // A transcript that leaks a credential, and a model that echoes it back in
    // its response — the exact way a secret could get durably parked in a new
    // table that nobody thought of as user content.
    const SECRET: &str = "AKIAIOSFODNN7EXAMPLE";
    let text = format!("{}\nops: the key is {SECRET}\n", transcript("C"));
    let source_id = Uuid::from_bytes([97u8; 16]);
    seed_source(&store, source_id, &text).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let echo = CountingProvider {
        inner: MockProvider::new(|_: &ChatRequest| {
            format!(
                r#"{{"memories":[{{"kind":"pitfall","content":"the deploy key {SECRET} was pasted into the channel"}}]}}"#
            )
        }),
        model: "qwen:qwen-max".into(),
        calls: calls.clone(),
    };
    run(&store, &echo, version, source_id, &text).await;

    let rows: Vec<(String,)> = sqlx::query_as("SELECT response FROM extraction_cache")
        .fetch_all(&admin)
        .await
        .expect("read cache");
    assert_eq!(rows.len(), 1, "the extraction was cached");
    assert!(
        !rows[0].0.contains(SECRET),
        "a raw credential survived into a cache row: {}",
        rows[0].0
    );
}
