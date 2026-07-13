//! MCP surface test (DATABASE_URL-gated): drive the JSON-RPC handler
//! directly — initialize handshake, tool listing, and every tool under a
//! real fixture principal with real RLS.

use std::sync::Arc;

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::Principal;
use brainiac_fixtures::ids::stable_uuid;
use brainiac_server::mcp::{handle_message, process_line, McpState};
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
        embedder: Arc::new(DeterministicEmbedder::default()),
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
            "entity_lookup",
            "knowledge_propose",
            "memory_feedback"
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

    // memory_feedback: closes the loop on a memory this principal CAN read.
    // Grab a visible id from a search first (data-team memories are seeded).
    let r = handle_message(
        &state,
        &rpc(
            7,
            "tools/call",
            json!({
                "name": "memory_search",
                "arguments": { "query": "feature store", "k": 5 }
            }),
        ),
    )
    .await
    .expect("response");
    let visible_id = tool_payload(&r)["memories"][0]["id"]
        .as_str()
        .expect("a visible memory to rate")
        .to_string();
    let r = handle_message(
        &state,
        &rpc(
            8,
            "tools/call",
            json!({
                "name": "memory_feedback",
                "arguments": { "memory_id": visible_id, "verdict": "outdated", "note": "superseded by the new ingestion path" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["recorded"], true);
    assert_eq!(payload["feedback_totals"][0]["verdict"], "outdated");
    assert_eq!(payload["feedback_totals"][0]["count"], 1);

    // The verdict comes back as a trust signal on the next search: the agent
    // that retrieves this memory is told it is disputed.
    let r = handle_message(
        &state,
        &rpc(
            12,
            "tools/call",
            json!({
                "name": "memory_search",
                "arguments": { "query": "feature store", "k": 5 }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    let rated = payload["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .find(|m| m["id"].as_str() == Some(visible_id.as_str()))
        .expect("the rated memory is still retrievable");
    assert_eq!(rated["feedback"]["outdated"], 1);
    assert_eq!(rated["feedback"]["disputed"], true);
    assert!(
        rated["warning"]
            .as_str()
            .expect("warning")
            .contains("re-verified"),
        "a disputed memory must warn the agent reading it"
    );

    // Invalid verdict is refused.
    let r = handle_message(
        &state,
        &rpc(
            9,
            "tools/call",
            json!({
                "name": "memory_feedback",
                "arguments": { "memory_id": visible_id, "verdict": "meh" }
            }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], true);

    // Feedback on an RLS-invisible memory reads as not-found (no oracle).
    let r = handle_message(
        &state,
        &rpc(
            10,
            "tools/call",
            json!({
                "name": "memory_feedback",
                "arguments": { "memory_id": stable_uuid("mem-pay-0055").to_string(), "verdict": "helpful" }
            }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("not found"));

    // knowledge_propose: a raw data-team memory the analyst CAN read gets a
    // needs_review promotion row; re-proposing is refused; an invisible
    // memory reads as not-found (no oracle).
    let raw_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, $3, 'team', 'raw', 'howto',
                 'raw: rotate the feast serving cache before schema bumps')",
    )
    .bind(raw_id)
    .bind(stable_uuid(&fx.org.org))
    .bind(stable_uuid("team-data"))
    .execute(&admin)
    .await
    .expect("raw memory");

    let r = handle_message(
        &state,
        &rpc(
            12,
            "tools/call",
            json!({
                "name": "knowledge_propose",
                "arguments": { "memory_id": raw_id.to_string() }
            }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], false, "propose failed: {r}");
    let payload = tool_payload(&r);
    assert_eq!(payload["proposed"], true);
    assert_eq!(payload["from_status"], "raw");
    assert_eq!(payload["to_status"], "candidate");
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM promotions
         WHERE memory_id = $1 AND policy_decision = 'needs_review' AND reviewed_at IS NULL",
    )
    .bind(raw_id)
    .fetch_one(&admin)
    .await
    .expect("count");
    assert_eq!(pending, 1, "one review-queue row per proposal");

    let r = handle_message(
        &state,
        &rpc(
            13,
            "tools/call",
            json!({
                "name": "knowledge_propose",
                "arguments": { "memory_id": raw_id.to_string() }
            }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("already awaiting review"));

    let r = handle_message(
        &state,
        &rpc(
            14,
            "tools/call",
            json!({
                "name": "knowledge_propose",
                "arguments": { "memory_id": stable_uuid("mem-pay-0055").to_string() }
            }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("not found"));

    // unknown method → error
    let r = handle_message(&state, &rpc(15, "resources/list", json!({})))
        .await
        .expect("response");
    assert_eq!(r["error"]["code"], -32601);
    assert!(r["error"]["message"]
        .as_str()
        .expect("err")
        .contains("method not found"));

    // ── JSON-RPC conformance & session hardening (Direction 1) ──────────
    // An unparseable line → -32700 with a null id (per spec), never dropped.
    let r = process_line(&state, "{ this is not json ]")
        .await
        .expect("a malformed frame must still be answered");
    assert_eq!(r["error"]["code"], -32700);
    assert!(r["id"].is_null());
    // A blank line yields no reply.
    assert!(process_line(&state, "   ").await.is_none());

    // A well-formed object carrying an id but NO method is an invalid request
    // (-32600) — it must get a reply, never silence (the agent would hang).
    let r = handle_message(&state, &json!({ "jsonrpc": "2.0", "id": 16 }))
        .await
        .expect("id-carrying request without method must be answered");
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], 16);

    // A non-request shape (a stray response object) that still carries an id
    // gets -32600, not silence.
    let r = handle_message(
        &state,
        &json!({ "jsonrpc": "2.0", "id": 17, "result": { "anything": true } }),
    )
    .await
    .expect("non-request with id must be answered");
    assert_eq!(r["error"]["code"], -32600);

    // A true notification (no id) still gets no reply, whatever its method.
    assert!(
        handle_message(&state, &json!({ "jsonrpc": "2.0", "method": "ping" }))
            .await
            .is_none()
    );

    // tools/call with no `name` → invalid params (-32602), a protocol error
    // (no `result`), not a tool result.
    let r = handle_message(&state, &rpc(18, "tools/call", json!({ "arguments": {} })))
        .await
        .expect("response");
    assert_eq!(r["error"]["code"], -32602);
    assert!(r.get("result").is_none());

    // An unknown tool name → -32602.
    let r = handle_message(
        &state,
        &rpc(19, "tools/call", json!({ "name": "does_not_exist" })),
    )
    .await
    .expect("response");
    assert_eq!(r["error"]["code"], -32602);

    // A required argument missing (query) → -32602, not a tool error.
    let r = handle_message(
        &state,
        &rpc(
            20,
            "tools/call",
            json!({ "name": "memory_search", "arguments": {} }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["error"]["code"], -32602);

    // Oversized input is a CLEAR TOOL ERROR (isError), not unbounded work and
    // not a protocol error.
    let huge = "x".repeat(3_000);
    let r = handle_message(
        &state,
        &rpc(
            21,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": huge } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], true, "oversized query: {r}");
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("too large"));

    // The session keeps serving after every failure frame above: a normal call
    // still succeeds.
    let r = handle_message(&state, &rpc(22, "ping", json!({})))
        .await
        .expect("response");
    assert!(r["result"].is_object(), "session must keep serving: {r}");

    // ── Contradiction-aware results (Direction 2) ──────────────────────
    // Two canonical data-team memories the analyst CAN read, in an OPEN
    // contradiction, sharing a distinctive keyword so FTS surfaces both.
    let org = stable_uuid(&fx.org.org);
    let team = stable_uuid("team-data");
    let mem_a = uuid::Uuid::new_v4();
    let mem_b = uuid::Uuid::new_v4();
    for (id, body) in [
        (mem_a, "zqxcontradiction the widget cache TTL is 30 seconds"),
        (
            mem_b,
            "zqxcontradiction the widget cache TTL is 300 seconds",
        ),
    ] {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1, $2, $3, 'team', 'canonical', 'fact', $4)",
        )
        .bind(id)
        .bind(org)
        .bind(team)
        .bind(body)
        .execute(&admin)
        .await
        .expect("contradiction memory");
    }
    let contradiction_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO contradictions (id, org_id, memory_a, memory_b, detected_by, status)
         VALUES ($1, $2, $3, $4, 'test', 'open')",
    )
    .bind(contradiction_id)
    .bind(org)
    .bind(mem_a)
    .bind(mem_b)
    .execute(&admin)
    .await
    .expect("contradiction");

    // memory_search flags BOTH hits, each pointing at the other and at the
    // same contradiction id.
    let contradiction_query = |id: u64| {
        rpc(
            id,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zqxcontradiction widget cache", "k": 25 } }),
        )
    };
    let r = handle_message(&state, &contradiction_query(23))
        .await
        .expect("response");
    let payload = tool_payload(&r);
    let find = |mems: &Value, id: &str| -> Value {
        mems.as_array()
            .expect("memories")
            .iter()
            .find(|m| m["id"].as_str() == Some(id))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let a_str = mem_a.to_string();
    let b_str = mem_b.to_string();
    let ma = find(&payload["memories"], &a_str);
    let mb = find(&payload["memories"], &b_str);
    assert_eq!(ma["contradicted"], true, "A must be flagged: {payload}");
    assert_eq!(mb["contradicted"], true, "B must be flagged: {payload}");
    assert_eq!(ma["contradicts"][0]["counterpart_memory_id"], b_str);
    assert_eq!(mb["contradicts"][0]["counterpart_memory_id"], a_str);
    assert_eq!(
        ma["contradicts"][0]["contradiction_id"],
        contradiction_id.to_string()
    );

    // memory_context renders the conflict textually so a text-only agent sees it.
    let r = handle_message(
        &state,
        &rpc(
            24,
            "tools/call",
            json!({ "name": "memory_context", "arguments": { "task_hint": "zqxcontradiction widget cache ttl" } }),
        ),
    )
    .await
    .expect("response");
    let ctx = tool_payload(&r)["context"]
        .as_str()
        .expect("ctx")
        .to_string();
    assert!(ctx.contains("CONTRADICTED"), "context must warn: {ctx}");

    // Resolve the contradiction (store-level) → the flags disappear.
    sqlx::query("UPDATE contradictions SET status = 'resolved', resolved_at = now() WHERE id = $1")
        .bind(contradiction_id)
        .execute(&admin)
        .await
        .expect("resolve");
    let r = handle_message(&state, &contradiction_query(25))
        .await
        .expect("response");
    let payload = tool_payload(&r);
    let ma = find(&payload["memories"], &a_str);
    let mb = find(&payload["memories"], &b_str);
    assert!(ma["contradicted"].is_null(), "A flag must clear: {payload}");
    assert!(mb["contradicted"].is_null(), "B flag must clear: {payload}");
}
