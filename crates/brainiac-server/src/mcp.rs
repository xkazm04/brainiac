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
//! knowledge_propose, memory_feedback, memory_provenance.

use std::sync::Arc;

use anyhow::{Context, Result};
use brainiac_core::embed::Embedder;
use brainiac_core::{MemoryKind, MemoryStatus, Principal};
use brainiac_store::retrieval::RetrievalFilters;
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
/// Bounded excerpt of a source's raw text returned by `memory_provenance` — a
/// citation handle, never the whole (possibly huge) transcript.
const SOURCE_EXCERPT_CHARS: usize = 500;
/// Max bytes in a single JSON-RPC frame (one stdio line).
///
/// The per-field caps above are enforced AFTER the line is parsed, so without a
/// bound at the transport a caller could stream an unbounded line — simply never
/// sending a newline — and OOM the process before any cap ran. The hardening note
/// above promises "a runaway caller can never hand us an unbounded blob to embed,
/// store, or scan"; that promise needs a limit here, not just on the fields. 1 MB
/// comfortably fits MAX_CONTENT_CHARS (8k) plus JSON-RPC overhead.
const MAX_FRAME_BYTES: u64 = 1_000_000;

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

/// Optional point-in-time view. STRICT: an unparseable `as_of` is an error, not a
/// silent fallback to "now".
///
/// This used to be `.and_then(|s| s.parse().ok())`, deliberately lenient — so a
/// caller asking "what was true on 2026-01-01" got "what is true NOW", with no
/// error, no marker, and no way to detect the substitution. In a temporal memory
/// engine that is a confidently-wrong answer the agent will act on: a superseded
/// fact reappears as current, a not-yet-valid one is served early. The lenient
/// path was also easy to hit by accident — RFC3339 requires an offset, so the very
/// common date-only `2026-01-01` fails to parse. Every other narrowing param
/// (`scope`, `kinds`, `min_confidence`) already errors with -32602; `as_of` now
/// matches them.
fn parse_as_of(args: &Value) -> Result<Option<DateTime<Utc>>, ToolError> {
    match args.get("as_of") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let s = v
                .as_str()
                .ok_or_else(|| invalid("`as_of` must be an RFC3339 timestamp string"))?;
            s.parse::<DateTime<Utc>>().map(Some).map_err(|_| {
                invalid(format!(
                    "`as_of` must be an RFC3339 timestamp with an offset \
                     (e.g. 2026-01-01T00:00:00Z); got {s:?}"
                ))
            })
        }
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
            // Serve path: refuse to start on a version whose reembed backfill did
            // not complete, rather than silently answering from a half-embedded
            // corpus. Writers (the worker) still use ensure_embedding_version.
            let v = brainiac_store::memories::serving_embedding_version(
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
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    tracing::info!("brainiac MCP server on stdio");
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        // Bounded read: `lines()`/`read_until` would buffer an unbounded frame.
        let n = match (&mut reader)
            .take(MAX_FRAME_BYTES)
            .read_until(b'\n', &mut buf)
            .await
        {
            Ok(n) => n,
            Err(e) => {
                // A read fault means the peer is gone; end the session cleanly.
                tracing::warn!(error = %e, "stdin read error; ending MCP session");
                break;
            }
        };
        if n == 0 {
            break; // clean EOF on stdin
        }
        // Hit the cap without a terminator ⇒ the peer is streaming an unbounded
        // frame. Resyncing would mean draining the rest of it, which is equally
        // unbounded, so a frame-size violation ends the session rather than being
        // treated as a recoverable frame fault. (No newline but UNDER the cap is
        // just EOF on a final line — that one is processed normally.)
        if !buf.ends_with(b"\n") && n as u64 >= MAX_FRAME_BYTES {
            tracing::warn!(
                limit = MAX_FRAME_BYTES,
                "MCP frame exceeded the size cap with no newline; ending session"
            );
            break;
        }
        let line = String::from_utf8_lossy(&buf)
            .trim_end_matches(['\r', '\n'])
            .to_string();
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
            "description": "Hybrid search over the organization's governed memory (vector + keyword + knowledge-graph expansion) — permission-scoped to you, provenance attached. Reach for this MID-TASK, not just at the start: whenever you are about to make a non-trivial decision, change shared or cross-team behavior, or rely on an assumption you have not checked THIS session — especially deep into a long task, when your earlier context has drifted. The org may hold a pitfall, an invariant, or a REVERSAL about the exact code in front of you that is not written in this repo, and it will not surface unless you ask. By default only reviewed knowledge (candidate + canonical) is returned; each below-canonical result is tagged as provisional.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What you want to know" },
                    "k": { "type": "integer", "description": "Max results (default 10, cap 25)" },
                    "as_of": { "type": "string", "description": "RFC3339 timestamp: answer as of this moment in time" },
                    "scope": { "type": "string", "enum": ["team", "org"], "description": "\"org\" (default): everything you can see across the org. \"team\": only memories owned by your team." },
                    "kinds": { "type": "array", "items": { "type": "string", "enum": ["fact", "decision", "pattern", "pitfall", "howto"] }, "description": "Keep only these memory kinds (default: all)" },
                    "min_confidence": { "type": "number", "description": "0-1: drop memories below this extractor confidence (memories with no confidence are dropped)" },
                    "include_unreviewed": { "type": "boolean", "description": "Default false. When false, raw (never-reviewed) memories are excluded — you see only knowledge that cleared at least an automated gate. Set true ONLY when you deliberately want to see unpromoted/raw captures (e.g. triaging your own recent memory_add); such rows carry no governance guarantee." },
                    "include_contested": { "type": "boolean", "description": "Default false. When false, memories in an UNRESOLVED contradiction are withheld (the response reports how many, so you know the area is contested and can escalate). Their truth is undetermined; do not act on them. Set true only to inspect the conflict for reconciliation — never to pick a side by recency or provenance." }
                },
                "required": ["query"]
            }
        },
        {
            "name": "memory_context",
            "description": "The most relevant CANONICAL (human-certified) organizational knowledge for a task, token-budgeted, each entry carrying a compact provenance ref. Call this when you START work to load what the org already knows — and call it again (or memory_search) when you MOVE to a new area, hit a decision point, or are about to touch shared behavior. It is a canonical-only briefing, so it is safe but conservative; for anything mid-task or narrower, memory_search is the sharper tool. Organizational knowledge does not announce itself — if you never ask, you build as if the org knows nothing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_hint": { "type": "string", "description": "Short description of the task/repo/area you are working on" },
                    "as_of": { "type": "string", "description": "RFC3339 timestamp: build the bundle as of this moment in time" }
                },
                "required": ["task_hint"]
            }
        },
        {
            "name": "memory_add",
            "description": "Record a piece of durable knowledge (fact, decision, pattern, pitfall, howto). It enters the extraction/review pipeline as raw knowledge — it is NOT immediately canonical. The optional kind/entities hints steer the extractor.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "One self-contained natural-language statement" },
                    "kind": { "type": "string", "enum": ["fact", "decision", "pattern", "pitfall", "howto"], "description": "Optional: the kind of knowledge this is — a hint to the extractor" },
                    "entities": { "type": "array", "items": { "type": "string" }, "description": "Optional: names of the services/repos/techs/features this concerns — surfaced to the extractor so it anchors them" }
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
            "description": "Report how a retrieved memory held up in practice: helpful/useful (it was right and useful), wrong (factually incorrect), or outdated/stale (was true, no longer is). This closes the retrieval loop — verdicts drive ranking and re-verification.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "The memory id you were given (memory:<uuid> citations or search results)" },
                    "verdict": { "type": "string", "enum": ["helpful", "useful", "wrong", "outdated", "stale"], "description": "helpful (alias useful), wrong, or outdated (alias stale)" },
                    "note": { "type": "string", "description": "Optional: what happened (especially for wrong/outdated)" }
                },
                "required": ["memory_id", "verdict"]
            }
        },
        {
            "name": "memory_provenance",
            "description": "Trace a memory's evidence chain to decide whether to trust it: WHO it came from (recorded_by = the human whose session it originated in, when there was one; plus the recording actor/model), WHEN (created_at, and the validity window valid_from/valid_to), whether it is STILL TRUE (still_valid + status), the originating source with a short excerpt, and the canonical entities it anchors. Use before acting on a served memory whose age or authorship matters. Scoped to you: a memory you cannot see returns 'not found'.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "The memory id you were given (memory:<uuid> citations or search results)" }
                },
                "required": ["memory_id"]
            }
        },
        {
            "name": "doc_search",
            "description": "Find knowledge-base PAGES (compiled, human-published views over the org's memories) by topic. A page is the org's settled, reviewed account of a service or topic — prefer it over raw memory_search when you need the whole picture of something rather than a specific fact. Returns page slugs + titles; read one with doc_get.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Topic, service, or entity you want the org's page on" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "doc_get",
            "description": "Read a knowledge-base page as markdown. Every factual sentence carries an inline [m:<uuid>] citation to the governed memory it came from, so you can trace or feed back on any claim (memory_provenance / memory_feedback). IMPORTANT: the page reflects only what a named human PUBLISHED — a page marked stale:true has pending changes not yet reviewed, and claims marked not-yet-shipped describe intent, not production. Scoped to you: a page you cannot see returns 'not found'.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Page slug, e.g. `psp-gateway` (from doc_search)" }
                },
                "required": ["slug"]
            }
        },
        {
            "name": "standards_for",
            "description": "The org's ADOPTED coding standards for a tech stack — fetch these BEFORE writing or reviewing code so your work follows the org's ratified judgment, not your defaults. Each rule carries its statement, how strongly it binds (mandatory / recommended / experimental), the rationale, and verbatim good/bad examples. Only rules a named human adopted are returned; proposals never reach you as if they were policy.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "stack": { "type": "string", "description": "Tech stack, e.g. `rust`, `typescript`, `general`. Omit for all stacks." },
                    "category": { "type": "string", "description": "Narrow to one category, e.g. `errors`, `testing`." }
                }
            }
        },
        {
            "name": "skill_search",
            "description": "Find org skills — packaged, versioned procedures a maintainer published for agents like you (runbooks, review checklists, codified workflows). Returns slugs + descriptions; download one with skill_fetch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What you are trying to do" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "skill_fetch",
            "description": "Download a skill's current PUBLISHED bundle (manifest + markdown body + resources) by slug. Follow the skill's instructions as org-ratified procedure. A skill with no published version returns not-found — a draft nobody signed is never served to you.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Skill slug (from skill_search)" }
                },
                "required": ["slug"]
            }
        },
        {
            "name": "standard_propose",
            "description": "Propose a coding-standard candidate when you found a pattern the org's standards don't cover — or found yourself deliberately diverging from your own approach because of org context. The outcome is ONLY ever a proposal: a named human adopts or rejects it, never you. Cite an evidence memory id if one backs the pattern (a proposal without evidence can only be adopted by an explicit human decree). Deduplicated: if the org already has this rule — adopted, proposed, or REJECTED — you get that standard back instead of a new one; respect a rejection rather than rephrasing it. Rate-limited per session identity; propose the one or two patterns that mattered, not everything you noticed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Short practice name, e.g. `service retry policy` (the dedup key)" },
                    "statement": { "type": "string", "description": "The rule, in ONE sentence" },
                    "stack": { "type": "string", "description": "Tech stack (`rust`, `typescript`, …). Omit for `general`." },
                    "category": { "type": "string", "description": "Category (`errors`, `testing`, …). Omit for `practice`." },
                    "rationale": { "type": "string", "description": "Why — the incident, the cost, the context" },
                    "examples_md": { "type": "string", "description": "Good/bad examples as markdown, verbatim" },
                    "evidence_memory_id": { "type": "string", "description": "A memory id backing this (from memory_search / memory_add)" }
                },
                "required": ["name", "statement"]
            }
        },
        {
            "name": "skill_report_usage",
            "description": "Report that you applied a skill or checked your work against a standard — this keeps the org's library honest (rules and skills nobody uses get retired). Usage is counted for your TEAM, never for you personally; the storage cannot record a person.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "artifact_kind": { "type": "string", "enum": ["standard", "skill"], "description": "What you used" },
                    "slug": { "type": "string", "description": "The standard's or skill's slug" },
                    "event": { "type": "string", "enum": ["check", "apply"], "description": "`check`: compared work against a standard. `apply`: ran a skill." }
                },
                "required": ["artifact_kind", "slug", "event"]
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
        "memory_provenance" => memory_provenance(state, &args).await,
        "doc_get" => doc_get(state, &args).await,
        "doc_search" => doc_search(state, &args).await,
        "standards_for" => standards_for(state, &args).await,
        "standard_propose" => standard_propose(state, &args).await,
        "skill_search" => skill_search(state, &args).await,
        "skill_fetch" => skill_fetch(state, &args).await,
        "skill_report_usage" => skill_report_usage(state, &args).await,
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

