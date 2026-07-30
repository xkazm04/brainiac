//! Bedrock provider end-to-end against a wiremock upstream: proves a SigV4-signed
//! `Authorization` header actually reaches the wire, the Claude-on-Bedrock body
//! carries `anthropic_version` (not a `model` field), and the Anthropic response
//! parser runs. It reads AWS creds + `BEDROCK_BASE_URL` from process env, so it is
//! its own binary. (The SigV4 crypto itself is proven against AWS's published
//! signing-key vector in a unit test in `src/providers/bedrock.rs`.)

use std::sync::Once;

use brainiac_gateway::{BedrockProvider, ChatProvider, ChatRequest};
use serde_json::json;
use wiremock::matchers::{body_string_contains, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn env(base_url: &str) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("BRAINIAC_GATEWAY_RETRIES", "2");
        std::env::set_var("BRAINIAC_GATEWAY_BASE_DELAY_MS", "1");
        std::env::set_var("AWS_ACCESS_KEY_ID", "AKIDEXAMPLE");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "secret-example");
        std::env::set_var("AWS_REGION", "us-east-1");
        std::env::set_var("BEDROCK_MODEL", "anthropic.test-model");
    });
    std::env::set_var("BEDROCK_BASE_URL", base_url);
}

#[tokio::test]
async fn bedrock_signs_and_invokes_then_parses_anthropic_body() {
    let server = MockServer::start().await;
    env(&server.uri());

    Mock::given(method("POST"))
        .and(path("/model/anthropic.test-model/invoke"))
        // The signed request must carry an AWS SigV4 Authorization + x-amz-date.
        .and(header_exists("authorization"))
        .and(header_exists("x-amz-date"))
        // Bedrock body uses anthropic_version in place of a model field.
        .and(body_string_contains("bedrock-2023-05-31"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "bedrock says hi"}],
            "usage": {"input_tokens": 8, "output_tokens": 4}
        })))
        .mount(&server)
        .await;

    let p = BedrockProvider::from_env(None).expect("bedrock from env");
    let resp = p
        .complete(&ChatRequest {
            system: "s".into(),
            user: "u".into(),
            json_mode: false,
            max_tokens: 64,
            temperature: 0.0,
        })
        .await
        .expect("bedrock ok");
    assert_eq!(resp.text, "bedrock says hi");
    assert_eq!(resp.model_ref, "bedrock:anthropic.test-model");
    assert_eq!(resp.input_tokens, 8);
    assert_eq!(resp.output_tokens, 4);
}
