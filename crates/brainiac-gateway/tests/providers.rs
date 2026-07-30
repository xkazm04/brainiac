//! Wire-contract tests for the non-Qwen chat providers, exercised against a
//! wiremock upstream so we assert the real request shape each vendor gets and the
//! real error taxonomy the pipeline caller sees — never a network or a live model.
//!
//! These providers take their base URL by constructor (no process env), so they
//! share one test binary. Bedrock (AWS env + SigV4) and Vertex (SA/token env) read
//! the environment and live in their own binaries.

use std::sync::Once;

use brainiac_gateway::{
    AnthropicProvider, ChatProvider, ChatRequest, GeminiProvider, OpenAiCompatProvider,
};
use serde_json::json;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Deterministic + fast retries for the whole binary (a separate process), so the
/// error-path tests don't sleep out real backoff.
fn fast_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("BRAINIAC_GATEWAY_RETRIES", "2");
        std::env::set_var("BRAINIAC_GATEWAY_BASE_DELAY_MS", "1");
    });
}

fn req() -> ChatRequest {
    ChatRequest {
        system: "you extract memories".into(),
        user: "the sky is blue".into(),
        json_mode: true,
        max_tokens: 256,
        temperature: 0.0,
    }
}

// ── Anthropic (Messages API) ─────────────────────────────────────────────────

#[tokio::test]
async fn anthropic_happy_path_parses_text_blocks_and_usage() {
    fast_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "test-key"))
        .and(header("anthropic-version", "2023-06-01"))
        // json_mode must harden the system prompt with a strict-JSON instruction.
        .and(body_string_contains("single valid JSON object"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [
                {"type": "text", "text": "the answer "},
                {"type": "text", "text": "is 42"}
            ],
            "usage": {"input_tokens": 11, "output_tokens": 5}
        })))
        .mount(&server)
        .await;

    let p = AnthropicProvider::new("test-key".into(), None, Some(server.uri()));
    let resp = p.complete(&req()).await.expect("anthropic ok");
    assert_eq!(resp.text, "the answer is 42"); // text blocks concatenated
    assert_eq!(resp.model_ref, "anthropic:claude-sonnet-4-5");
    assert_eq!(resp.input_tokens, 11);
    assert_eq!(resp.output_tokens, 5);
}

#[tokio::test]
async fn anthropic_empty_content_is_no_answer() {
    fast_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [],
            "usage": {"input_tokens": 3, "output_tokens": 0}
        })))
        .mount(&server)
        .await;

    let p = AnthropicProvider::new("k".into(), None, Some(server.uri()));
    let err = p.complete(&req()).await.expect_err("no text content");
    assert!(
        format!("{err:#}").contains("no text content"),
        "got: {err:#}"
    );
}

#[tokio::test]
async fn anthropic_5xx_is_retried_then_exhausted() {
    fast_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_string("overloaded"))
        .mount(&server)
        .await;

    let p = AnthropicProvider::new("k".into(), None, Some(server.uri()));
    let err = p.complete(&req()).await.expect_err("persistent 5xx");
    let chain = format!("{err:#}");
    assert!(chain.contains("529"), "names the status, got: {chain}");
    assert!(chain.contains("failed after 2 attempts"), "got: {chain}");
}

// ── Google Gemini (generateContent) ──────────────────────────────────────────

#[tokio::test]
async fn gemini_happy_path_parses_parts_and_usage_metadata() {
    fast_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .and(header("x-goog-api-key", "test-key"))
        // json_mode must request the JSON response MIME type.
        .and(body_string_contains("application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{"content": {"parts": [{"text": "hello "}, {"text": "world"}]}}],
            "usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 2}
        })))
        .mount(&server)
        .await;

    let p = GeminiProvider::new("test-key".into(), None, Some(server.uri()));
    let resp = p.complete(&req()).await.expect("gemini ok");
    assert_eq!(resp.text, "hello world");
    assert_eq!(resp.model_ref, "google:gemini-2.5-flash");
    assert_eq!(resp.input_tokens, 9);
    assert_eq!(resp.output_tokens, 2);
}

#[tokio::test]
async fn gemini_safety_blocked_candidate_is_no_answer() {
    fast_env();
    let server = MockServer::start().await;
    // A safety block returns a candidate with no content/parts.
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{"finishReason": "SAFETY"}]
        })))
        .mount(&server)
        .await;

    let p = GeminiProvider::new("k".into(), None, Some(server.uri()));
    let err = p.complete(&req()).await.expect_err("safety block");
    assert!(
        format!("{err:#}").contains("no text content"),
        "got: {err:#}"
    );
}

// ── Generic OpenAI-compatible (OpenAI / Groq / Together / …) ──────────────────

#[tokio::test]
async fn openai_compat_happy_path() {
    fast_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 7, "completion_tokens": 3}
        })))
        .mount(&server)
        .await;

    let p = OpenAiCompatProvider::openai("test-key".into(), None, Some(server.uri()), "openai");
    let resp = p.complete(&req()).await.expect("openai ok");
    assert_eq!(resp.text, "ok");
    assert_eq!(resp.model_ref, "openai:gpt-4o-mini");
    assert_eq!(resp.input_tokens, 7);
    assert_eq!(resp.output_tokens, 3);
}
