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
//! v0 tools: memory_search, memory_context, memory_add, entity_lookup,
//! knowledge_propose, memory_feedback.

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

// ── Input size caps (§5.1 hardening) ────────────────────────────────────
// The MCP surface is a trust boundary reached by autonomous agents; every
// free-text field is bounded so a runaway caller can never hand us an
// unbounded blob to embed, store, or scan. Oversized input is rejected as a
// clear tool error before any work is done — never silently truncated.
/// `memory_add` content — one self-contained statement, generously sized.
const MAX_CONTENT_CHARS: usize = 8_000;
/// `memory_search` query and `memory_context` task_hint.
const MAX_QUERY_CHARS: usize = 2_000;
/// `entity_lookup` name — a surface form, not prose.
const MAX_NAME_CHARS: usize = 200;
/// `memory_feedback` note — a short human explanation.
const MAX_NOTE_CHARS: usize = 2_000;

/// A JSON-RPC-level error (maps to an `error` object with a spec code). Distinct
/// from a *tool* error, which is a successful response carrying `isError: true`.
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Outcome of running a tool. The three arms map to distinct wire shapes so the
/// caller can tell a protocol mistake from a rejected input from an internal
/// fault — and so raw internal/DB error strings NEVER reach the agent.
enum ToolError {
    /// Malformed/missing arguments → JSON-RPC `-32602 Invalid params`.
    InvalidParams(String),
    /// The tool ran and deliberately refused (business rule, RLS not-found,
    /// oversized input) → a tool error (`isError: true`) with this message,
    /// which is safe to show the agent.
    Rejected(String),
    /// An internal fault (DB, embedder, queue). The detail is logged; the agent
    /// gets a generic tool error so no internal string ever leaks.
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ToolError {
    fn from(e: anyhow::Error) -> Self {
        ToolError::Internal(e)
    }
}

impl From<sqlx::Error> for ToolError {
    fn from(e: sqlx::Error) -> Self {
        ToolError::Internal(e.into())
    }
}

fn invalid(msg: impl Into<String>) -> ToolError {
    ToolError::InvalidParams(msg.into())
}

fn rejected(msg: impl Into<String>) -> ToolError {
    ToolError::Rejected(msg.into())
}

/// A required string argument, present and non-empty after trimming.
fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    match args.get(key).and_then(|v| v.as_str()).map(str::trim) {
        Some(s) if !s.is_empty() => Ok(s),
        Some(_) => Err(invalid(format!("`{key}` must not be empty"))),
        None => Err(invalid(format!("`{key}` is required and must be a string"))),
    }
}

/// Enforce a documented character cap on a free-text field. Oversized input is
/// a clear tool error (rejected), never silent truncation or unbounded work.
fn within_cap<'a>(value: &'a str, cap: usize, field: &str) -> Result<&'a str, ToolError> {
    if value.chars().count() > cap {
        return Err(rejected(format!(
            "`{field}` is too large ({} chars); the limit is {cap}",
            value.chars().count()
        )));
    }
    Ok(value)
}

/// A required argument parsed as a UUID (bad format is a param error, not an
/// internal fault).
fn required_uuid(args: &Value, key: &str) -> Result<Uuid, ToolError> {
    let raw = required_str(args, key)?;
    raw.parse()
        .map_err(|_| invalid(format!("`{key}` must be a UUID")))
}

/// A JSON-RPC error frame. `id` is `Null` for pre-dispatch failures (parse /
/// invalid request) per the spec.
fn error_frame(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

/// A successful response whose payload is a tool error (`isError: true`).
fn tool_error(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

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

/// Blocking stdio loop: one JSON-RPC message per line. Individual frame faults
/// (unparseable line, write hiccup) never take the session down — the loop logs
/// and continues; only a broken/closed stdout ends it, cleanly.
pub async fn serve_stdio(state: Arc<McpState>) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();
    tracing::info!("brainiac MCP server on stdio");
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break, // clean EOF on stdin
            Err(e) => {
                // A read fault means the peer is gone; end the session cleanly.
                tracing::warn!(error = %e, "stdin read error; ending MCP session");
                break;
            }
        };
        let Some(response) = process_line(&state, &line).await else {
            continue; // blank line or notification — no reply
        };
        if !write_frame(&mut stdout, &response).await {
            break; // stdout is broken/closed — exit cleanly
        }
    }
    Ok(())
}

/// Process one raw input line into an optional reply frame. Blank lines and
/// notifications yield `None`; a malformed frame yields a spec `-32700` reply
/// (id null) instead of being silently dropped, so an id-carrying caller never
/// hangs. Exposed so tests can drive the raw-line failure paths.
pub async fn process_line(state: &McpState, line: &str) -> Option<Value> {
    if line.trim().is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(msg) => handle_message(state, &msg).await,
        Err(e) => {
            tracing::warn!(error = %e, "unparseable MCP frame");
            Some(error_frame(Value::Null, -32700, "parse error"))
        }
    }
}

