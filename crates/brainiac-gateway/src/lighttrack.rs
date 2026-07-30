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
//!   - `BRAINIAC_ENV`       environment tag stamped on every event; unset ⇒ `"local"`
//!     (the same variable and default Sentry uses in `brainiac-server`'s `main.rs`).
//!     Without it, local and production traffic that share one LightTrack instance
//!     are indistinguishable and dev calls pollute production cost/quality rollups.
//!
//! Design: a single background OS thread drains a channel and POSTs each event
//! with a 2s timeout. Every failure is swallowed — telemetry must never break or
//! slow the host app. Disabled (URL unset) ⇒ [`record`] returns immediately.

use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

/// The live channel sender + the values resolved once at init: project id and
/// environment tag. `None` ⇒ disabled.
struct Client {
    tx: Sender<Value>,
    project: Option<String>,
    /// `BRAINIAC_ENV`, resolved once — never re-read per event.
    environment: String,
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
    let key = std::env::var("LIGHTTRACK_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    let project = std::env::var("LIGHTTRACK_PROJECT")
        .ok()
        .filter(|s| !s.is_empty());
    let environment = environment();

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
    Some(Client {
        tx,
        project,
        environment,
    })
}

/// The deployment tag for every event, matching `init_sentry()` in
/// `brainiac-server`: `BRAINIAC_ENV`, or `"local"` when unset. Blank/whitespace
/// also degrades to `"local"` so a half-filled `.env` can't emit an untagged
/// event that silently joins the production rollup.
fn environment() -> String {
    std::env::var("BRAINIAC_ENV")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".into())
}

/// Record one completed LLM call (best-effort, non-blocking). A no-op when the
/// client is disabled. `name` is the stage/use-case rollup key when reachable.
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
    let ev = event(
        provider,
        model,
        name,
        input_tokens,
        output_tokens,
        latency_ms,
        ok,
        client.project.as_deref(),
        &client.environment,
    );
    let _ = client.tx.send(ev);
}

/// Build one event body. Pure (no env reads, no I/O) so the wire shape is
/// unit-testable.
///
/// Mirrors `LlmEvent`: `id`/`ts`/`project_id` all have serde defaults on the
/// server, so we omit `id`/`ts` and send `project_id` only when known.
/// `environment` is always present — an untagged event is exactly the ambiguity
/// this field exists to remove.
#[allow(clippy::too_many_arguments)] // one wire record, assembled in one place
fn event(
    provider: &str,
    model: &str,
    name: Option<&str>,
    input_tokens: u64,
    output_tokens: u64,
    latency_ms: u64,
    ok: bool,
    project: Option<&str>,
    environment: &str,
) -> Value {
    let mut ev = json!({
        "provider": provider,
        "model": model,
        "operation": "chat",
        "usage": { "input": input_tokens, "output": output_tokens },
        "latency_ms": latency_ms,
        "status": if ok { "success" } else { "error" },
        "source": "brainiac-gateway",
        "environment": environment,
    });
    if let Some(n) = name {
        ev["name"] = Value::String(n.to_string());
    }
    if let Some(p) = project {
        ev["project_id"] = Value::String(p.to_string());
    }
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_carries_the_environment_tag() {
        let ev = event(
            "qwen",
            "qwen-plus",
            None,
            10,
            20,
            42,
            true,
            None,
            "production",
        );
        assert_eq!(ev["environment"], "production");
        assert_eq!(ev["provider"], "qwen");
        assert_eq!(ev["model"], "qwen-plus");
        assert_eq!(ev["usage"]["input"], 10);
        assert_eq!(ev["usage"]["output"], 20);
        assert_eq!(ev["latency_ms"], 42);
        assert_eq!(ev["status"], "success");
        assert_eq!(ev["source"], "brainiac-gateway");
        // Absent optionals stay absent — the server defaults them.
        assert!(ev.get("name").is_none());
        assert!(ev.get("project_id").is_none());
    }

    #[test]
    fn failed_call_meters_zero_tokens_with_error_status() {
        let ev = event(
            "openai",
            "gpt-4o-mini",
            Some("extract"),
            0,
            0,
            7,
            false,
            Some("brainiac"),
            "local",
        );
        assert_eq!(ev["status"], "error");
        assert_eq!(ev["usage"]["input"], 0);
        assert_eq!(ev["name"], "extract");
        assert_eq!(ev["project_id"], "brainiac");
        assert_eq!(ev["environment"], "local");
    }
}
