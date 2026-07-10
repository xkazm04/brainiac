//! MCP surface test (DATABASE_URL-gated): drive the JSON-RPC handler
//! directly — initialize handshake, tool listing, and every tool under a
//! real fixture principal with real RLS.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::Principal;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_server::mcp::{handle_message, McpState};
use brainiac_store::Store;
use serde_json::{json, Value};

fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn tool_payload(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text content");
    serde_json::from_str(text).expect("tool payload is JSON")
}

#[tokio::test]
async fn mcp_handshake_and_tools() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
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

    // Principal: the data analyst (team-data only) — the leak-sensitive case.
    let state = Arc::new(McpState {
        store,
        embedder: DeterministicEmbedder::default(),
        embedding_version: seeded.embedding_version,
        principal: Principal {
            org_id: stable_uuid(&fx.org.org),
            user_id: stable_uuid("user-data-analyst1"),
            team_ids: vec![stable_uuid("team-data")],
        },
    });

    // initialize
    let r = handle_message(&state, &rpc(1, "initialize", json!({})))
        .await
        .expect("response");
    assert_eq!(r["result"]["serverInfo"]["name"], "brainiac");
    assert!(r["result"]["capabilities"]["tools"].is_object());

    // notifications get no response
    assert!(handle_message(
        &state,
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .await
    .is_none());

    // tools/list
    let r = handle_message(&state, &rpc(2, "tools/list", json!({})))
        .await
        .expect("response");
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        vec![
            "memory_search",
            "memory_context",
            "memory_add",
            "entity_lookup"
        ]
    );

    // memory_search: RLS applies — the payments team-private memory must not
    // surface even on a targeted query.
    let r = handle_message(
        &state,
        &rpc(
            3,
            "tools/call",
            json!({
                "name": "memory_search",
                "arguments": { "query": "psp webhook signing secret rotation", "k": 25 }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    let forbidden = stable_uuid("mem-pay-0055").to_string();
    assert!(
        payload["memories"]
            .as_array()
            .expect("memories")
            .iter()
            .all(|m| m["id"].as_str() != Some(forbidden.as_str())),
        "team-private memory leaked through MCP"
    );

    // memory_context: canonical-only bundle with citations.
    let r = handle_message(
        &state,
        &rpc(4, "tools/call", json!({
            "name": "memory_context",
            "arguments": { "task_hint": "working on checkout funnel analytics and kafka ingestion" }
        })),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert!(payload["memories_included"].as_u64().expect("count") > 0);
    assert!(payload["context"]
        .as_str()
        .expect("ctx")
        .contains("memory:"));

    // entity_lookup: a team-specific surface form resolves to the canonical
    // and reveals the other teams' aliases (the collision-tolerance payoff).
    let r = handle_message(
        &state,
        &rpc(
            5,
            "tools/call",
            json!({
                "name": "entity_lookup",
                "arguments": { "name": "MSK cluster" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["found"], true);
    assert_eq!(payload["canonical"]["name"], "kafka");
    let known_as: Vec<&str> = payload["known_as"]
        .as_array()
        .expect("aliases")
        .iter()
        .map(|v| v.as_str().expect("alias"))
        .collect();
    assert!(known_as.contains(&"Kafka") && known_as.contains(&"the event bus"));

    // memory_add: accepted into the pipeline queue.
    let r = handle_message(
        &state,
        &rpc(
            6,
            "tools/call",
            json!({
                "name": "memory_add",
                "arguments": { "content": "event-lake nightly backfills start at 04:00 UTC" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["accepted"], true);

    // unknown method → error
    let r = handle_message(&state, &rpc(7, "resources/list", json!({})))
        .await
        .expect("response");
    assert!(r["error"]["message"]
        .as_str()
        .expect("err")
        .contains("method not found"));
}
