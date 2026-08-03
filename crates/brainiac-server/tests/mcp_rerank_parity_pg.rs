//! MCP ⇄ REST ranking parity (DATABASE_URL-gated).
//!
//! MCP is where coding agents actually retrieve. Before this suite existed the
//! agent surface called the reranker-less `retrieval::search` wrapper, so an
//! operator-selected stage-5 reranker was structurally unreachable from the
//! product's primary consumer while the console got it. These tests pin the
//! three properties that must hold now:
//!
//! 1. **Default is unchanged.** With `reranker: None` (the shipped default)
//!    `memory_search` returns exactly what the untouched reranker-less
//!    `retrieval::search` returns — same ids, same order, same scores.
//! 2. **The reranker reaches BOTH agent tools.** With a reranker configured,
//!    `memory_search` and `memory_context` reorder; the ordering is exactly
//!    `retrieval::search_reranked`'s, i.e. one shared entry point, no second
//!    ranking path that can drift.
//! 3. **REST and MCP agree.** The same query, the same reranker, the same k
//!    ⇒ the same ranking on the HTTP surface and the agent surface.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::rerank::LexicalOverlapReranker;
use brainiac_core::Principal;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_server::mcp::{handle_message, McpState};
use brainiac_store::retrieval::{RetrievalFilters, RetrievalRequest};
use brainiac_store::Store;
use serde_json::{json, Value};

/// The query both surfaces are asked. Multi-token and lexically uneven, so the
/// overlap reranker has something to reorder on.
const QUERY: &str = "psp webhook signing secret rotation";
const K: usize = 10;

fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn tool_payload(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text content");
    serde_json::from_str(text).expect("tool payload is JSON")
}

/// Ids of an MCP `memory_search` payload, in served order.
fn payload_ids(payload: &Value) -> Vec<String> {
    payload["memories"]
        .as_array()
        .expect("memories array")
        .iter()
        .map(|m| m["id"].as_str().expect("id").to_string())
        .collect()
}

/// The `(memory:<uuid>)` citation handles a `memory_context` bundle carries,
/// in packed order.
fn context_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("(memory:") {
        rest = &rest[at + "(memory:".len()..];
        if let Some(end) = rest.find(')') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        } else {
            break;
        }
    }
    out
}

/// Is `sub` a subsequence of `full`? The context bundle quarantines contested
/// rows into a trailing section and cuts on a char budget, so its packed ids
/// are a SUBSEQUENCE of the retriever's ranking, not a prefix of it — but the
/// relative order must still be the retriever's.
fn is_subsequence(sub: &[String], full: &[String]) -> bool {
    let mut it = full.iter();
    sub.iter().all(|s| it.any(|f| f == s))
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
    tool_payload(&r)
}

async fn mcp_context(state: &McpState, args: Value) -> String {
    let r = handle_message(
        state,
        &rpc(
            2,
            "tools/call",
            json!({ "name": "memory_context", "arguments": args }),
        ),
    )
    .await
    .expect("response");
    assert!(r["error"].is_null(), "memory_context failed: {r}");
    tool_payload(&r)["context"]
        .as_str()
        .expect("context text")
        .to_string()
}

