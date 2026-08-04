//! Contested withholding happens BEFORE truncation (DATABASE_URL-gated).
//!
//! MCP withholds memories locked in an OPEN contradiction. It used to do so
//! *after* the retriever had already cut the ranking to `k`, so contested hits
//! occupied top-k slots, were then dropped from the response, and the next-best
//! UNCONTESTED memories were never considered — an agent that asked for 10
//! silently received 7, with answers 8–10 sitting unexamined one rank below the
//! cut.
//!
//! This suite contests the head of a real ranking and pins:
//!   * the response is full length (`k` really is `k`),
//!   * the tail is backfilled with exactly the NEXT-BEST uncontested memories,
//!     in the retriever's own order — not an arbitrary refill,
//!   * `contested_withheld` still reports honestly what was refused,
//!   * `include_contested: true` still serves both sides, unfiltered,
//!   * REST is unchanged — it REPORTS contradictions, it does not withhold.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::Principal;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_server::mcp::{handle_message, McpState};
use brainiac_store::retrieval::{RetrievalFilters, RetrievalRequest};
use brainiac_store::Store;
use serde_json::{json, Value};
use uuid::Uuid;

const QUERY: &str = "psp webhook signing secret rotation";
const K: usize = 10;
/// How many head-of-ranking memories to lock in open contradictions. Even, so
/// they can be paired with each other — pairing with an out-of-window memory
/// would contest that one too and muddy the expectation.
const CONTESTED: usize = 4;

fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn payload_ids(payload: &Value) -> Vec<String> {
    payload["memories"]
        .as_array()
        .expect("memories array")
        .iter()
        .map(|m| m["id"].as_str().expect("id").to_string())
        .collect()
}

async fn mcp_search(state: &McpState, args: Value) -> Value {
    let r = handle_message(
        state,
        &rpc(
            1,
            "tools/call",
            json!({ "name": "memory_search", "arguments": args }),
        ),
    )
    .await
    .expect("response");
    assert!(r["error"].is_null(), "memory_search failed: {r}");
    let text = r["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text content");
    serde_json::from_str(text).expect("tool payload is JSON")
}