/// Serialize and write one frame. Returns `false` if the write path is broken
/// (session should end); a mere serialization fault is logged and swallowed
/// (returns `true`) so one bad response can't kill the loop.
async fn write_frame(stdout: &mut tokio::io::Stdout, response: &Value) -> bool {
    use tokio::io::AsyncWriteExt;
    let mut out = match serde_json::to_vec(response) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize MCP response; dropping frame");
            return true; // keep serving
        }
    };
    out.push(b'\n');
    if let Err(e) = stdout.write_all(&out).await {
        tracing::warn!(error = %e, "stdout write failed; ending MCP session");
        return false;
    }
    if let Err(e) = stdout.flush().await {
        tracing::warn!(error = %e, "stdout flush failed; ending MCP session");
        return false;
    }
    true
}

/// Handle one JSON-RPC message. Returns `None` only for notifications (a frame
/// with no `id`); every id-carrying frame gets a reply.
pub async fn handle_message(state: &McpState, msg: &Value) -> Option<Value> {
    // A notification carries no id and never gets a reply, whatever its shape.
    let id = match msg.get("id") {
        Some(id) if !id.is_null() => id.clone(),
        _ => return None,
    };
    // An id-carrying frame that is not a well-formed request (no string method —
    // e.g. a stray response object) must still get a reply, never silence.
    let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
        return Some(error_frame(
            id,
            -32600,
            "invalid request: `method` must be present and a string",
        ));
    };
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    let result: Result<Value, RpcError> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "brainiac", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(state, &params).await,
        other => Err(RpcError::new(-32601, format!("method not found: {other}"))),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(e) => error_frame(id, e.code, e.message),
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
        },
        {
            "name": "knowledge_propose",
            "description": "Nominate a raw or candidate memory for promotion to the next status tier. A maintainer of the owning team reviews it — nothing becomes canonical without a human. Use after memory_add once you believe a captured learning deserves org-wide standing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "The memory id (memory:<uuid> citations or search results)" }
                },
                "required": ["memory_id"]
            }
        },
        {
            "name": "memory_feedback",
            "description": "Report how a retrieved memory held up in practice: helpful (it was right and useful), wrong (factually incorrect), or outdated (was true, no longer is). This closes the retrieval loop — verdicts drive ranking and re-verification.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "The memory id you were given (memory:<uuid> citations or search results)" },
                    "verdict": { "type": "string", "enum": ["helpful", "wrong", "outdated"] },
                    "note": { "type": "string", "description": "Optional: what happened (especially for wrong/outdated)" }
                },
                "required": ["memory_id", "verdict"]
            }
        }
    ])
}

async fn call_tool(state: &McpState, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RpcError::new(-32602, "tools/call requires a string `name`"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let payload = match name {
        "memory_search" => memory_search(state, &args).await,
        "memory_context" => memory_context(state, &args).await,
        "memory_add" => memory_add(state, &args).await,
        "entity_lookup" => entity_lookup(state, &args).await,
        "memory_feedback" => memory_feedback(state, &args).await,
        "knowledge_propose" => knowledge_propose(state, &args).await,
        other => return Err(RpcError::new(-32602, format!("unknown tool: {other}"))),
    };

    match payload {
        Ok(value) => Ok(json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "isError": false
        })),
        // A malformed call is a protocol error, not a tool result.
        Err(ToolError::InvalidParams(msg)) => Err(RpcError::new(-32602, msg)),
        // A deliberate refusal is safe to show the agent.
        Err(ToolError::Rejected(msg)) => Ok(tool_error(&msg)),
        // An internal fault: log the detail, hand back a generic message so no
        // internal/DB string ever reaches the agent.
        Err(ToolError::Internal(e)) => {
            tracing::error!(tool = name, error = ?e, "MCP tool internal error");
            Ok(tool_error(
                "brainiac hit an internal error handling this call; it has been logged",
            ))
        }
    }
}

