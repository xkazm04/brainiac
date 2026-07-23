//! Minimal fire-and-forget LightTrack client (LLM-call telemetry).
//!
//! Vendored — NOT the upstream SDK — to keep the gateway's dependency surface
//! tiny. It mirrors LightTrack's `LlmEvent` wire shape (LightTrack
//! `crates/core/src/event.rs`) so payloads never drift from the ingest API:
//! `POST {LIGHTTRACK_URL}/v1/events` with an optional `Bearer LIGHTTRACK_KEY`.
//!
//! Contract (matches LightTrack `clients/rust/src/lib.rs`):
//!   - `LIGHTTRACK_URL`     ingest base URL; UNSET ⇒ client disabled entirely (no-op).
//!   - `LIGHTTRACK_KEY`     optional bearer token; when set the server derives the project.
//!   - `LIGHTTRACK_PROJECT` optional project id (dev mode / admin key).
//!
//! Design: a single background OS thread drains a channel and POSTs each event
//! with a 2s timeout. Every failure is swallowed — telemetry must never break or
//! slow the host app. Disabled (URL unset) ⇒ [`record`] returns immediately.

use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

/// The live channel sender + resolved project id. `None` ⇒ disabled.
struct Client {
    tx: Sender<Value>,
    project: Option<String>,
}

static CLIENT: OnceLock<Option<Client>> = OnceLock::new();

fn client() -> Option<&'static Client> {
    CLIENT.get_or_init(init).as_ref()
}

/// Build the client from the environment once. Returns `None` (permanently
/// disabled) when `LIGHTTRACK_URL` is unset/blank — the production posture.
fn init() -> Option<Client> {
    let base = std::env::var("LIGHTTRACK_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())?;
    let key = std::env::var("LIGHTTRACK_KEY").ok().filter(|s| !s.is_empty());
    let project = std::env::var("LIGHTTRACK_PROJECT")
        .ok()
        .filter(|s| !s.is_empty());

    let (tx, rx) = mpsc::channel::<Value>();
    // A dedicated thread with a blocking HTTP client: independent of any tokio
    // runtime, draining until every `Sender` drops. If the thread can't spawn we
    // simply have no telemetry — never a reason to fail the host.
    let spawned = std::thread::Builder::new()
        .name("lighttrack".into())
        .spawn(move || {
            let http = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            let endpoint = format!("{base}/v1/events");
            while let Ok(body) = rx.recv() {
                let mut req = http.post(&endpoint).json(&body);
                if let Some(k) = &key {
                    req = req.bearer_auth(k);
                }
                let _ = req.send(); // best-effort: telemetry must never break the host
            }
        })
        .is_ok();
    if !spawned {
        return None;
    }
    Some(Client { tx, project })
}

/// Record one completed LLM call (best-effort, non-blocking). A no-op when the
/// client is disabled. `name` is the stage/use-case rollup key when reachable.
///
/// Mirrors `LlmEvent`: `id`/`ts`/`project_id` all have serde defaults on the
/// server, so we omit `id`/`ts` and send `project_id` only when known.
pub fn record(
    provider: &str,
    model: &str,
    name: Option<&str>,
    input_tokens: u64,
    output_tokens: u64,
    latency_ms: u64,
    ok: bool,
) {
    let Some(client) = client() else { return };
    let mut ev = json!({
        "provider": provider,
        "model": model,
        "operation": "chat",
        "usage": { "input": input_tokens, "output": output_tokens },
        "latency_ms": latency_ms,
        "status": if ok { "success" } else { "error" },
        "source": "brainiac-gateway",
    });
    if let Some(n) = name {
        ev["name"] = Value::String(n.to_string());
    }
    if let Some(p) = &client.project {
        ev["project_id"] = Value::String(p.clone());
    }
    let _ = client.tx.send(ev);
}
