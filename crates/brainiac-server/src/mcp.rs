//! MCP server — the first-class agent surface (ARCHITECTURE.md §5.1).
//!
//! Transport: MCP stdio (newline-delimited JSON-RPC 2.0). The protocol layer
//! is deliberately hand-rolled and minimal — initialize / tools/list /
//! tools/call / ping — so there is no SDK churn in the trust boundary; the
//! whole loop is a pure `handle_message` function that tests drive without
//! a process.
//!
//! Identity: the MCP process authenticates as ONE principal — the developer's
//! personal token (`BRAINIAC_MCP_TOKEN`, resolved through the same
//! `BRAINIAC_TOKENS` map as REST). RLS therefore applies transparently to
//! every tool call: an agent can never retrieve what its operator can't.
//!
//! v0 tools: memory_search, memory_context, memory_add, entity_lookup.

use std::sync::Arc;

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::Principal;
use brainiac_store::Store;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Token budget for the memory_context bundle (chars ≈ tokens × 4).
const CONTEXT_CHAR_BUDGET: usize = 6000;

pub struct McpState {
    pub store: Store,
    pub embedder: Arc<dyn Embedder>,
    pub embedding_version: i32,
    pub principal: Principal,
}

impl McpState {
    pub async fn from_env(store: Store, embedder: Arc<dyn Embedder>) -> Result<Self> {
        let tokens = crate::auth::TokenMap::from_env()?;
        let token = std::env::var("BRAINIAC_MCP_TOKEN")
            .context("BRAINIAC_MCP_TOKEN must be set for the MCP surface")?;
        let principal = tokens
            .resolve(&token)
            .cloned()
            .context("BRAINIAC_MCP_TOKEN does not resolve to a principal")?;
        let embedding_version = {
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
        Ok(Self {
            store,
            embedder,
            embedding_version,
            principal,
        })
    }
}

/// Blocking stdio loop: one JSON-RPC message per line.
pub async fn serve_stdio(state: Arc<McpState>) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();
    tracing::info!("brainiac MCP server on stdio");
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "unparseable MCP frame");
                continue;
            }
        };
        if let Some(response) = handle_message(&state, &msg).await {
            let mut out = serde_json::to_vec(&response)?;
            out.push(b'\n');
            stdout.write_all(&out).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// Handle one JSON-RPC message. Returns None for notifications.
pub async fn handle_message(state: &McpState, msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;
    let id = msg.get("id").cloned();
    // Notifications (no id) get no response.
    let id = match id {
        Some(id) if !id.is_null() => id,
        _ => return None,
    };
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "brainiac", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(state, &params).await,
        other => Err(format!("method not found: {other}")),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": message }
        }),
    })
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "memory_search",
            "description": "Hybrid search over the organization's governed memory (vector + keyword + knowledge-graph expansion). Returns memories with provenance; results are permission-scoped to you.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What you want to know" },
                    "k": { "type": "integer", "description": "Max results (default 10, cap 25)" },
                    "as_of": { "type": "string", "description": "RFC3339 timestamp: answer as of this moment in time" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "memory_context",
            "description": "Session-start bundle: the most relevant CANONICAL organizational knowledge for a task, token-budgeted. Call once when starting work on something.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_hint": { "type": "string", "description": "Short description of the task/repo/area you are working on" }
                },
                "required": ["task_hint"]
            }
        },
        {
            "name": "memory_add",
            "description": "Record a piece of durable knowledge (fact, decision, pattern, pitfall, howto). It enters the extraction/review pipeline as raw knowledge — it is NOT immediately canonical.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "One self-contained natural-language statement" }
                },
                "required": ["content"]
            }
        },
        {
            "name": "entity_lookup",
            "description": "Resolve a name (service, repo, tech, feature, concept) to the org's canonical entity: its known aliases across teams and the strongest memories about it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "The name as you know it" }
                },
                "required": ["name"]
            }
        }
    ])
}

async fn call_tool(state: &McpState, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("tools/call requires a name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let payload = match name {
        "memory_search" => memory_search(state, &args).await,
        "memory_context" => memory_context(state, &args).await,
        "memory_add" => memory_add(state, &args).await,
        "entity_lookup" => entity_lookup(state, &args).await,
        other => return Err(format!("unknown tool: {other}")),
    };

    match payload {
        Ok(value) => Ok(json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "isError": false
        })),
        Err(e) => Ok(json!({
            "content": [{ "type": "text", "text": format!("error: {e}") }],
            "isError": true
        })),
    }
}