/// Build [`RetrievalFilters`] from the optional narrowing params documented in
/// ARCHITECTURE.md §5.1. Every value is validated up front — a malformed one is
/// `-32602 InvalidParams` per the hardening contract, never a silent no-op.
/// Unset params narrow nothing (byte-identical to the prior default filters).
///
/// `scope` is mapped onto what [`RetrievalFilters`] actually supports —
/// team ownership (`team_id`); it has no visibility lever, so team membership is
/// the axis we express:
///   - `"org"` (the documented default): no team filter. RLS already caps the
///     caller to their org, so this is the org-wide view — everything they can
///     see (org-visible knowledge plus every team they belong to).
///   - `"team"`: restrict to memories OWNED BY the caller's primary team
///     (`principal.team_ids[0]`). A caller who belongs to no team has nothing to
///     scope to, so it is refused.
fn parse_filters(state: &McpState, args: &Value) -> Result<RetrievalFilters, ToolError> {
    let mut filters = RetrievalFilters::default();

    if let Some(scope) = args.get("scope") {
        let scope = scope
            .as_str()
            .ok_or_else(|| invalid("`scope` must be a string"))?
            .trim();
        match scope {
            "org" => {} // org-wide view = the RLS-scoped default (no team filter)
            "team" => {
                let team = state.principal.team_ids.first().copied().ok_or_else(|| {
                    rejected("`scope`=\"team\" needs a team to scope to, but you belong to none")
                })?;
                filters.team_id = Some(team);
            }
            other => {
                return Err(invalid(format!(
                    "`scope` must be \"team\" or \"org\" (got \"{other}\")"
                )))
            }
        }
    }

    if let Some(kinds) = args.get("kinds") {
        let arr = kinds
            .as_array()
            .ok_or_else(|| invalid("`kinds` must be an array of memory kinds"))?;
        let mut out = Vec::with_capacity(arr.len());
        for k in arr {
            let s = k
                .as_str()
                .ok_or_else(|| invalid("each `kinds` entry must be a string"))?
                .trim();
            let kind = MemoryKind::parse(s).ok_or_else(|| {
                invalid(format!(
                    "unknown memory kind `{s}` (fact|decision|pattern|pitfall|howto)"
                ))
            })?;
            if !out.contains(&kind) {
                out.push(kind);
            }
        }
        filters.kinds = out;
    }

    if let Some(mc) = args.get("min_confidence") {
        let c = mc
            .as_f64()
            .ok_or_else(|| invalid("`min_confidence` must be a number in [0,1]"))?;
        if !(0.0..=1.0).contains(&c) {
            return Err(invalid(format!(
                "`min_confidence` must be in [0,1] (got {c})"
            )));
        }
        filters.min_confidence = Some(c as f32);
    }

    // Governance floor. `raw` memories are pipeline extractions (or unpromoted
    // `memory_add`s) that NO human and NO policy has reviewed — serving them to
    // an agent as if they were org knowledge is exactly what the review queue
    // exists to prevent, so they are excluded by default. `Candidate` keeps
    // candidate+canonical; the caller can drop the floor to see unreviewed rows,
    // but only by asking for them explicitly (and every below-canonical row it
    // gets back is tagged — see `memory_search`). This is the one governance
    // guarantee that must hold on the tool an agent actually reaches for mid-task.
    let include_unreviewed = args
        .get("include_unreviewed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !include_unreviewed {
        filters.min_status = Some(MemoryStatus::Candidate);
    }

    Ok(filters)
}

