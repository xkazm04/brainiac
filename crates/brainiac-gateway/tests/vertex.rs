//! Vertex provider end-to-end against a wiremock upstream, using the
//! `VERTEX_ACCESS_TOKEN` static-token path (the service-account RS256 mint is
//! exercised implicitly by that code path; this test pins the request/response
//! contract without needing a real Google key). Reads config from process env, so
//! it is its own binary.

use std::sync::Once;

use brainiac_gateway::{ChatProvider, ChatRequest, VertexProvider};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn env(base_url: &str) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("BRAINIAC_GATEWAY_RETRIES", "2");
        std::env::set_var("BRAINIAC_GATEWAY_BASE_DELAY_MS", "1");
        std::env::set_var("GOOGLE_CLOUD_PROJECT", "test-proj");
        std::env::set_var("VERTEX_LOCATION", "us-central1");
        std::env::set_var("VERTEX_MODEL", "gemini-2.5-flash");
        std::env::set_var("VERTEX_ACCESS_TOKEN", "ya29.test-token");
    });
    std::env::set_var("VERTEX_BASE_URL", base_url);
}

#[tokio::test]
async fn vertex_posts_generate_content_with_bearer_and_parses() {
    let server = MockServer::start().await;
    env(&server.uri());

    Mock::given(method("POST"))
        .and(path(
            "/v1/projects/test-proj/locations/us-central1/publishers/google/models/gemini-2.5-flash:generateContent",
        ))
        .and(header("authorization", "Bearer ya29.test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{"content": {"parts": [{"text": "vertex hi"}]}}],
            "usageMetadata": {"promptTokenCount": 6, "candidatesTokenCount": 2}
        })))
        .mount(&server)
        .await;

    let p = VertexProvider::from_env(None).expect("vertex from env");
    let resp = p
        .complete(&ChatRequest {
            system: "s".into(),
            user: "u".into(),
            json_mode: true,
            max_tokens: 64,
            temperature: 0.0,
        })
        .await
        .expect("vertex ok");
    assert_eq!(resp.text, "vertex hi");
    assert_eq!(resp.model_ref, "vertex:gemini-2.5-flash");
    assert_eq!(resp.input_tokens, 6);
    assert_eq!(resp.output_tokens, 2);
}