#[tokio::test]
async fn mcp_honors_the_selected_reranker_and_matches_rest() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
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

    // ── Reference rankings, straight from the store, under the SAME principal.
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    let req = || RetrievalRequest {
        query: QUERY.to_string(),
        k: K,
        as_of: None,
        filters: RetrievalFilters::default(),
    };
    // `retrieval::search` is untouched by this change — it is still the
    // `reranker: None` delegate every pre-change caller used. Comparing MCP to
    // it is the "byte-identical to today" proof.
    let plain = brainiac_store::retrieval::search(
        &mut tx,
        store.pool(),
        &embedder,
        seeded.embedding_version,
        &req(),
    )
    .await
    .expect("plain search");
    let lexed = brainiac_store::retrieval::search_reranked(
        &mut tx,
        store.pool(),
        &embedder,
        Some(&LexicalOverlapReranker),
        seeded.embedding_version,
        &req(),
    )
    .await
    .expect("reranked search");
    tx.commit().await.expect("commit");

    let plain_ids: Vec<String> = plain.iter().map(|h| h.memory.id.to_string()).collect();
    let lexed_ids: Vec<String> = lexed.iter().map(|h| h.memory.id.to_string()).collect();
    assert!(plain_ids.len() >= 3, "need a few hits to compare orderings");
    // Guard on the fixture itself: if the reranker does not move this query,
    // every "reranker reached MCP" assertion below would pass vacuously.
    assert_ne!(
        plain_ids, lexed_ids,
        "fixture/query no longer exercises the reranker — pick another query"
    );

    let state_none = McpState {
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
    let state_lex = McpState {
        store: store.clone(),
        embedder: Arc::new(DeterministicEmbedder::default()),
        reranker: Some(Arc::new(LexicalOverlapReranker)),
        embedding_version: seeded.embedding_version,
        principal: principal.clone(),
        scopes: None,
        project_id: None,
        session_remote: None,
        token_id: None,
    };

    // ── 1. Default (`--reranker none`) is byte-identical to today ──────────
    let args = json!({ "query": QUERY, "k": K, "include_contested": true });
    let none_payload = mcp_search(&state_none, args.clone()).await;
    assert_eq!(
        payload_ids(&none_payload),
        plain_ids,
        "default MCP ranking drifted from the reranker-less path"
    );
    // …and not only the ids: the per-hit payload the agent reads must match the
    // reranker-less hits field for field. (`memory_search` does not expose the
    // blended score, so the graph provenance flag and the content are what
    // carry through.)
    for (i, h) in none_payload["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .enumerate()
    {
        assert_eq!(h["content"], json!(plain[i].memory.content), "content @{i}");
        assert_eq!(h["via_graph"], json!(plain[i].via_graph), "via_graph @{i}");
        assert_eq!(
            h["status"],
            json!(plain[i].memory.status.as_str()),
            "status @{i}"
        );
    }

    // ── 2. The selected reranker reaches `memory_search` ───────────────────
    let lex_payload = mcp_search(&state_lex, args.clone()).await;
    assert_eq!(
        payload_ids(&lex_payload),
        lexed_ids,
        "MCP did not rank through the configured reranker"
    );
    assert_ne!(
        payload_ids(&lex_payload),
        plain_ids,
        "MCP ignored the configured reranker (this is the pre-fix behavior)"
    );

    // ── 3. …and `memory_context` ──────────────────────────────────────────
    let ctx_args = json!({ "task_hint": QUERY });
    let ctx_none = mcp_context(&state_none, ctx_args.clone()).await;
    let ctx_lex = mcp_context(&state_lex, ctx_args).await;
    assert_ne!(
        context_ids(&ctx_none),
        context_ids(&ctx_lex),
        "memory_context ignored the configured reranker (pre-fix behavior)"
    );
    // The bundle's packed order must be the retriever's order — a canonical
    // reference ranking, since the tool applies a Canonical floor and k=25.
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    let canon_lex = brainiac_store::retrieval::search_reranked(
        &mut tx,
        store.pool(),
        &embedder,
        Some(&LexicalOverlapReranker),
        seeded.embedding_version,
        &RetrievalRequest {
            query: QUERY.to_string(),
            k: 25,
            as_of: None,
            filters: RetrievalFilters {
                min_status: Some(brainiac_core::MemoryStatus::Canonical),
                ..Default::default()
            },
        },
    )
    .await
    .expect("canonical reranked search");
    tx.commit().await.expect("commit");
    let canon_ids: Vec<String> = canon_lex.iter().map(|h| h.memory.id.to_string()).collect();
    let packed = context_ids(&ctx_lex);
    assert!(!packed.is_empty(), "context bundle packed nothing");
    assert!(
        is_subsequence(&packed, &canon_ids),
        "memory_context order is not the reranked retrieval order:\n packed={packed:?}\n ranked={canon_ids:?}"
    );

    // ── 4. REST and MCP return the same ranking for the same config ────────
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
        Some(Arc::new(LexicalOverlapReranker)),
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
    let rest_ids: Vec<String> = body["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|h| h["id"].as_str().expect("id").to_string())
        .collect();
    server.abort();

    assert_eq!(
        rest_ids,
        payload_ids(&lex_payload),
        "REST and MCP disagree on the ranking for the same query and reranker"
    );
}
