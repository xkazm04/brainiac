//! QwenProvider ↔ DashScope (OpenAI-compatible) HTTP contract, exercised
//! against a wiremock upstream so we assert the *real* error taxonomy the
//! pipeline caller sees — never a network or a live model.
//!
//! What the caller (`QwenProvider::complete`) actually does, per src/lib.rs:
//!   - 2xx JSON with a non-empty `choices[0].message.content` ⇒ `Ok(ChatResponse)`
//!   - 2xx body that isn't the expected JSON ⇒ `Err(_)` carrying "dashscope parse"
//!   - 2xx JSON with empty `choices` ⇒ `Err(_)` "returned 200 with no choices"
//!   - 2xx JSON with no `usage` ⇒ `Ok` but 0 tokens (a billed-but-unmetered call)
//!   - 429 / 5xx ⇒ retried by resilience, then `Err(_)` "...failed after N attempts"
//!
//! Timeout is a transport-level concern gated on a ≥1s wall clock (the http
//! client's minimum), so it lives in its own binary: `qwen_timeout.rs`.

use std::sync::Once;

use brainiac_gateway::{ChatProvider, ChatRequest, QwenProvider};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Deterministic + fast: 2 attempts with a 1ms base backoff. Set once for the
/// whole test binary (a separate process from every other test file), so the
/// value is constant and the parallel writes are race-free.
fn fast_resilient_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("BRAINIAC_GATEWAY_RETRIES", "2");
        std::env::set_var("BRAINIAC_GATEWAY_BASE_DELAY_MS", "1");
    });
}

fn provider(base_url: &str) -> QwenProvider {
    fast_resilient_env();
    QwenProvider::new("test-key".into(), None, Some(base_url.into()))
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

async fn chat_mock(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(template)
        .mount(server)
        .await;
}

#[tokio::test]
async fn happy_path_parses_content_model_ref_and_usage() {
    let server = MockServer::start().await;
    chat_mock(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "the answer is 42"}}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 5}
        })),
    )
    .await;

    let resp = provider(&server.uri())
        .complete(&req())
        .await
        .expect("2xx with content parses to Ok");

    assert_eq!(resp.text, "the answer is 42");
    assert_eq!(resp.model_ref, "qwen:qwen-max"); // DEFAULT_MODEL
    assert_eq!(resp.input_tokens, 11);
    assert_eq!(resp.output_tokens, 5);

    // Exactly one upstream call on the happy path — no spurious retry.
    let hits = server.received_requests().await.expect("recorded");
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn malformed_json_body_surfaces_a_parse_error() {
    let server = MockServer::start().await;
    // 200 OK, but the body is not the OpenAI envelope. resilience.send() returns
    // it as success (status is 2xx); the parse in complete() is what fails.
    chat_mock(
        &server,
        ResponseTemplate::new(200).set_body_string("<html>gateway ate your json</html>"),
    )
    .await;

    let err = provider(&server.uri())
        .complete(&req())
        .await
        .expect_err("malformed 2xx body must be an error, not a hollow success");

    let chain = format!("{err:#}");
    assert!(
        chain.contains("dashscope parse"),
        "caller must see the parse-stage context, got: {chain}"
    );
}

#[tokio::test]
async fn empty_choices_is_no_answer_not_empty_answer() {
    let server = MockServer::start().await;
    chat_mock(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({
            "choices": [],
            "usage": {"prompt_tokens": 3, "completion_tokens": 0}
        })),
    )
    .await;

    let err = provider(&server.uri())
        .complete(&req())
        .await
        .expect_err("200 with no choices must fail so the pipeline retries/surfaces");

    let chain = format!("{err:#}");
    assert!(
        chain.contains("no choices"),
        "expected the no-choices taxonomy, got: {chain}"
    );
}

#[tokio::test]
async fn missing_usage_still_succeeds_but_meters_zero() {
    let server = MockServer::start().await;
    // A billed call whose response omitted `usage`: contract is Ok with 0 tokens
    // (logged as cost-unknown), never a failure.
    chat_mock(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "ok"}}]
        })),
    )
    .await;

    let resp = provider(&server.uri())
        .complete(&req())
        .await
        .expect("missing usage is Ok, not an error");

    assert_eq!(resp.text, "ok");
    assert_eq!(resp.input_tokens, 0);
    assert_eq!(resp.output_tokens, 0);
}

#[tokio::test]
async fn rate_limit_429_is_retried_then_exhausted() {
    let server = MockServer::start().await;
    chat_mock(
        &server,
        ResponseTemplate::new(429).set_body_string("{\"error\":\"Requests rate limit exceeded\"}"),
    )
    .await;

    let err = provider(&server.uri())
        .complete(&req())
        .await
        .expect_err("a persistent 429 must ultimately fail");

    let chain = format!("{err:#}");
    // The caller sees a retryable-exhausted error that names the 429 status and
    // the attempt budget it burned (BRAINIAC_GATEWAY_RETRIES=2 for this binary).
    assert!(
        chain.contains("429"),
        "must name the 429 status, got: {chain}"
    );
    assert!(
        chain.contains("failed after 2 attempts"),
        "must report the exhausted attempt budget, got: {chain}"
    );

    // 429 is retried at the provider boundary: the upstream saw both attempts.
    let hits = server.received_requests().await.expect("recorded");
    assert_eq!(
        hits.len(),
        2,
        "429 must be retried up to the attempt budget"
    );
}

#[tokio::test]
async fn permanent_4xx_is_not_retried() {
    let server = MockServer::start().await;
    // 401 (bad key) is permanent: fail immediately, no retry storm.
    chat_mock(
        &server,
        ResponseTemplate::new(401).set_body_string("{\"error\":\"invalid api key\"}"),
    )
    .await;

    let err = provider(&server.uri())
        .complete(&req())
        .await
        .expect_err("auth failure is permanent");

    let chain = format!("{err:#}");
    assert!(
        chain.contains("401"),
        "must name the 401 status, got: {chain}"
    );

    let hits = server.received_requests().await.expect("recorded");
    assert_eq!(hits.len(), 1, "permanent 4xx must not be retried");
}