async fn memory_search(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let query = within_cap(required_str(args, "query")?, MAX_QUERY_CHARS, "query")?;
    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(10).min(25) as usize;
    let as_of = parse_as_of(args)?;
    let filters = parse_filters(state, args)?;

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
            filters,
        },
    )
    .await?;
    // Trust signals: what previous readers reported about these memories, so
    // the agent can weigh a disputed memory instead of trusting it blindly.
    // One batched query for the whole result set — never an N+1.
    let ids: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
    let trust = brainiac_store::feedback::trust_for(&mut tx, &ids).await?;
    // Open contradictions touching the result set: an agent must never be
    // handed one side of a live conflict without being told the other exists.
    // One batched, RLS-scoped query (never an N+1) — orthogonal to the
    // feedback-derived `disputed` signal above.
    let contradictions = brainiac_store::governance::open_contradictions_for(&mut tx, &ids).await?;
    // An OPEN contradiction means the org has not determined which side is true —
    // so the memory's truth value is UNKNOWN, not merely "flagged". Serving it as
    // an actionable result lets an agent pick a side on surface cues (recency,
    // provenance), and a well-crafted poison wins that (UAT run 2026-07-13-l2).
    // So, like the raw governance floor, contested memories are WITHHELD by
    // default and surfaced only on explicit `include_contested:true` — where they
    // carry a hard "do not adjudicate this yourself" warning. This is symmetric
    // with `include_unreviewed` and it makes the governance debt show up as
    // missing knowledge (pressure to reconcile), never as served poison.
    let include_contested = args
        .get("include_contested")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let withheld_contested = if include_contested {
        0
    } else {
        hits.iter()
            .filter(|h| contradictions.contains_key(&h.memory.id))
            .count()
    };
    Ok(json!({
        "memories": hits.iter().filter(|h| {
            include_contested || !contradictions.contains_key(&h.memory.id)
        }).map(|h| {
            let t = trust.get(&h.memory.id).cloned().unwrap_or_default();
            let mut m = json!({
                "id": h.memory.id,
                "kind": h.memory.kind.as_str(),
                "status": h.memory.status.as_str(),
                "content": h.memory.content,
                // "Is it still true / how fresh": the validity window and record
                // time travel WITH the result, so an agent can weight a fact by
                // age and see a live vs. time-boxed one. valid_to == null = live.
                "valid_from": h.memory.valid_from,
                "valid_to": h.memory.valid_to,
                "created_at": h.memory.created_at,
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
            // Governance visibility at the point of consumption: the reviewer's
            // work is invisible unless the payload says whether a memory cleared
            // it. Canonical = a human promoted it; candidate = it passed an
            // auto/policy gate but is NOT human-certified org knowledge. Say so,
            // in the same shape as the disputed/contradiction warnings above, so
            // an agent can weight an unreviewed row instead of trusting it blind.
            if h.memory.status != MemoryStatus::Canonical {
                m["governance"] = json!("candidate");
                m["governance_warning"] = json!(
                    "this memory is NOT canonical — it passed an automated gate but no human maintainer has certified it as org knowledge; weight it as provisional. (raw, never-reviewed memories are excluded unless you pass include_unreviewed:true.)"
                );
            }
            if let Some(flags) = contradictions.get(&h.memory.id) {
                m["contradicted"] = json!(true);
                m["contradicts"] = json!(flags.iter().map(|f| json!({
                    "contradiction_id": f.contradiction_id,
                    "counterpart_memory_id": f.counterpart_id,
                })).collect::<Vec<_>>());
                m["actionable"] = json!(false);
                m["contradiction_warning"] = json!(
                    "this memory is in an OPEN, UNRESOLVED contradiction with another memory (see `contradicts`). The org has not determined which is true. Do NOT adjudicate this yourself — recency, provenance, or confidence do NOT decide which side is correct in an unresolved contradiction. Escalate to a maintainer or verify against source; do not act on either side as fact."
                );
            }
            m
        }).collect::<Vec<_>>(),
        // Governance debt made visible: N results matched but are withheld because
        // they sit in unresolved contradictions. The agent is told they exist (so
        // it knows the area is contested and can escalate) without being handed a
        // side to act on. `include_contested:true` surfaces them, warned.
        "contested_withheld": withheld_contested,
        "note": if withheld_contested > 0 {
            json!(format!(
                "{withheld_contested} matching memory(ies) are withheld: they are in unresolved contradictions and are not settled knowledge. This area is contested — a maintainer must reconcile it. Pass include_contested:true to see them (they cannot be safely acted on until resolved)."
            ))
        } else {
            Value::Null
        }
    }))
}

/// Render a compact provenance citation handle for a packed context line: the
/// recording actor kind and its sharpest identifier — the model ref when the
/// memory was LLM-produced, else the actor ref. `None` when the row carries no
/// usable identity (so the caller omits the tag entirely).
fn provenance_tag(p: &brainiac_store::governance::ProvenanceRef) -> Option<String> {
    let kind = p.actor_kind.as_deref()?;
    let who = p.model_ref.as_deref().or(p.actor_ref.as_deref());
    Some(match who {
        Some(who) => format!("via {kind} ({who})"),
        None => format!("via {kind}"),
    })
}

async fn memory_context(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let hint = within_cap(
        required_str(args, "task_hint")?,
        MAX_QUERY_CHARS,
        "task_hint",
    )?;
    // Optional point-in-time view — strictly parsed (see `parse_as_of`).
    let as_of = parse_as_of(args)?;

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    // Push the Canonical floor INTO the SQL candidate stage (RetrievalFilters
    // min_status) rather than filtering post-hoc: the full k-budget is now spent
    // on servable canonical rows, so a task whose top matches are mostly raw no
    // longer yields a thin (or empty) bundle. Graph-expansion extras re-apply the
    // same floor in `RetrievalFilters::admits`, so every returned hit is
    // canonical and there is nothing left to filter here.
    let hits = brainiac_store::retrieval::search(
        &mut tx,
        state.store.pool(),
        state.embedder.as_ref(),
        state.embedding_version,
        &brainiac_store::retrieval::RetrievalRequest {
            query: hint.to_string(),
            k: 25,
            as_of,
            filters: RetrievalFilters {
                min_status: Some(brainiac_core::MemoryStatus::Canonical),
                ..Default::default()
            },
        },
    )
    .await?;

    // Whole entries packed into the char budget, in blended-score order (the
    // hits are already ranked and truncated by the retriever).
    let bundle_ids: Vec<Uuid> = hits.iter().map(|h| h.memory.id).collect();
    // Open contradictions touching this bundle: a text-consuming agent must see
    // the conflict inline, not only in structured search output. One batched,
    // RLS-scoped query.
    let contradictions =
        brainiac_store::governance::open_contradictions_for(&mut tx, &bundle_ids).await?;
    // Compact provenance refs for every packed line (ARCHITECTURE.md §4.6
    // "attach provenance refs"). BATCHED — one query for the whole bundle, never
    // N single `provenance_for_memory` calls.
    let provenance = brainiac_store::governance::provenance_refs_for(&mut tx, &bundle_ids).await?;
    // Partition the bundle. A memory locked in an OPEN (unresolved) contradiction
    // is NOT settled knowledge — the org has not determined which side is true —
    // so it must not sit in the actionable "rely on this, cite it" set alongside
    // uncontested canonicals. Serving both sides of a live conflict as equal
    // canonicals is what lets a well-provenanced poison win the agent's own
    // tiebreak (UAT run 2026-07-13-l2). Settled memories go in the bundle;
    // contested ones are quarantined into a separate DO-NOT-ACT section that
    // frames them as needing reconciliation, not as fact.
    let mut bundle = String::new();
    let mut contested = String::new();
    let mut included = 0usize;
    let mut contested_count = 0usize;
    for h in &hits {
        let mut line = format!(
            "- [{}] {} (memory:{})",
            h.memory.kind.as_str(),
            h.memory.content,
            h.memory.id
        );
        if let Some(tag) = provenance.get(&h.memory.id).and_then(provenance_tag) {
            line.push_str(&format!(" — {tag}"));
        }
        // "When", compactly: the flat bundle carried no date, so a stale and a
        // fresh canonical fact read identically. Stamp the effective date (the
        // validity-window start, else the record time) so the reader can judge
        // recency without a second `memory_provenance` round-trip.
        let effective = h.memory.valid_from.unwrap_or(h.memory.created_at);
        line.push_str(&format!(" [as of {}]", effective.format("%Y-%m-%d")));
        if let Some(flags) = contradictions.get(&h.memory.id) {
            for f in flags {
                line.push_str(&format!(" (conflicts with memory:{})", f.counterpart_id));
            }
            line.push('\n');
            contested.push_str(&line);
            contested_count += 1;
            continue;
        }
        line.push('\n');
        if bundle.len() + line.len() > CONTEXT_CHAR_BUDGET && included > 0 {
            break;
        }
        bundle.push_str(&line);
        included += 1;
    }
    let mut context = format!(
        "Organizational knowledge relevant to your task ({included} settled canonical memories, cite ids when you rely on them):\n{bundle}"
    );
    if contested_count > 0 {
        context.push_str(&format!(
            "\n⚠ CONTESTED — {contested_count} memory(ies) in this area are in UNRESOLVED contradictions. \
             The org has NOT determined which is true, so these are NOT settled knowledge. \
             Do NOT act on them or cite them as fact; a maintainer must reconcile them first. \
             If you need this, escalate or verify against the source/code:\n{contested}"
        ));
    }
    Ok(json!({
        "context": context,
        "memories_included": included,
        "contested_count": contested_count
    }))
}

/// `entities` cap — a manual note anchors a handful of things, never a bulk
/// list; a runaway array is rejected before it can bloat the source text.
const MAX_ENTITY_HINTS: usize = 32;

/// The optional entity-name hints on `memory_add`: an array of non-empty,
/// bounded, de-duplicated surface forms. Validation mirrors the rest of the
/// surface — a bad shape is `-32602`, an oversized set is a tool error.
fn parse_entity_names(args: &Value) -> Result<Vec<String>, ToolError> {
    let Some(v) = args.get("entities") else {
        return Ok(Vec::new());
    };
    let arr = v
        .as_array()
        .ok_or_else(|| invalid("`entities` must be an array of names"))?;
    if arr.len() > MAX_ENTITY_HINTS {
        return Err(rejected(format!(
            "too many entities ({}); the limit is {MAX_ENTITY_HINTS}",
            arr.len()
        )));
    }
    let mut out: Vec<String> = Vec::with_capacity(arr.len());
    for e in arr {
        let name = e
            .as_str()
            .map(str::trim)
            .ok_or_else(|| invalid("each `entities` entry must be a string"))?;
        if name.is_empty() {
            return Err(invalid("`entities` names must not be empty"));
        }
        within_cap(name, MAX_NAME_CHARS, "entities")?;
        if !out.iter().any(|x| x.eq_ignore_ascii_case(name)) {
            out.push(name.to_string());
        }
    }
    Ok(out)
}

/// Weave the optional kind/entities hints into the source text the extractor
/// reads. The pipeline consumes ONLY `sources.raw_text` (worker::process_job),
/// so a short natural-language preamble the prompt-driven extractor incorporates
/// is the in-scope lever that actually reaches extraction — no worker change, no
/// new column. With no hints the stored text is the content verbatim (today's
/// behavior preserved byte-for-byte).
fn build_source_text(content: &str, kind: Option<MemoryKind>, entities: &[String]) -> String {
    let mut hints: Vec<String> = Vec::new();
    if let Some(k) = kind {
        // Phrased as a non-restrictive hint: the flywheel run showed
        // "recording this as a pitfall" led the extractor to take ONLY the
        // pitfall and drop a co-located howto/decision. Signal the primary kind
        // without narrowing extraction to it (the prompt reinforces this).
        hints.push(format!(
            "The author considers this primarily a {}, but extract every distinct durable \
             learning it contains, not only the {}.",
            k.as_str(),
            k.as_str()
        ));
    }
    if !entities.is_empty() {
        hints.push(format!(
            "It concerns these entities: {}.",
            entities.join(", ")
        ));
    }
    if hints.is_empty() {
        content.to_string()
    } else {
        format!("{content}\n\n[Context for extraction: {}]", hints.join(" "))
    }
}

async fn memory_add(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let content = within_cap(required_str(args, "content")?, MAX_CONTENT_CHARS, "content")?;
    // Optional kind hint, validated against MemoryKind.
    let kind = match args.get("kind") {
        Some(v) => {
            let s = v
                .as_str()
                .ok_or_else(|| invalid("`kind` must be a string"))?
                .trim();
            Some(MemoryKind::parse(s).ok_or_else(|| {
                invalid(format!(
                    "unknown memory kind `{s}` (fact|decision|pattern|pitfall|howto)"
                ))
            })?)
        }
        None => None,
    };
    let entities = parse_entity_names(args)?;
    let raw_text = build_source_text(content, kind, &entities);

    let team_id = state.principal.team_ids.first().copied();
    let source_id = Uuid::new_v4();
    let mut tx = state.store.scoped_tx(&state.principal).await?;
    brainiac_store::governance::insert_source(
        &mut tx,
        source_id,
        state.principal.org_id,
        team_id,
        "manual",
        &raw_text,
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
        "kind": kind.map(|k| k.as_str()),
        "entities": entities,
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

/// Canonicalize the documented feedback vocabulary onto the STORED verdicts.
/// ARCHITECTURE.md §5.1 says useful / stale / wrong; the corpus stores
/// helpful / wrong / outdated. Synonyms are mapped BEFORE validation so the doc
/// terms are accepted while the stored vocabulary is left unchanged.
fn canonical_verdict(v: &str) -> &str {
    match v {
        "useful" => "helpful",
        "stale" => "outdated",
        other => other,
    }
}

async fn memory_feedback(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let memory_id = required_uuid(args, "memory_id")?;
    let verdict = canonical_verdict(required_str(args, "verdict")?);
    if !brainiac_store::feedback::VERDICTS.contains(&verdict) {
        return Err(rejected(
            "verdict must be one of helpful|wrong|outdated (aliases: useful→helpful, stale→outdated)",
        ));
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

// ── knowledge base (§8.4) ───────────────────────────────────────────────
//
// Agents get READ access to pages and nothing else. There is deliberately no
// `doc_write` / `doc_edit` tool: an agent contributes by writing MEMORIES
// (memory_add / knowledge_propose), which pass the review gate and then flow
// into pages by composition. Letting an agent author a page directly would put
// unreviewed prose into the org's wiki through the one door the whole product
// exists to keep shut.

/// Read a page. Everything runs under the operator's RLS scope, so a page the
/// developer cannot see is simply "not found" — existence is itself information.
async fn doc_get(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let slug = within_cap(required_str(args, "slug")?, MAX_NAME_CHARS, "slug")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;

    let Some(doc) = brainiac_store::documents::get_document_by_slug(&mut tx, slug).await? else {
        return Ok(json!({ "found": false, "slug": slug }));
    };
    let current = brainiac_store::documents::current_revision(&mut tx, doc.id).await?;
    tx.commit().await?;

    // An unpublished page has no content to serve. Handing an agent a draft
    // nobody signed would defeat the review gate as surely as letting it write
    // one — so we say the page exists and is unpublished, and stop there.
    let Some(rev) = current else {
        return Ok(json!({
            "found": true,
            "slug": doc.slug,
            "title": doc.title,
            "published": false,
            "note": "this page has no published revision yet — a maintainer has not signed one. Use memory_search for the underlying knowledge."
        }));
    };

    // Content was served to an agent: record the read (0025). Agents consuming
    // pages is exactly the liquidity the KB exists to create, so this channel
    // is worth telling apart from the console.
    crate::docs::record_read(&state.store, &state.principal, &doc, "mcp").await;

    Ok(json!({
        "found": true,
        "slug": doc.slug,
        "title": doc.title,
        "kind": doc.doc_kind.as_str(),
        "published": true,
        "published_at": rev.published_at,
        // The honest freshness signal: an underlying memory has changed and the
        // page has not recomposed yet, so what you are reading may already be
        // behind the org's actual belief.
        "stale": doc.dirty_at.is_some(),
        "content_md": rev.content_md,
        "cites": rev.composed_from,
    }))
}

/// Find pages by topic. Page-level retrieval, not memory-level: an agent that
/// needs the whole picture of a service should read the org's settled account of
/// it rather than reassembling one from twenty facts.
async fn doc_search(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    use sqlx::Row;
    let query = within_cap(required_str(args, "query")?, MAX_QUERY_CHARS, "query")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;

    // Lexical over title/slug + the published markdown. Deliberately simple:
    // pages are few and their titles name the thing they are about, so embedding
    // the corpus of pages would buy little and cost an index to keep fresh.
    // If page count ever makes this weak, the fix is a page embedding — not a
    // cleverer LIKE.
    let rows = sqlx::query(
        "SELECT d.slug, d.title, d.doc_kind, d.dirty_at IS NOT NULL AS stale
         FROM documents d
         LEFT JOIN document_revisions r ON r.id = d.current_revision
         WHERE d.status = 'published'
           AND (d.title ILIKE '%' || $1 || '%'
                OR d.slug ILIKE '%' || $1 || '%'
                OR r.content_md ILIKE '%' || $1 || '%')
         ORDER BY (d.title ILIKE '%' || $1 || '%') DESC, d.updated_at DESC
         LIMIT 10",
    )
    .bind(query)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(json!({
        "pages": rows.iter().map(|r| json!({
            "slug": r.get::<String, _>("slug"),
            "title": r.get::<String, _>("title"),
            "kind": r.get::<String, _>("doc_kind"),
            "stale": r.get::<bool, _>("stale"),
        })).collect::<Vec<_>>()
    }))
}

// ── the library (LIBRARY-PLAN LB1) ───────────────────────────────────────
//
// Agents get the DISTRIBUTION surface: adopted standards, published skill
// bundles, and the usage channel back. There is deliberately no propose tool
// yet (LB4) and no adopt/deprecate — an agent can never decree a rule, only a
// named human can, through the maintainer surface.

/// Record a usage signal in its own transaction, warn-only on failure — the
/// vital signs must never cost an agent its answer. Team-attributed; the
/// schema has no user column to fill (the never-a-leaderboard invariant).
async fn record_library_usage_quietly(
    state: &McpState,
    kind: brainiac_core::LibraryArtifactKind,
    artifact_id: uuid::Uuid,
    version: Option<&str>,
    event: brainiac_core::LibraryUsageEvent,
) {
    let outcome = async {
        let mut tx = state.store.scoped_tx(&state.principal).await?;
        brainiac_store::library::record_usage(
            &mut tx,
            state.principal.org_id,
            kind,
            artifact_id,
            version,
            event,
            state.principal.team_ids.first().copied(),
        )
        .await?;
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(e) = outcome {
        tracing::warn!(artifact = %artifact_id, error = %e, "library artifact served but usage not recorded");
    }
}

/// The org's ADOPTED rules for a stack. Proposals are never served — an agent
/// must not mistake a candidate for the org's judgment.
async fn standards_for(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let stack = match args.get("stack") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            within_cap(
                v.as_str()
                    .ok_or_else(|| invalid("`stack` must be a string"))?,
                MAX_NAME_CHARS,
                "stack",
            )?
            .to_string(),
        ),
    };
    let category = match args.get("category") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            within_cap(
                v.as_str()
                    .ok_or_else(|| invalid("`category` must be a string"))?,
                MAX_NAME_CHARS,
                "category",
            )?
            .to_string(),
        ),
    };

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let standards = brainiac_store::library::list_standards(
        &mut tx,
        stack.as_deref(),
        Some(brainiac_core::StandardLifecycle::Adopted),
    )
    .await?;
    tx.commit().await?;

    let rules: Vec<_> = standards
        .iter()
        .filter(|s| category.as_deref().is_none_or(|c| s.category == c))
        .collect();

    // Serving the rules IS the adoption signal's denominator: record a fetch
    // per rule, after the answer is safe.
    for s in &rules {
        record_library_usage_quietly(
            state,
            brainiac_core::LibraryArtifactKind::Standard,
            s.id,
            None,
            brainiac_core::LibraryUsageEvent::Fetch,
        )
        .await;
    }

    Ok(json!({
        "standards": rules.iter().map(|s| json!({
            "slug": s.slug,
            "stack": s.stack,
            "category": s.category,
            "statement": s.statement,
            "enforcement": s.enforcement.as_str(),
            "rationale": s.rationale,
            // Examples verbatim — never re-typed by a model.
            "examples_md": s.detail_md,
        })).collect::<Vec<_>>(),
        "note": "these are the org's adopted rules — follow mandatory ones, prefer recommended ones, and report divergence honestly rather than silently ignoring a rule"
    }))
}

/// Propose a standard candidate (LB4). The outcome is only ever a proposal;
/// dedup and the rate limit live in the store so REST and MCP cannot drift.
async fn standard_propose(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let name = within_cap(required_str(args, "name")?, 120, "name")?;
    let statement = within_cap(required_str(args, "statement")?, 500, "statement")?;
    let opt = |key: &str, cap: usize| -> Result<Option<String>, ToolError> {
        match args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => Ok(Some(
                within_cap(
                    v.as_str()
                        .ok_or_else(|| invalid(format!("`{key}` must be a string")))?,
                    cap,
                    key,
                )?
                .to_string(),
            )),
        }
    };
    let evidence = match args.get("evidence_memory_id") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_str()
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
                .ok_or_else(|| invalid("`evidence_memory_id` must be a memory uuid"))?,
        ),
    };

    let per_hour = std::env::var("BRAINIAC_LIB_PROPOSE_PER_HOUR")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(brainiac_store::library::DEFAULT_PROPOSE_PER_HOUR);

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let outcome = brainiac_store::library::propose_standard(
        &mut tx,
        &brainiac_store::library::Proposal {
            org_id: state.principal.org_id,
            author: state.principal.user_id,
            name: name.to_string(),
            statement: statement.to_string(),
            stack: opt("stack", MAX_NAME_CHARS)?,
            category: opt("category", MAX_NAME_CHARS)?,
            rationale: opt("rationale", 2_000)?,
            detail_md: opt("examples_md", 4_000)?,
            evidence_memory_id: evidence,
        },
        per_hour,
    )
    .await?;

    use brainiac_store::library::ProposeOutcome;
    Ok(match outcome {
        ProposeOutcome::Created(id) => {
            tx.commit().await?;
            json!({
                "outcome": "created",
                "standard_id": id,
                "note": "your proposal is a CANDIDATE waiting at the gate — a named human adopts or rejects it. Do not treat it as policy yet."
            })
        }
        ProposeOutcome::Duplicate {
            standard_id,
            lifecycle,
        } => json!({
            "outcome": "duplicate",
            "standard_id": standard_id,
            "lifecycle": lifecycle.as_str(),
            "note": match lifecycle.as_str() {
                "adopted" => "the org already adopted this — follow the existing rule rather than proposing it",
                "rejected" => "the org already considered and REJECTED this — respect that decision rather than rephrasing it",
                _ => "this idea is already at the gate — no second candidate was created",
            }
        }),
        ProposeOutcome::RateLimited { per_hour } => json!({
            "outcome": "rate_limited",
            "note": format!("proposal budget spent ({per_hour}/hour) — keep the remaining ideas for the session summary, or back the strongest one as a memory instead")
        }),
        ProposeOutcome::EvidenceNotFound => json!({
            "outcome": "evidence_not_found",
            "note": "the cited evidence memory does not exist or is not visible to you — propose without it, or memory_add the evidence first"
        }),
    })
}