async fn memory_search(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let query = within_cap(required_str(args, "query")?, MAX_QUERY_CHARS, "query")?;
    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(10).min(25) as usize;
    let as_of: Option<DateTime<Utc>> = args
        .get("as_of")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.store.pool(),
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
    // Trust signals: what previous readers reported about these memories, so
    // the agent can weigh a disputed memory instead of trusting it blindly.
    // One batched query for the whole result set — never an N+1.
    let ids: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
    let trust = brainiac_store::feedback::trust_for(&mut tx, &ids).await?;
    Ok(json!({
        "memories": hits.iter().map(|h| {
            let t = trust.get(&h.memory.id).cloned().unwrap_or_default();
            let mut m = json!({
                "id": h.memory.id,
                "kind": h.memory.kind.as_str(),
                "status": h.memory.status.as_str(),
                "content": h.memory.content,
                "via_graph": h.via_graph,
                "provenance_id": h.memory.provenance_id,
                "entity_anchors": h.anchors.iter().map(|a| json!({
                    "id": a.id, "name": a.name,
                })).collect::<Vec<_>>(),
            });
            if !t.is_empty() {
                m["feedback"] = json!({
                    "helpful": t.helpful,
                    "wrong": t.wrong,
                    "outdated": t.outdated,
                    "disputed": t.disputed(),
                });
                if t.disputed() {
                    m["warning"] = json!(
                        "readers have reported this memory as wrong or outdated and it has not been re-verified — treat it as unconfirmed"
                    );
                }
            }
            m
        }).collect::<Vec<_>>()
    }))
}

async fn memory_context(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let hint = within_cap(
        required_str(args, "task_hint")?,
        MAX_QUERY_CHARS,
        "task_hint",
    )?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.store.pool(),
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

async fn memory_add(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let content = within_cap(required_str(args, "content")?, MAX_CONTENT_CHARS, "content")?;
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

async fn knowledge_propose(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    use sqlx::Row;
    let memory_id = required_uuid(args, "memory_id")?;

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    // Visibility gate under the caller's RLS — proposing a memory you can't
    // read is refused as not-found (no existence oracle).
    let row = sqlx::query("SELECT status::text AS status FROM memories WHERE id = $1")
        .bind(memory_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| rejected("memory not found"))?;
    let status: String = row.get("status");
    let (from, to) = match status.as_str() {
        "raw" => (
            brainiac_core::MemoryStatus::Raw,
            brainiac_core::MemoryStatus::Candidate,
        ),
        "candidate" => (
            brainiac_core::MemoryStatus::Candidate,
            brainiac_core::MemoryStatus::Canonical,
        ),
        other => {
            return Err(rejected(format!(
                "only raw or candidate memories can be proposed (this one is {other})"
            )))
        }
    };
    let pending = sqlx::query(
        "SELECT 1 FROM promotions
         WHERE memory_id = $1 AND policy_decision = 'needs_review' AND reviewed_at IS NULL",
    )
    .bind(memory_id)
    .fetch_optional(&mut *tx)
    .await?;
    if pending.is_some() {
        return Err(rejected("already awaiting review"));
    }

    brainiac_store::governance::insert_promotion(
        &mut tx,
        state.principal.org_id,
        memory_id,
        from,
        to,
        brainiac_core::PolicyDecision::NeedsReview,
        "mcp.knowledge_propose",
    )
    .await?;
    tx.commit().await?;
    Ok(json!({
        "proposed": true,
        "memory_id": memory_id,
        "from_status": from.as_str(),
        "to_status": to.as_str(),
        "review": "a maintainer of the owning team must approve",
    }))
}

async fn memory_feedback(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let memory_id = required_uuid(args, "memory_id")?;
    let verdict = required_str(args, "verdict")?;
    if !brainiac_store::feedback::VERDICTS.contains(&verdict) {
        return Err(rejected("verdict must be one of helpful|wrong|outdated"));
    }
    let note = args
        .get("note")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(note) = note {
        within_cap(note, MAX_NOTE_CHARS, "note")?;
    }

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    // Visibility gate under the caller's RLS: feedback on a memory you can't
    // read is refused as not-found (FK checks alone would bypass RLS and
    // leak existence).
    let visible = sqlx::query("SELECT 1 FROM memories WHERE id = $1")
        .bind(memory_id)
        .fetch_optional(&mut *tx)
        .await?;
    if visible.is_none() {
        return Err(rejected("memory not found"));
    }
    brainiac_store::feedback::insert(
        &mut tx,
        Uuid::new_v4(),
        state.principal.org_id,
        memory_id,
        state.principal.user_id,
        verdict,
        note,
    )
    .await?;
    let summary = brainiac_store::feedback::summary(&mut tx, memory_id).await?;
    tx.commit().await?;
    Ok(json!({
        "recorded": true,
        "memory_id": memory_id,
        "verdict": verdict,
        "feedback_totals": summary.iter().map(|(v, n)| json!({
            "verdict": v, "count": n,
        })).collect::<Vec<_>>(),
    }))
}

async fn entity_lookup(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    use sqlx::Row;
    let name = within_cap(required_str(args, "name")?, MAX_NAME_CHARS, "name")?;
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