async fn memory_search(state: &McpState, args: &Value) -> Result<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .context("query is required")?;
    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(10).min(25) as usize;
    let as_of: Option<DateTime<Utc>> = args
        .get("as_of")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.embedder.as_ref(),
        state.embedding_version,
        &brainiac_store::retrieval::RetrievalRequest {
            query: query.to_string(),
            k,
            as_of,
            filters: Default::default(),
        },
    )
    .await?;
    Ok(json!({
        "memories": hits.iter().map(|h| json!({
            "id": h.memory.id,
            "kind": h.memory.kind.as_str(),
            "status": h.memory.status.as_str(),
            "content": h.memory.content,
            "via_graph": h.via_graph,
            "provenance_id": h.memory.provenance_id,
        })).collect::<Vec<_>>()
    }))
}

async fn memory_context(state: &McpState, args: &Value) -> Result<Value> {
    let hint = args
        .get("task_hint")
        .and_then(|v| v.as_str())
        .context("task_hint is required")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.embedder.as_ref(),
        state.embedding_version,
        &brainiac_store::retrieval::RetrievalRequest {
            query: hint.to_string(),
            k: 25,
            as_of: None,
            filters: Default::default(),
        },
    )
    .await?;

    // Canonical-only, whole entries packed into the char budget.
    let mut bundle = String::new();
    let mut included = 0usize;
    for h in hits
        .iter()
        .filter(|h| h.memory.status == brainiac_core::MemoryStatus::Canonical)
    {
        let line = format!(
            "- [{}] {} (memory:{})\n",
            h.memory.kind.as_str(),
            h.memory.content,
            h.memory.id
        );
        if bundle.len() + line.len() > CONTEXT_CHAR_BUDGET && included > 0 {
            break;
        }
        bundle.push_str(&line);
        included += 1;
    }
    Ok(json!({
        "context": format!(
            "Organizational knowledge relevant to your task ({included} canonical memories, cite ids when you rely on them):\n{bundle}"
        ),
        "memories_included": included
    }))
}

async fn memory_add(state: &McpState, args: &Value) -> Result<Value> {
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("content is required")?;
    let team_id = state.principal.team_ids.first().copied();
    let source_id = Uuid::new_v4();
    let mut tx = state.store.scoped_tx(&state.principal).await?;
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        state.principal.org_id,
        team_id,
        "manual",
        content,
        Some(state.principal.user_id),
    )
    .await?;
    tx.commit().await?;
    let job_id =
        brainiac_pipeline::worker::enqueue_source(&state.store, state.principal.org_id, source_id)
            .await?;
    Ok(json!({
        "accepted": true,
        "source_id": source_id,
        "job_id": job_id,
        "note": "queued for extraction; it becomes searchable after the pipeline runs and is subject to review before promotion"
    }))
}

async fn entity_lookup(state: &McpState, args: &Value) -> Result<Value> {
    use sqlx::Row;
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("name is required")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;

    // Resolve: canonical by name, or via a raw entity's link.
    let row = sqlx::query(
        "SELECT c.id, c.name, c.kind, c.summary
         FROM canonical_entities c
         WHERE lower(c.name) = lower($1)
         UNION
         SELECT c.id, c.name, c.kind, c.summary
         FROM entities e
         JOIN entity_links l ON l.entity_id = e.id
         JOIN canonical_entities c ON c.id = l.canonical_id
         WHERE lower(e.name) = lower($1)
         LIMIT 1",
    )
    .bind(name)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(json!({ "found": false, "name": name }));
    };
    let canonical_id: Uuid = row.get("id");

    // Aliases: every linked raw surface form.
    let aliases = sqlx::query(
        "SELECT e.name FROM entities e
         JOIN entity_links l ON l.entity_id = e.id
         WHERE l.canonical_id = $1",
    )
    .bind(canonical_id)
    .fetch_all(&mut *tx)
    .await?;
    let alias_names: Vec<String> = aliases.iter().map(|r| r.get::<String, _>("name")).collect();

    // Strongest visible memories anchored to any linked raw entity.
    let sibling_ids = sqlx::query("SELECT entity_id FROM entity_links WHERE canonical_id = $1")
        .bind(canonical_id)
        .fetch_all(&mut *tx)
        .await?;
    let ids: Vec<Uuid> = sibling_ids.iter().map(|r| r.get("entity_id")).collect();
    let memories = brainiac_store::memories::for_entities(&mut tx, &ids, 8).await?;

    Ok(json!({
        "found": true,
        "canonical": {
            "id": canonical_id,
            "name": row.get::<String, _>("name"),
            "kind": row.get::<String, _>("kind"),
            "summary": row.get::<Option<String>, _>("summary"),
        },
        "known_as": alias_names,
        "memories": memories.iter().map(|m| json!({
            "id": m.id, "kind": m.kind.as_str(), "content": m.content
        })).collect::<Vec<_>>()
    }))
}
