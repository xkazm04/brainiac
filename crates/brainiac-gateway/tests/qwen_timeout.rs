//! Timeout taxonomy for QwenProvider, isolated in its own binary because it is
//! the one case gated on a real wall clock: `build_http_client` clamps
//! BRAINIAC_GATEWAY_REQUEST_TIMEOUT_SECS to a ≥1s minimum, so the fastest
//! honest timeout we can drive is ~1s. Everything else lives in
//! `qwen_provider.rs` and is sub-millisecond.
//!
//! A stalled-but-connected upstream must become a retryable transport error
//! (reqwest `is_timeout()`), not an infinite hang — see the doc comment on
//! `build_http_client` in src/lib.rs.

use std::error::Error;
use std::sync::Once;
use std::time::{Duration, Instant};

use brainiac_gateway::{ChatProvider, ChatRequest, QwenProvider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn timeout_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // 1s request timeout, single attempt (a timeout is retryable, so this
        // keeps the test to ~one timeout window rather than N).
        std::env::set_var("BRAINIAC_GATEWAY_REQUEST_TIMEOUT_SECS", "1");
        std::env::set_var("BRAINIAC_GATEWAY_RETRIES", "1");
        std::env::set_var("BRAINIAC_GATEWAY_BASE_DELAY_MS", "1");
    });
}

#[tokio::test]
async fn stalled_upstream_times_out_as_a_retryable_transport_error() {
    timeout_env();
    let server = MockServer::start().await;
    // Connection is accepted immediately, but the response is delayed past the
    // 1s request timeout — the "connected but stalled" failure mode.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
        .mount(&server)
        .await;

    let provider = QwenProvider::new("test-key".into(), None, Some(server.uri()));
    let started = Instant::now();
    let err = provider
        .complete(&ChatRequest {
            system: "s".into(),
            user: "u".into(),
            json_mode: false,
            max_tokens: 16,
            temperature: 0.0,
        })
        .await
        .expect_err("a stalled upstream must error, never hang");

    // Fired on the ~1s timeout, nowhere near the 3s delay.
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "must abort on the timeout, not wait out the full delay (elapsed {elapsed:?})"
    );

    // The taxonomy the caller sees is specifically a reqwest timeout, wrapped in
    // the resilience attempt-budget context.
    let chain = format!("{err:#}");
    assert!(
        chain.contains("failed after 1 attempts"),
        "must be a retryable-exhausted error, got: {chain}"
    );
    let is_timeout = err
        .chain()
        .filter_map(|e| (e as &dyn Error).downcast_ref::<reqwest::Error>())
        .any(reqwest::Error::is_timeout);
    assert!(
        is_timeout,
        "the root cause must be a genuine timeout, got: {chain}"
    );

    // The upstream really was hit (connect succeeded, then stalled) — this is a
    // timeout, not a connection refusal.
    let hits = server.received_requests().await.expect("recorded");
    assert_eq!(hits.len(), 1);
}
