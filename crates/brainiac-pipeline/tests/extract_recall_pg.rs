//! Extraction recall-robustness, against a real Postgres (DATABASE_URL-gated).
//!
//! The unit tests in `extract.rs` prove the retry POLICY (an empty result on a
//! substantive chunk is re-sampled, a pass that exhausts its repairs hands the
//! chunk on, the per-chunk call budget is hard). These prove the two claims
//! that only the data plane can settle:
//!
//!   1. the recovered extraction actually LANDS as durable memories — the whole
//!      point of raising recall is rows in `memories`, not a parsed struct; and
//!   2. the existing `(source, content)` dedup ABSORBS every repeat the
//!      multi-pass/multi-chunk path can create, so no duplicate memory reaches
//!      the store.

use brainiac_core::embed::{DeterministicEmbedder, Embedder};
use brainiac_core::Principal;
use brainiac_gateway::{ChatRequest, MockProvider};
use brainiac_pipeline::extract;
use brainiac_store::{governance, memories, orgs, Store};
use sqlx::Row;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

const ORG: Uuid = Uuid::from_bytes([71u8; 16]);
const TEAM: Uuid = Uuid::from_bytes([72u8; 16]);

fn principal() -> Principal {
    Principal {
        org_id: ORG,
        user_id: Uuid::from_bytes([73u8; 16]),
        team_ids: vec![TEAM],
        project_id: None,
    }
}

async fn fresh_store(url: &str) -> (Store, sqlx::PgPool, i32) {
    brainiac_store::migrate(url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, canonical_entity_embeddings, entity_links,
                  edges, contradictions, promotions, memories, canonical_entities, entities,
                  provenance, sources, team_members, users, teams, orgs, pipeline_runs,
                  queue.jobs, queue.archive CASCADE",
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

async fn seed_source(store: &Store, source_id: Uuid, text: &str) {
    let mut tx = store.scoped_tx(&principal()).await.expect("tx");
    governance::insert_source(
        &mut tx,
        source_id,
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

/// A transcript-shaped body of `n` chars — long enough to earn a resample, and
/// (when asked) long enough to force the chunker to split it.
fn transcript(n: usize) -> String {
    let line = "engineer: we capped psp-gateway retries at three because the \
                double-charge incident traced back to a retry storm. ";
    line.repeat(n / line.len() + 1)
}

const MEM_A: &str = r#"{"memories":[{"kind":"pitfall","title":"psp-gateway retry storm",
    "content":"a per-request idempotency key lets a psp-gateway retry storm double-charge",
    "confidence":0.9,"entities":[{"name":"psp-gateway","kind":"service"}]}]}"#;

#[tokio::test]
async fn a_transcript_that_first_extracts_nothing_still_lands_its_memories() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — extraction recall test needs Postgres");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let (store, admin, version) = fresh_store(&url).await;
    let embedder = DeterministicEmbedder::default();
    let source_id = Uuid::from_bytes([81u8; 16]);
    let text = transcript(1_500);
    seed_source(&store, source_id, &text).await;

    // The failure the eval measured: the model returns a well-formed EMPTY
    // extraction for a transcript that plainly contains knowledge. Before the
    // resample this parsed cleanly, wrote zero rows, and scored 0.0 recall.
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let provider = MockProvider::new(move |_: &ChatRequest| {
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            r#"{"memories":[]}"#.to_string()
        } else {
            MEM_A.to_string()
        }
    });

    let mut tx = store.worker_tx(&principal()).await.expect("tx");
    let stats = extract::run_extract(
        &mut tx,
        &provider,
        &embedder,
        version,
        ORG,
        Some(TEAM),
        source_id,
        "session_transcript",
        &text,
        None,
        None,
    )
    .await
    .expect("extract");
    tx.commit().await.expect("commit");

    assert_eq!(
        stats.memories_written, 1,
        "the re-sampled extraction must reach the store, not just parse"
    );
    assert_eq!(stats.recovered_from_empty, 1);
    assert_eq!(stats.empty_resamples, 1);
    assert_eq!(stats.llm_calls, 2, "one wasted call + one that worked");

    // Read back over the admin (RLS-bypassing) pool, exactly as the extraction
    // eval profile does — the assertion is about what LANDED, and must not be
    // confounded by the reader's own scope.
    let rows = sqlx::query("SELECT content FROM memories WHERE org_id = $1")
        .bind(ORG)
        .fetch_all(&admin)
        .await
        .expect("read back");
    assert_eq!(rows.len(), 1);
    assert!(rows[0]
        .get::<String, _>("content")
        .contains("double-charge"));
}

#[tokio::test]
async fn dedupe_absorbs_every_repeat_the_multi_pass_path_can_create() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set — extraction dedupe test needs Postgres");
        return;
    };
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    let (store, admin, version) = fresh_store(&url).await;
    let embedder = DeterministicEmbedder::default();
    let source_id = Uuid::from_bytes([82u8; 16]);

    // A source long enough to split into several overlapping windows — the
    // structural duplicate generator (800 chars of every window is re-read by
    // the next one). Every window's model call returns the SAME memory, which
    // is the worst case for dedup: N chunks, one distinct fact.
    let text = transcript(30_000);
    seed_source(&store, source_id, &text).await;
    let provider = MockProvider::new(|_: &ChatRequest| MEM_A.to_string());

    let mut tx = store.worker_tx(&principal()).await.expect("tx");
    let stats = extract::run_extract(
        &mut tx,
        &provider,
        &embedder,
        version,
        ORG,
        Some(TEAM),
        source_id,
        "session_transcript",
        &text,
        None,
        None,
    )
    .await
    .expect("extract");
    tx.commit().await.expect("commit");

    assert!(
        stats.chunks > 1,
        "source must actually split: {}",
        stats.chunks
    );
    assert_eq!(stats.memories_written, 1, "one distinct fact → one row");
    assert_eq!(
        stats.deduped,
        stats.chunks - 1,
        "every overlap repeat is absorbed, not written"
    );

    // …and a REDELIVERED job (30s backoff / 300s visibility make this routine)
    // adds nothing either: same source, same content, all deduped.
    let mut tx = store.worker_tx(&principal()).await.expect("tx");
    let again = extract::run_extract(
        &mut tx,
        &provider,
        &embedder,
        version,
        ORG,
        Some(TEAM),
        source_id,
        "session_transcript",
        &text,
        None,
        None,
    )
    .await
    .expect("re-extract");
    tx.commit().await.expect("commit");
    assert_eq!(again.memories_written, 0);
    assert_eq!(again.deduped, again.chunks);

    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM memories WHERE org_id = $1")
        .bind(ORG)
        .fetch_one(&admin)
        .await
        .expect("count");
    assert_eq!(n, 1, "no duplicate memory ever reached the store");
}
