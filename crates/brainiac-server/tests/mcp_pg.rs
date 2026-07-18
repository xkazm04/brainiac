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

    // Principal: the data analyst (team-data only) — the leak-sensitive case.
    let state = Arc::new(McpState {
        store,
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: seeded.embedding_version,
        principal: Principal {
            org_id: stable_uuid(&fx.org.org),
            user_id: stable_uuid("user-data-analyst1"),
            team_ids: vec![stable_uuid("team-data")],
            project_id: None,
        },
        scopes: None,
        project_id: None,
        session_remote: None,
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
            "memory_list",
            "memory_context",
            "memory_add",
            // F-1/F-2: close the async-ingest loop — poll a source for the
            // memory ids it produced, so an agent can confirm its contribution
            // landed and cite it.
            "memory_status",
            "entity_lookup",
            "knowledge_propose",
            "memory_feedback",
            "memory_provenance",
            // KB2 (§8.4): agents READ the knowledge base. There is deliberately
            // no doc_write/doc_edit — an agent contributes by proposing
            // memories, which pass the review gate and then flow into pages.
            "doc_search",
            "doc_get",
            // The library (LIBRARY-PLAN LB1/LB4): agents fetch the org's
            // ADOPTED judgment, pull published skill bundles, report what they
            // used, and propose patterns. Same asymmetry as the KB — there is
            // no standard_adopt: an agent proposes a CANDIDATE, and only a
            // named human ever turns one into a rule.
            "standards_for",
            "skill_search",
            "skill_fetch",
            "standard_propose",
            // F-4: the authoring counterpart to standard_propose for
            // procedures — an agent proposes a skill as a DRAFT; a named human
            // publishes it.
            "skill_propose",
            "skill_report_usage"
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
    let src = payload["source_id"]
        .as_str()
        .expect("source_id")
        .to_string();

    // memory_status (F-1/F-2): the loop-closer. Right after the add, extraction
    // has not run, so the source exists but has produced nothing yet.
    let r = handle_message(
        &state,
        &rpc(
            61,
            "tools/call",
            json!({ "name": "memory_status", "arguments": { "source_id": src } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["found"], true);
    assert_eq!(
        payload["extracted"], false,
        "nothing extracted yet: {payload}"
    );

    // Link a memory to that source the way extraction would, then poll again:
    // the memory id must surface — that is the handle an agent cites as evidence.
    let org = stable_uuid(&fx.org.org);
    let (prov_id, mem_id) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    sqlx::query("INSERT INTO provenance (id, org_id, actor_kind, actor_id, source_id) VALUES ($1,$2,'pipeline','worker:test',$3)")
        .bind(prov_id).bind(org).bind(src.parse::<uuid::Uuid>().expect("source_id is a uuid"))
        .execute(&admin).await.expect("provenance");
    sqlx::query(
        "INSERT INTO memories (id, org_id, visibility, status, kind, content, provenance_id)
         VALUES ($1,$2,'org','canonical','fact','event-lake backfills at 04:00 UTC',$3)",
    )
    .bind(mem_id)
    .bind(org)
    .bind(prov_id)
    .execute(&admin)
    .await
    .expect("memory");

    let r = handle_message(
        &state,
        &rpc(
            62,
            "tools/call",
            json!({ "name": "memory_status", "arguments": { "source_id": src } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(
        payload["extracted"], true,
        "the linked memory must surface: {payload}"
    );
    let ids: Vec<&str> = payload["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .map(|m| m["id"].as_str().expect("id"))
        .collect();
    assert!(
        ids.contains(&mem_id.to_string().as_str()),
        "the produced memory id is the citable handle: {payload}"
    );

    // A source that does not exist (or is invisible under RLS) is "not found",
    // never an error — existence is itself information.
    let r = handle_message(
        &state,
        &rpc(
            63,
            "tools/call",
            json!({ "name": "memory_status", "arguments": { "source_id": uuid::Uuid::new_v4() } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(tool_payload(&r)["found"], false);

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

    // UAT fix (open-contradiction serving): by DEFAULT, memory_search WITHHOLDS
    // both sides of an unresolved contradiction — their truth is undetermined, so
    // handing either over lets an agent pick a side on surface cues. The response
    // reports the count so the agent knows the area is contested.
    let r = handle_message(
        &state,
        &rpc(
            23,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zqxcontradiction widget cache", "k": 25 } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert!(
        find(&payload["memories"], &a_str).is_null()
            && find(&payload["memories"], &b_str).is_null(),
        "default search must WITHHOLD both contested sides: {payload}"
    );
    assert!(
        payload["contested_withheld"].as_u64().expect("count") >= 2,
        "the withheld count must surface the contested area: {payload}"
    );

    // include_contested:true surfaces them — flagged, non-actionable, each
    // pointing at the other and the same contradiction id.
    let r = handle_message(
        &state,
        &rpc(
            24,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zqxcontradiction widget cache", "k": 25, "include_contested": true } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    let ma = find(&payload["memories"], &a_str);
    let mb = find(&payload["memories"], &b_str);
    assert_eq!(ma["contradicted"], true, "A must be flagged: {payload}");
    assert_eq!(mb["contradicted"], true, "B must be flagged: {payload}");
    assert_eq!(ma["actionable"], false, "contested must be non-actionable");
    assert_eq!(ma["contradicts"][0]["counterpart_memory_id"], b_str);
    assert_eq!(mb["contradicts"][0]["counterpart_memory_id"], a_str);
    assert_eq!(
        ma["contradicts"][0]["contradiction_id"],
        contradiction_id.to_string()
    );

    // memory_context QUARANTINES the conflict into a CONTESTED section (not the
    // actionable bundle) so a text-only agent is not handed a side to act on.
    let r = handle_message(
        &state,
        &rpc(
            25,
            "tools/call",
            json!({ "name": "memory_context", "arguments": { "task_hint": "zqxcontradiction widget cache ttl" } }),
        ),
    )
    .await
    .expect("response");
    let ctxp = tool_payload(&r);
    let ctx = ctxp["context"].as_str().expect("ctx").to_string();
    assert!(ctx.contains("CONTESTED"), "context must quarantine: {ctx}");
    assert!(
        ctxp["contested_count"].as_u64().expect("cc") >= 2,
        "contested_count must be reported: {ctxp}"
    );

    // Resolve the contradiction (store-level) → the memories become actionable
    // again: default search now returns them, unflagged.
    sqlx::query("UPDATE contradictions SET status = 'resolved', resolved_at = now() WHERE id = $1")
        .bind(contradiction_id)
        .execute(&admin)
        .await
        .expect("resolve");
    let r = handle_message(
        &state,
        &rpc(
            26,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zqxcontradiction widget cache", "k": 25 } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    let ma = find(&payload["memories"], &a_str);
    let mb = find(&payload["memories"], &b_str);
    assert!(
        !ma.is_null() && !mb.is_null(),
        "resolved → served again: {payload}"
    );
    assert!(ma["contradicted"].is_null(), "A flag must clear: {payload}");
    assert!(mb["contradicted"].is_null(), "B flag must clear: {payload}");

    // ── memory_provenance: the evidence chain tool (Direction 3) ────────
    // Seed a full chain the analyst CAN read: source → provenance → memory.
    let sid = uuid::Uuid::new_v4();
    let pid = uuid::Uuid::new_v4();
    let mpid = uuid::Uuid::new_v4();
    // A credential pasted into the raw session, followed by padding to force
    // truncation. The excerpt must come back with the secret masked (H4).
    let long_source = format!(
        "my api_key: sk-abcdef0123456789ABCDEF here {}",
        "s".repeat(600)
    );
    sqlx::query(
        "INSERT INTO sources (id, org_id, team_id, kind, raw_text)
         VALUES ($1, $2, $3, 'manual', $4)",
    )
    .bind(sid)
    .bind(org)
    .bind(team)
    .bind(&long_source)
    .execute(&admin)
    .await
    .expect("source");
    sqlx::query(
        "INSERT INTO provenance (id, org_id, actor_kind, actor_id, model_ref, source_id)
         VALUES ($1, $2, 'agent', 'claude-code', 'claude-opus', $3)",
    )
    .bind(pid)
    .bind(org)
    .bind(sid)
    .execute(&admin)
    .await
    .expect("provenance");
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content, provenance_id)
         VALUES ($1, $2, $3, 'team', 'canonical', 'fact', 'provenance-chain probe memory', $4)",
    )
    .bind(mpid)
    .bind(org)
    .bind(team)
    .bind(pid)
    .execute(&admin)
    .await
    .expect("memory with provenance");

    let r = handle_message(
        &state,
        &rpc(
            26,
            "tools/call",
            json!({ "name": "memory_provenance", "arguments": { "memory_id": mpid.to_string() } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], false, "provenance failed: {r}");
    let payload = tool_payload(&r);
    assert_eq!(payload["actor_kind"], "agent");
    assert_eq!(payload["actor_ref"], "claude-code");
    assert_eq!(payload["model_ref"], "claude-opus");
    assert!(payload["created_at"].is_string(), "chain time: {payload}");
    assert_eq!(payload["source"]["kind"], "manual");
    let excerpt = payload["source"]["excerpt"].as_str().expect("excerpt");
    assert!(excerpt.ends_with('…'), "excerpt must be bounded: {excerpt}");
    assert!(
        excerpt.chars().count() <= 501,
        "excerpt over cap: {}",
        excerpt.chars().count()
    );
    // H4 redaction: the pasted credential must NOT survive into the excerpt.
    assert!(
        !excerpt.contains("sk-abcdef0123456789ABCDEF") && excerpt.contains("[REDACTED]"),
        "provenance excerpt must be redacted: {excerpt}"
    );
    assert!(payload["entity_anchors"].is_array());
    // H8 fix: the attribution tool answers "is it still true?" — the seeded
    // canonical memory has no valid_to, so still_valid is true and status is
    // canonical, and the validity/status keys are always present.
    assert_eq!(
        payload["still_valid"], true,
        "a memory with no valid_to must report still_valid: {payload}"
    );
    assert_eq!(
        payload["status"], "canonical",
        "status must travel: {payload}"
    );
    assert!(
        payload.get("valid_from").is_some() && payload.get("recorded_by").is_some(),
        "who/when keys must be present (even if null): {payload}"
    );

    // Leak case: provenance for an RLS-invisible memory reads as not-found —
    // the SAME answer as a nonexistent id (no existence oracle).
    let r = handle_message(
        &state,
        &rpc(
            27,
            "tools/call",
            json!({ "name": "memory_provenance", "arguments": { "memory_id": stable_uuid("mem-pay-0055").to_string() } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("not found"));

    // ── memory_context v2: SQL canonical floor, as_of, provenance refs
    // (Direction 1) ─────────────────────────────────────────────────────
    // One CANONICAL row amid a pile of RAW noise sharing a distinctive keyword.
    // Post-hoc canonical filtering over a k=25 top list would let the raw pile
    // crowd the servable row out; pushing the floor into the SQL candidate stage
    // guarantees the full budget is spent on canonical rows, so it survives.
    let noise_kw = "zzzcanonfloor";
    let canon_ctx = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, $3, 'team', 'canonical', 'fact', $4)",
    )
    .bind(canon_ctx)
    .bind(org)
    .bind(team)
    .bind(format!(
        "{noise_kw} the canonical widget flush interval is authoritative"
    ))
    .execute(&admin)
    .await
    .expect("canonical ctx memory");
    for i in 0..40 {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1, $2, $3, 'team', 'raw', 'fact', $4)",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(org)
        .bind(team)
        .bind(format!("{noise_kw} raw widget note number {i}"))
        .execute(&admin)
        .await
        .expect("raw noise memory");
    }
    let r = handle_message(
        &state,
        &rpc(
            28,
            "tools/call",
            json!({ "name": "memory_context", "arguments": { "task_hint": format!("{noise_kw} widget flush") } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert!(
        payload["memories_included"].as_u64().expect("count") >= 1,
        "canonical floor must survive raw noise: {payload}"
    );
    assert!(
        payload["context"]
            .as_str()
            .expect("ctx")
            .contains(&canon_ctx.to_string()),
        "the canonical row must be in the bundle despite the raw pile: {payload}"
    );

    // Each packed line carries a compact provenance ref (§4.6): the canonical
    // provenance-chain memory (mpid, seeded above with an agent/model chain)
    // must render its recorder inline.
    let r = handle_message(
        &state,
        &rpc(
            29,
            "tools/call",
            json!({ "name": "memory_context", "arguments": { "task_hint": "provenance-chain probe memory" } }),
        ),
    )
    .await
    .expect("response");
    let ctx = tool_payload(&r)["context"]
        .as_str()
        .expect("ctx")
        .to_string();
    assert!(
        ctx.contains("via agent"),
        "a packed line must carry a resolved provenance ref: {ctx}"
    );
    // H8 fix: every packed line stamps an effective date so a text-only agent
    // can judge recency without a second call.
    assert!(
        ctx.contains("[as of "),
        "a packed line must carry an effective date: {ctx}"
    );

    // as_of: a canonical memory valid ONLY in the past is absent at "now" and
    // present when the bundle is built as of a moment inside its window.
    let past_mem = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content, valid_from, valid_to)
         VALUES ($1, $2, $3, 'team', 'canonical', 'fact', $4,
                 now() - interval '10 days', now() - interval '2 days')",
    )
    .bind(past_mem)
    .bind(org)
    .bind(team)
    .bind("zzzpasttime historical widget cache setting")
    .execute(&admin)
    .await
    .expect("past-only memory");

    let r = handle_message(
        &state,
        &rpc(
            30,
            "tools/call",
            json!({ "name": "memory_context", "arguments": { "task_hint": "zzzpasttime historical widget" } }),
        ),
    )
    .await
    .expect("response");
    let ctx_now = tool_payload(&r)["context"]
        .as_str()
        .expect("ctx")
        .to_string();
    assert!(
        !ctx_now.contains(&past_mem.to_string()),
        "a past-only memory must be absent at now: {ctx_now}"
    );

    let as_of = (chrono::Utc::now() - chrono::Duration::days(5)).to_rfc3339();
    let r = handle_message(
        &state,
        &rpc(
            31,
            "tools/call",
            json!({ "name": "memory_context", "arguments": { "task_hint": "zzzpasttime historical widget", "as_of": as_of } }),
        ),
    )
    .await
    .expect("response");
    let ctx_past = tool_payload(&r)["context"]
        .as_str()
        .expect("ctx")
        .to_string();
    assert!(
        ctx_past.contains(&past_mem.to_string()),
        "the past-only memory must appear under as_of: {ctx_past}"
    );

    // ── Documented tool contract: scope / kinds / min_confidence / kind /
    // entities / feedback synonyms (Direction 2) ────────────────────────

    // kinds: two canonical memories sharing a keyword, different kinds; the
    // filter narrows to just the requested kind.
    let kf_fact = uuid::Uuid::new_v4();
    let kf_decision = uuid::Uuid::new_v4();
    for (id, kind) in [(kf_fact, "fact"), (kf_decision, "decision")] {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
             VALUES ($1, $2, $3, 'team', 'canonical', $4, $5)",
        )
        .bind(id)
        .bind(org)
        .bind(team)
        .bind(kind)
        .bind(format!("zzzkindfilter widget retention rule as a {kind}"))
        .execute(&admin)
        .await
        .expect("kind-filter memory");
    }
    let search_ids = |payload: &Value| -> Vec<String> {
        payload["memories"]
            .as_array()
            .expect("memories")
            .iter()
            .map(|m| m["id"].as_str().expect("id").to_string())
            .collect()
    };
    let r = handle_message(
        &state,
        &rpc(
            32,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzkindfilter widget retention", "k": 25, "kinds": ["decision"] } }),
        ),
    )
    .await
    .expect("response");
    let ids = search_ids(&tool_payload(&r));
    assert!(
        ids.contains(&kf_decision.to_string()) && !ids.contains(&kf_fact.to_string()),
        "kinds filter must keep the decision and drop the fact: {ids:?}"
    );

    // min_confidence: two canonical memories sharing a keyword with distinct
    // confidences; the floor drops the low one.
    let mc_high = uuid::Uuid::new_v4();
    let mc_low = uuid::Uuid::new_v4();
    for (id, conf) in [(mc_high, 0.9_f32), (mc_low, 0.2_f32)] {
        sqlx::query(
            "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content, confidence)
             VALUES ($1, $2, $3, 'team', 'canonical', 'fact', $4, $5)",
        )
        .bind(id)
        .bind(org)
        .bind(team)
        .bind(format!("zzzconffilter cache eviction note conf {conf}"))
        .bind(conf)
        .execute(&admin)
        .await
        .expect("confidence memory");
    }
    let r = handle_message(
        &state,
        &rpc(
            33,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzconffilter cache eviction", "k": 25, "min_confidence": 0.5 } }),
        ),
    )
    .await
    .expect("response");
    let ids = search_ids(&tool_payload(&r));
    assert!(
        ids.contains(&mc_high.to_string()) && !ids.contains(&mc_low.to_string()),
        "min_confidence must keep the 0.9 and drop the 0.2: {ids:?}"
    );

    // scope: a team-data memory and an org-visible memory owned by ANOTHER
    // team, both readable by the analyst. Default/org scope shows both; team
    // scope drops the one this team does not own.
    let other_team = uuid::Uuid::new_v4();
    let scope_mine = uuid::Uuid::new_v4();
    let scope_other = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, $3, 'team', 'canonical', 'fact', 'zzzscopefilter owned by my data team')",
    )
    .bind(scope_mine)
    .bind(org)
    .bind(team)
    .execute(&admin)
    .await
    .expect("scope mine");
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, $3, 'org', 'canonical', 'fact', 'zzzscopefilter org-wide from another team')",
    )
    .bind(scope_other)
    .bind(org)
    .bind(other_team)
    .execute(&admin)
    .await
    .expect("scope other");
    // org (default): both visible.
    let r = handle_message(
        &state,
        &rpc(
            34,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzscopefilter", "k": 25, "scope": "org" } }),
        ),
    )
    .await
    .expect("response");
    let ids = search_ids(&tool_payload(&r));
    assert!(
        ids.contains(&scope_mine.to_string()) && ids.contains(&scope_other.to_string()),
        "org scope must show both my team's and the org-wide memory: {ids:?}"
    );
    // team: only my team's memory survives.
    let r = handle_message(
        &state,
        &rpc(
            35,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzscopefilter", "k": 25, "scope": "team" } }),
        ),
    )
    .await
    .expect("response");
    let ids = search_ids(&tool_payload(&r));
    assert!(
        ids.contains(&scope_mine.to_string()) && !ids.contains(&scope_other.to_string()),
        "team scope must keep my team's and drop the other team's org-wide memory: {ids:?}"
    );
    // An unknown scope is a protocol error (-32602), not a silent no-op.
    let r = handle_message(
        &state,
        &rpc(
            36,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzscopefilter", "scope": "galaxy" } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["error"]["code"], -32602, "bad scope must be -32602: {r}");
    // An unknown kind likewise.
    let r = handle_message(
        &state,
        &rpc(
            37,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzscopefilter", "kinds": ["nonsense"] } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(r["error"]["code"], -32602, "bad kind must be -32602: {r}");
    // min_confidence out of range likewise.
    let r = handle_message(
        &state,
        &rpc(
            38,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": "zzzscopefilter", "min_confidence": 5 } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(
        r["error"]["code"], -32602,
        "out-of-range min_confidence must be -32602: {r}"
    );

    // memory_add: kind + entities fold into the stored source text the pipeline
    // reads, so they genuinely reach extraction.
    let r = handle_message(
        &state,
        &rpc(
            39,
            "tools/call",
            json!({
                "name": "memory_add",
                "arguments": {
                    "content": "the feast serving cache must be flushed before schema bumps",
                    "kind": "howto",
                    "entities": ["Feast", "Kafka"]
                }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["accepted"], true);
    assert_eq!(payload["kind"], "howto");
    let added_source: uuid::Uuid = payload["source_id"]
        .as_str()
        .expect("source_id")
        .parse()
        .expect("source_id is a uuid");
    let stored: String = sqlx::query_scalar("SELECT raw_text FROM sources WHERE id = $1")
        .bind(added_source)
        .fetch_one(&admin)
        .await
        .expect("stored source");
    assert!(
        stored.contains("howto") && stored.contains("Feast") && stored.contains("Kafka"),
        "kind/entities must be folded into the stored source text: {stored}"
    );
    // A bad kind on memory_add is a protocol error.
    let r = handle_message(
        &state,
        &rpc(
            40,
            "tools/call",
            json!({ "name": "memory_add", "arguments": { "content": "x", "kind": "nope" } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(
        r["error"]["code"], -32602,
        "bad add kind must be -32602: {r}"
    );

    // memory_feedback: the documented synonyms are accepted and stored
    // canonically (useful→helpful, stale→outdated); the doc vocabulary never
    // reaches the table.
    let syn_mem = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memories (id, org_id, team_id, visibility, status, kind, content)
         VALUES ($1, $2, $3, 'team', 'canonical', 'fact', 'zzzsynonym feedback probe memory')",
    )
    .bind(syn_mem)
    .bind(org)
    .bind(team)
    .execute(&admin)
    .await
    .expect("synonym memory");
    let r = handle_message(
        &state,
        &rpc(
            41,
            "tools/call",
            json!({ "name": "memory_feedback", "arguments": { "memory_id": syn_mem.to_string(), "verdict": "useful" } }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["recorded"], true);
    assert_eq!(
        payload["verdict"], "helpful",
        "useful must canonicalize to helpful: {payload}"
    );
    let r = handle_message(
        &state,
        &rpc(
            42,
            "tools/call",
            json!({ "name": "memory_feedback", "arguments": { "memory_id": syn_mem.to_string(), "verdict": "stale" } }),
        ),
    )
    .await
    .expect("response");
    assert_eq!(
        tool_payload(&r)["verdict"],
        "outdated",
        "stale must canonicalize to outdated"
    );
    // The stored rows use only the canonical vocabulary.
    let stored_verdicts: Vec<String> = sqlx::query_scalar(
        "SELECT verdict FROM memory_feedback WHERE memory_id = $1 ORDER BY verdict",
    )
    .bind(syn_mem)
    .fetch_all(&admin)
    .await
    .expect("stored verdicts");
    assert_eq!(
        stored_verdicts,
        vec!["helpful".to_string(), "outdated".to_string()],
        "only canonical verdicts may be stored: {stored_verdicts:?}"
    );

    // ── Governance floor on memory_search (UAT defect fix) ──────────────
    // The `zzzcanonfloor` pile seeded above is 1 canonical + 40 raw rows.
    // memory_search is the tool an agent reaches for mid-task; serving raw,
    // never-reviewed extractions there is exactly what the review queue exists
    // to prevent. Default search must return the canonical row and NONE of the
    // raw pile — the review step must actually guard the agent's main path.
    let r = handle_message(
        &state,
        &rpc(
            43,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": format!("{noise_kw} widget"), "k": 25 } }),
        ),
    )
    .await
    .expect("response");
    let default_hits = tool_payload(&r);
    let default_ids = search_ids(&default_hits);
    assert!(
        default_ids.contains(&canon_ctx.to_string()),
        "the canonical row must survive the default floor: {default_ids:?}"
    );
    assert!(
        default_hits["memories"]
            .as_array()
            .expect("memories")
            .iter()
            .all(|m| m["status"] != "raw"),
        "default memory_search must serve NO raw rows: {default_hits}"
    );

    // The floor is an opt-out, not a wall: include_unreviewed:true brings the
    // raw pile back (for a dev triaging their own captures), and every raw row
    // that comes back is explicitly tagged as ungoverned so the agent can weigh
    // it rather than trust it as org knowledge.
    let r = handle_message(
        &state,
        &rpc(
            44,
            "tools/call",
            json!({ "name": "memory_search", "arguments": { "query": format!("{noise_kw} widget"), "k": 25, "include_unreviewed": true } }),
        ),
    )
    .await
    .expect("response");
    let unrev = tool_payload(&r);
    let raw_rows: Vec<&Value> = unrev["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .filter(|m| m["status"] == "raw")
        .collect();
    assert!(
        !raw_rows.is_empty(),
        "include_unreviewed:true must surface the raw pile: {unrev}"
    );
    assert!(
        raw_rows
            .iter()
            .all(|m| m["governance"] == "candidate" && m["governance_warning"].is_string()),
        "every below-canonical row must carry a governance warning: {unrev}"
    );

    // skill_propose (F-4): an agent authors a skill over MCP. It lands as a
    // DRAFT — skill_fetch refuses it, the same way it refuses any unsigned
    // bundle — and re-proposing the name dedupes rather than duplicating.
    let r = handle_message(
        &state,
        &rpc(
            45,
            "tools/call",
            json!({ "name": "skill_propose", "arguments": {
                "name": "Backfill a price feed",
                "instructions_md": "# Backfill\n1. pause the live feed\n2. replay from the archive",
                "summary": "recover a gap in a provider's history",
                "domain": "providers"
            }}),
        ),
    )
    .await
    .expect("response");
    let proposed = tool_payload(&r);
    assert_eq!(proposed["outcome"], "created", "{proposed}");
    assert_eq!(proposed["slug"], "backfill-a-price-feed");

    let r = handle_message(
        &state,
        &rpc(
            46,
            "tools/call",
            json!({ "name": "skill_fetch", "arguments": { "slug": "backfill-a-price-feed" } }),
        ),
    )
    .await
    .expect("response");
    let fetched = tool_payload(&r);
    assert_eq!(fetched["found"], true);
    assert_eq!(
        fetched["published"], false,
        "a proposed draft is never served as a bundle: {fetched}"
    );

    let r = handle_message(
        &state,
        &rpc(
            47,
            "tools/call",
            json!({ "name": "skill_propose", "arguments": {
                "name": "backfill a price feed",
                "instructions_md": "# Different words, same name."
            }}),
        ),
    )
    .await
    .expect("response");
    let dup = tool_payload(&r);
    assert_eq!(dup["outcome"], "duplicate", "{dup}");
    assert_eq!(dup["maturity"], "draft");
}

// ── PROJECT-PLAN PR2: repo-fingerprint auto-attribution ────────────────────
//
// An org-wide key (`project_id: None`) that carries a `session_remote` (the
// checkout's git remote, from `BRAINIAC_REPO_REMOTE`) should have its
// `memory_add` writes auto-stamped with whatever project the org's repo
// whitelist resolves that remote to — without the agent ever passing a
// project explicitly. These tests set up a minimal org + project + whitelisted
// repo directly (no fixture loader needed — `memory_add` never touches
// retrieval/embeddings) and assert on the `sources.project_id` column the
// write actually landed in.

/// Fresh org/team/user for one attribution test, isolated by the caller's
/// TRUNCATE. Returns (org_id, user_id, team_id).
async fn seed_attr_org(store: &Store) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let org_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let team_id = uuid::Uuid::new_v4();
    let principal = Principal {
        org_id,
        user_id,
        team_ids: vec![team_id],
        project_id: None,
    };
    let mut tx = store.scoped_tx(&principal).await.expect("tx");
    {
        let c = &mut *tx;
        brainiac_store::orgs::upsert_org(c, org_id, "attr-org")
            .await
            .expect("org");
        brainiac_store::orgs::upsert_team(c, team_id, org_id, "attr-team")
            .await
            .expect("team");
        brainiac_store::orgs::upsert_user(c, user_id, org_id, "attr@x")
            .await
            .expect("user");
        brainiac_store::orgs::upsert_member(c, team_id, user_id, "member")
            .await
            .expect("member");
    }
    tx.commit().await.expect("commit");
    (org_id, user_id, team_id)
}

/// Connect + migrate + truncate, the same boilerplate every pg test in this
/// file starts with. Returns `None` (with a SKIP note) when DATABASE_URL is
/// unset.
async fn attr_setup() -> Option<(Store, sqlx::PgPool, tokio::sync::MutexGuard<'static, ()>)> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("SKIP: DATABASE_URL not set");
            return None;
        }
    };
    let guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive,
                  project_repos, projects
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");
    let store = Store::connect(&url).await.expect("connect");
    Some((store, admin, guard))
}

async fn source_project_id(admin: &sqlx::PgPool, source_id: uuid::Uuid) -> Option<uuid::Uuid> {
    use sqlx::Row;
    let row = sqlx::query("SELECT project_id FROM sources WHERE id = $1")
        .bind(source_id)
        .fetch_one(admin)
        .await
        .expect("source row");
    row.get("project_id")
}

/// resolve-hit: an org-wide key whose session_remote matches a whitelisted
/// repo gets its write auto-stamped with the resolved project.
///
/// Proof that this exercises the NEW path (not some other route to the same
/// project id): `state.project_id` is `None` here — the ONLY way
/// `effective_project_id` can come out `Some` is via the
/// `session_remote` → `normalize_remote` → `find_by_remote` branch added in
/// this change. On the prior code (which had no `session_remote` field and
/// always passed `state.project_id` straight through), this exact state would
/// have stamped `NULL` — i.e. this assertion is red on the old code and green
/// only with the new resolution logic wired in.
#[tokio::test]
async fn memory_add_auto_attributes_via_resolved_session_remote() {
    let Some((store, admin, _guard)) = attr_setup().await else {
        return;
    };
    let (org_id, user_id, team_id) = seed_attr_org(&store).await;

    let project_id = uuid::Uuid::new_v4();
    brainiac_store::projects::create(store.pool(), project_id, org_id, "attr-project")
        .await
        .expect("create project");
    let remote = "github.com/acme/attribution-target";
    brainiac_store::projects::add_repo(
        store.pool(),
        uuid::Uuid::new_v4(),
        org_id,
        project_id,
        remote,
        "",
    )
    .await
    .expect("add repo");

    let state = Arc::new(McpState {
        store,
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: Principal {
            org_id,
            user_id,
            team_ids: vec![team_id],
            project_id: None,
        },
        scopes: None,
        project_id: None,
        // Un-normalized form of the whitelisted remote — normalize_remote
        // must fold it to exactly `remote` for find_by_remote to hit.
        session_remote: Some(format!("https://{remote}.git")),
    });

    let r = handle_message(
        &state,
        &rpc(
            1,
            "tools/call",
            json!({
                "name": "memory_add",
                "arguments": { "content": "resolve-hit: an org-wide key with a resolvable checkout remote" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["accepted"], true, "{payload}");
    let source_id: uuid::Uuid = payload["source_id"]
        .as_str()
        .expect("source_id")
        .parse()
        .expect("uuid");

    let stamped = source_project_id(&admin, source_id).await;
    assert_eq!(
        stamped,
        Some(project_id),
        "org-wide key + resolvable session_remote must auto-stamp the resolved project"
    );
}

/// resolve-miss: session_remote is set but does not match any whitelisted
/// repo in this org → the write stays org-shared (NULL project_id) and the
/// call still succeeds — an unresolved remote is never an error.
#[tokio::test]
async fn memory_add_unresolvable_session_remote_stays_org_shared() {
    let Some((store, admin, _guard)) = attr_setup().await else {
        return;
    };
    let (org_id, user_id, team_id) = seed_attr_org(&store).await;

    // No project/repo whitelist entries at all in this org — every remote is
    // unresolvable by construction.
    let state = Arc::new(McpState {
        store,
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: Principal {
            org_id,
            user_id,
            team_ids: vec![team_id],
            project_id: None,
        },
        scopes: None,
        project_id: None,
        session_remote: Some("github.com/nobody/unknown".to_string()),
    });

    let r = handle_message(
        &state,
        &rpc(
            1,
            "tools/call",
            json!({
                "name": "memory_add",
                "arguments": { "content": "resolve-miss: an unwhitelisted checkout remote" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(
        payload["accepted"], true,
        "unresolved remote must not error: {payload}"
    );
    let source_id: uuid::Uuid = payload["source_id"]
        .as_str()
        .expect("source_id")
        .parse()
        .expect("uuid");

    let stamped = source_project_id(&admin, source_id).await;
    assert_eq!(
        stamped, None,
        "an unresolvable remote must fall back to org-shared (NULL project_id), not error"
    );

    // A garbage/unparseable remote (fails normalize_remote itself) must be
    // equally harmless — never a memory_add error.
    let r = handle_message(
        &state,
        &rpc(
            2,
            "tools/call",
            json!({
                "name": "memory_add",
                "arguments": { "content": "resolve-miss: a session_remote that is not even a valid remote" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(
        payload["accepted"], true,
        "a garbage BRAINIAC_REPO_REMOTE must never break memory_add: {payload}"
    );
}

/// explicit-scope-wins: a project-scoped key's own project always wins over
/// whatever its session_remote would otherwise resolve to.
#[tokio::test]
async fn memory_add_project_scoped_key_ignores_session_remote() {
    let Some((store, admin, _guard)) = attr_setup().await else {
        return;
    };
    let (org_id, user_id, team_id) = seed_attr_org(&store).await;

    let own_project = uuid::Uuid::new_v4();
    let remote_resolved_project = uuid::Uuid::new_v4();
    brainiac_store::projects::create(store.pool(), own_project, org_id, "own-project")
        .await
        .expect("create own project");
    brainiac_store::projects::create(
        store.pool(),
        remote_resolved_project,
        org_id,
        "remote-resolved-project",
    )
    .await
    .expect("create other project");
    let remote = "github.com/acme/other-repo";
    brainiac_store::projects::add_repo(
        store.pool(),
        uuid::Uuid::new_v4(),
        org_id,
        remote_resolved_project,
        remote,
        "",
    )
    .await
    .expect("add repo");

    let state = Arc::new(McpState {
        store,
        embedder: Arc::new(DeterministicEmbedder::default()),
        embedding_version: 1,
        principal: Principal {
            org_id,
            user_id,
            team_ids: vec![team_id],
            project_id: None,
        },
        scopes: None,
        // A project-scoped key — this must win.
        project_id: Some(own_project),
        // ...even though this remote resolves to a DIFFERENT project.
        session_remote: Some(remote.to_string()),
    });

    let r = handle_message(
        &state,
        &rpc(
            1,
            "tools/call",
            json!({
                "name": "memory_add",
                "arguments": { "content": "explicit-scope-wins: a project-scoped key with an unrelated session_remote" }
            }),
        ),
    )
    .await
    .expect("response");
    let payload = tool_payload(&r);
    assert_eq!(payload["accepted"], true, "{payload}");
    let source_id: uuid::Uuid = payload["source_id"]
        .as_str()
        .expect("source_id")
        .parse()
        .expect("uuid");

    let stamped = source_project_id(&admin, source_id).await;
    assert_eq!(
        stamped,
        Some(own_project),
        "the key's own project_id must win over whatever session_remote resolves to"
    );
}
