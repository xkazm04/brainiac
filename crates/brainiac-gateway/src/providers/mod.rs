//! Provider spectrum — concrete [`crate::ChatProvider`] / embedder implementations.
//!
//! Every provider builds its own auth'd `reqwest::RequestBuilder` and hands it to
//! the shared [`crate::resilience`] layer (retry + circuit-breaker + timeout),
//! then reports the completed call to [`crate::lighttrack`] through [`meter`].
//! The wire shape differs per provider; the resilience, telemetry, and HTTP-client
//! construction do not — those live here so no provider re-implements them.
//!
//! Families:
//! - [`openai`]  — OpenAI-compatible `/chat/completions` + `/embeddings`. One impl
//!   covers OpenAI, Azure OpenAI, Qwen/DashScope, Groq, Together, DeepSeek,
//!   Mistral, OpenRouter, and local Ollama/vLLM by base-URL + auth-style swap.
//! - [`anthropic`] — Anthropic Messages API (`/v1/messages`).
//! - [`google`]  — Google Gemini `generateContent`.
//! - [`bedrock`] — AWS Bedrock runtime (SigV4-signed; Anthropic-on-Bedrock body).
//! - [`vertex`]  — Google Vertex AI (`generateContent`; service-account OAuth).

use std::future::Future;

use crate::ChatResponse;

pub mod anthropic;
pub mod bedrock;
pub mod google;
pub mod openai;
pub mod vertex;

/// The HTTP client every provider call uses. reqwest applies NO default timeout,
/// so a stalled-but-connected upstream (slow-loris, LB half-close, model overload)
/// would hang the awaiting future forever — producing neither an error nor a
/// status, so the retry loop never fires and the circuit breaker never opens. A
/// connect + total-request timeout turns a stall into a retryable/breaker-
/// recordable error. reqwest's `.timeout()` is per `send()`, so it bounds each
/// resilience attempt while the retry loop still governs total attempts. Tunable
/// via BRAINIAC_GATEWAY_CONNECT_TIMEOUT_SECS / BRAINIAC_GATEWAY_REQUEST_TIMEOUT_SECS.
pub(crate) fn build_http_client() -> reqwest::Client {
    fn secs(key: &str, default: u64) -> std::time::Duration {
        let n = std::env::var(key)
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(default);
        std::time::Duration::from_secs(n)
    }
    reqwest::Client::builder()
        .connect_timeout(secs("BRAINIAC_GATEWAY_CONNECT_TIMEOUT_SECS", 10))
        .timeout(secs("BRAINIAC_GATEWAY_REQUEST_TIMEOUT_SECS", 60))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// First non-empty env var among `keys` (trimmed). Providers use this for the
/// vendor's several accepted key names (e.g. Gemini: `GEMINI_API_KEY` →
/// `GOOGLE_API_KEY` → `GOOGLE_GENERATIVE_AI_API_KEY`).
pub(crate) fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        std::env::var(k)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

/// Single telemetry chokepoint shared by every provider. Times `fut`, then
/// records the completed LLM call to LightTrack (best-effort, disabled unless
/// `LIGHTTRACK_URL` is set). `provider`/`model` are the LightTrack rollup keys,
/// so the price book downstream can attribute cost — that is why each provider
/// passes its OWN vendor tag here (`openai`/`anthropic`/`google`/`bedrock`/
/// `vertex`/`qwen`), never a generic label. A failed call meters as 0 tokens
/// with `status=error`, preserving the "cost unknown ≠ cost zero" contract.
pub(crate) async fn meter<F>(provider: &str, model: &str, fut: F) -> anyhow::Result<ChatResponse>
where
    F: Future<Output = anyhow::Result<ChatResponse>>,
{
    let started = std::time::Instant::now();
    let result = fut.await;
    let latency_ms = started.elapsed().as_millis() as u64;
    let (input_tokens, output_tokens, ok) = match &result {
        Ok(r) => (r.input_tokens, r.output_tokens, true),
        Err(_) => (0, 0, false),
    };
    crate::lighttrack::record(
        provider,
        model,
        None,
        input_tokens,
        output_tokens,
        latency_ms,
        ok,
    );
    result
}