/// Find published skills by what the agent is trying to do.
async fn skill_search(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    use sqlx::Row;
    let query = within_cap(required_str(args, "query")?, MAX_QUERY_CHARS, "query")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;

    // Lexical, like doc_search and for the same reason: skills are few and
    // named for what they do. If the catalog ever outgrows this, the fix is an
    // embedding — not a cleverer LIKE.
    let rows = sqlx::query(
        "SELECT slug, name, description, domain FROM skills
         WHERE maturity = 'published'
           AND (name ILIKE '%' || $1 || '%'
                OR slug ILIKE '%' || $1 || '%'
                OR description ILIKE '%' || $1 || '%'
                OR domain ILIKE '%' || $1 || '%')
         ORDER BY (name ILIKE '%' || $1 || '%') DESC, slug
         LIMIT 10",
    )
    .bind(query)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(json!({
        "skills": rows.iter().map(|r| json!({
            "slug": r.get::<String, _>("slug"),
            "name": r.get::<String, _>("name"),
            "description": r.get::<Option<String>, _>("description"),
            "domain": r.get::<Option<String>, _>("domain"),
        })).collect::<Vec<_>>()
    }))
}

/// Download a skill's current PUBLISHED bundle. A draft nobody signed returns
/// not-found — the same refusal doc_get makes for unsigned pages.
async fn skill_fetch(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let slug = within_cap(required_str(args, "slug")?, MAX_NAME_CHARS, "slug")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;

    let Some(skill) = brainiac_store::library::get_skill_by_slug(&mut tx, slug).await? else {
        return Ok(json!({ "found": false, "slug": slug }));
    };
    let version = brainiac_store::library::current_published_version(&mut tx, skill.id).await?;
    tx.commit().await?;

    let Some(v) = version else {
        return Ok(json!({
            "found": true,
            "slug": skill.slug,
            "published": false,
            "note": "this skill has no published version yet — a maintainer has not signed one."
        }));
    };

    record_library_usage_quietly(
        state,
        brainiac_core::LibraryArtifactKind::Skill,
        skill.id,
        Some(&v.semver),
        brainiac_core::LibraryUsageEvent::Fetch,
    )
    .await;

    Ok(json!({
        "found": true,
        "slug": skill.slug,
        "name": skill.name,
        "published": true,
        "semver": v.semver,
        "manifest": v.manifest,
        "content_md": v.content_md,
        "resources": v.resources,
    }))
}