#[tokio::test]
async fn contested_hits_are_withheld_before_the_top_k_cut() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE embedding_versions, memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let fx = brainiac_fixtures::load(brainiac_fixtures::loader::default_root()).expect("fixtures");
    let embedder = DeterministicEmbedder::default();
    let seeded = brainiac_eval::seed::seed_gold(&store, &fx, &embedder)
        .await
        .expect("seed");

    let principal = Principal {
        org_id: stable_uuid(&fx.org.org),
        user_id: stable_uuid("user-data-analyst1"),
        team_ids: vec![stable_uuid("team-data")],
        project_id: None,
    };

    // The UNGOVERNED ranking, deep enough to contain both the served window and
    // the backfill that must follow it. Contradiction status is not a scoring
    // input (`blended_score` has no contradiction term) and the candidate
    // queries never join `contradictions`, so this ranking is unaffected by the
    // rows inserted below — which is what makes it a valid expectation.
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    let deep = brainiac_store::retrieval::search_reranked(
        &mut tx,
        store.pool(),
        &embedder,
        None,
        seeded.embedding_version,
        &RetrievalRequest {
            query: QUERY.to_string(),
            k: K + CONTESTED + 5,
            as_of: None,
            filters: RetrievalFilters::default(),
        },
    )
    .await
    .expect("deep search");
    tx.commit().await.expect("commit");
    let deep_ids: Vec<String> = deep.iter().map(|h| h.memory.id.to_string()).collect();
    assert!(
        deep_ids.len() >= K + CONTESTED,
        "fixture too shallow for this test: {} hits",
        deep_ids.len()
    );

    // Contest the HEAD of the ranking: the first `CONTESTED` hits, paired with
    // each other so exactly that set (and nothing below it) becomes contested.
    // Both sides are visible to this principal, so the no-existence-oracle join
    // in `open_contradictions_for` is satisfied and the flags really surface.
    let org = stable_uuid(&fx.org.org);
    for pair in deep_ids[..CONTESTED].chunks(2) {
        sqlx::query(
            "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status, resolution_note)
             VALUES ($1, $2, $3, $4, 'test', 'open', 'unreconciled')",
        )
        .bind(Uuid::new_v4())
        .bind(org)
        .bind(Uuid::parse_str(&pair[0]).expect("uuid"))
        .bind(Uuid::parse_str(&pair[1]).expect("uuid"))
        .execute(&admin)
        .await
        .expect("contradiction");
    }

    let state = McpState {
        store: store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        reranker: None,
        embedding_version: seeded.embedding_version,
        principal: principal.clone(),
        scopes: None,
        project_id: None,
        session_remote: None,
        token_id: None,
    };

    // ── The agent asks for K and must GET K ────────────────────────────────
    let payload = mcp_search(&state, json!({ "query": QUERY, "k": K })).await;
    let served = payload_ids(&payload);
    assert_eq!(
        served.len(),
        K,
        "an agent that asked for {K} was served {} (contested hits still burn top-k slots)",
        served.len()
    );

    // …and the tail is the NEXT-BEST uncontested memories, in ranking order —
    // exactly the window the old post-truncation filter never looked at.
    let expected: Vec<String> = deep_ids[CONTESTED..CONTESTED + K].to_vec();
    assert_eq!(
        served, expected,
        "backfill is not the next-best uncontested set, in retrieval order"
    );

    // No contested memory leaked into the served set.
    for id in &deep_ids[..CONTESTED] {
        assert!(
            !served.contains(id),
            "contested memory {id} was served on the default path"
        );
    }

    // ── …and the withheld count stays honest ───────────────────────────────
    assert_eq!(
        payload["contested_withheld"].as_u64(),
        Some(CONTESTED as u64),
        "withheld count no longer reflects what was actually refused"
    );
    assert!(
        payload["note"].as_str().expect("note").contains("withheld"),
        "the governance-debt note must still be emitted"
    );

    // ── `include_contested: true` is unchanged: both sides, unfiltered ──────
    let all = mcp_search(
        &state,
        json!({ "query": QUERY, "k": K, "include_contested": true }),
    )
    .await;
    assert_eq!(
        payload_ids(&all),
        deep_ids[..K].to_vec(),
        "include_contested must serve the plain ranking"
    );
    assert_eq!(all["contested_withheld"].as_u64(), Some(0));
    let flagged = all["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .filter(|m| m["contradicted"] == json!(true))
        .count();
    assert_eq!(
        flagged, CONTESTED,
        "contested rows must still carry their do-not-adjudicate warning"
    );

    // ── REST is unchanged: it REPORTS contradictions, it does not withhold ──
    std::env::set_var(
        "BRAINIAC_TOKENS",
        json!({
            "tok_analyst": {
                "org": stable_uuid(&fx.org.org),
                "user": stable_uuid("user-data-analyst1"),
                "teams": [stable_uuid("team-data")],
            }
        })
        .to_string(),
    );
    let app = brainiac_server::http::router(
        store.clone(),
        Arc::new(DeterministicEmbedder::default()),
        None,
    )
    .await
    .expect("router");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let r = reqwest::Client::new()
        .post(format!("http://{addr}/v1/memories/search"))
        .bearer_auth("tok_analyst")
        .json(&json!({ "query": QUERY, "k": K }))
        .send()
        .await
        .expect("rest search");
    assert!(r.status().is_success(), "rest search: {}", r.status());
    let body: Value = r.json().await.expect("json");
    server.abort();
    let rest_ids: Vec<String> = body["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|h| h["id"].as_str().expect("id").to_string())
        .collect();
    assert_eq!(
        rest_ids,
        deep_ids[..K].to_vec(),
        "REST must keep serving contested hits (it reports, it does not filter)"
    );
    let with_flags = body["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .filter(|h| !h["contradictions"].as_array().is_none_or(|a| a.is_empty()))
        .count();
    assert_eq!(
        with_flags, CONTESTED,
        "REST must still surface the contradiction flags it reports"
    );
}