/// Report a check (against a standard) or an apply (of a skill). `fetch` is
/// recorded server-side when content is served, so an agent cannot inflate it.
async fn skill_report_usage(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let kind_str = required_str(args, "artifact_kind")?;
    let kind = brainiac_core::LibraryArtifactKind::parse(kind_str)
        .ok_or_else(|| invalid("`artifact_kind` must be `standard` or `skill`"))?;
    let slug = within_cap(required_str(args, "slug")?, MAX_NAME_CHARS, "slug")?;
    let event =
        match required_str(args, "event")? {
            "check" => brainiac_core::LibraryUsageEvent::Check,
            "apply" => brainiac_core::LibraryUsageEvent::Apply,
            _ => return Err(invalid(
                "`event` must be `check` or `apply` — fetches are counted when content is served",
            )),
        };

    let mut tx = state.store.scoped_tx(&state.principal).await?;
    let artifact_id = match kind {
        brainiac_core::LibraryArtifactKind::Standard => {
            brainiac_store::library::get_standard_by_slug(&mut tx, slug)
                .await?
                .map(|s| s.id)
        }
        brainiac_core::LibraryArtifactKind::Skill => {
            brainiac_store::library::get_skill_by_slug(&mut tx, slug)
                .await?
                .map(|s| s.id)
        }
    };
    let Some(artifact_id) = artifact_id else {
        return Ok(json!({ "recorded": false, "reason": "not found", "slug": slug }));
    };
    brainiac_store::library::record_usage(
        &mut tx,
        state.principal.org_id,
        kind,
        artifact_id,
        None,
        event,
        state.principal.team_ids.first().copied(),
    )
    .await?;
    tx.commit().await?;

    Ok(json!({ "recorded": true, "counted_for": "your team — never you personally" }))
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

/// The evidence chain behind a memory (ARCHITECTURE.md §2.2): actor, model,
/// time, originating source (bounded excerpt), and the canonical entities it
/// anchors — everything an agent needs to cite or attribute a served memory.
async fn memory_provenance(state: &McpState, args: &Value) -> Result<Value, ToolError> {
    let memory_id = required_uuid(args, "memory_id")?;
    let mut tx = state.store.scoped_tx(&state.principal).await?;

    // RLS gate: a memory invisible to the caller resolves to None — the SAME
    // "not found" as a nonexistent id, so this tool is no existence oracle.
    let Some(view) = brainiac_store::governance::provenance_for_memory(&mut tx, memory_id).await?
    else {
        return Err(rejected("memory not found"));
    };

    // Canonical entities anchoring the memory — reuse the batched helper.
    let anchors = brainiac_store::entities::canonical_anchors_for(&mut tx, &[memory_id]).await?;
    let entity_anchors = anchors
        .get(&memory_id)
        .map(|a| {
            a.iter()
                .map(|e| json!({ "id": e.id, "name": e.name }))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Bound the source excerpt to a documented cap (char-boundary safe): a
    // citation handle, never the whole transcript. REDACT first (H4): this window
    // is verbatim raw-session text served to any RLS-admitted agent, and the cap
    // is a length limit, not a secret control — a credential in the first 500
    // chars would otherwise be disclosed in full. Redact before truncating so a
    // secret straddling the cut is still masked.
    let source = view.source_kind.as_ref().map(|kind| {
        let excerpt = view.source_text.as_deref().map(|text| {
            let redacted = brainiac_core::redact::redact(text.trim());
            let excerpt: String = redacted.chars().take(SOURCE_EXCERPT_CHARS).collect();
            if redacted.chars().count() > SOURCE_EXCERPT_CHARS {
                format!("{excerpt}…")
            } else {
                excerpt
            }
        });
        json!({ "kind": kind, "excerpt": excerpt })
    });

    Ok(json!({
        "memory_id": memory_id,
        "actor_kind": view.actor_kind,
        "actor_ref": view.actor_ref,
        "model_ref": view.model_ref,
        // WHO decided: the human whose session this came from, distinct from the
        // agent/model that recorded it. Null when the source had no human author.
        "recorded_by": view.recorded_by,
        // WHEN the pipeline recorded it (not necessarily when it was decided).
        "created_at": view.created_at,
        // IS IT STILL TRUE: the memory's validity window + governance status.
        // valid_to == null means still in force.
        "valid_from": view.valid_from,
        "valid_to": view.valid_to,
        "still_valid": view.valid_to.is_none(),
        "status": view.status,
        "source": source,
        "entity_anchors": entity_anchors,
    }))
}
