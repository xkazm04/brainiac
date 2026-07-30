//! Anthropic Messages API (`POST {base}/v1/messages`).
//!
//! Bespoke wire shape (not OpenAI-compatible): `system` is a top-level field, the
//! key rides an `x-api-key` header with a pinned `anthropic-version`, `max_tokens`
//! is required, and usage is `{input_tokens, output_tokens}`. The request-body
//! builder and response parser are shared with the Bedrock provider, which invokes
//! the same Anthropic model family through AWS's SigV4-signed envelope.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{build_http_client, first_env, meter};
use crate::resilience::Resilience;
use crate::{ChatProvider, ChatRequest, ChatResponse};

pub struct AnthropicProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    version: String,
    resilience: Resilience,
}

impl AnthropicProvider {
    pub const DEFAULT_BASE: &'static str = "https://api.anthropic.com";
    pub const DEFAULT_MODEL: &'static str = "claude-sonnet-4-5";
    pub const DEFAULT_VERSION: &'static str = "2023-06-01";

    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            http: build_http_client(),
            base_url: base_url
                .unwrap_or_else(|| Self::DEFAULT_BASE.to_string())
                .trim_end_matches('/')
                .to_string(),
            api_key,
            model: model.unwrap_or_else(|| Self::DEFAULT_MODEL.to_string()),
            version: std::env::var("ANTHROPIC_VERSION")
                .unwrap_or_else(|_| Self::DEFAULT_VERSION.to_string()),
            resilience: Resilience::from_env(),
        }
    }

    /// From env: `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL`, `ANTHROPIC_BASE_URL`.
    pub fn from_env(model: Option<String>) -> Result<Self> {
        let key = first_env(&["ANTHROPIC_API_KEY"])
            .context("provider `anthropic` needs ANTHROPIC_API_KEY")?;
        Ok(Self::new(
            key,
            model.or_else(|| std::env::var("ANTHROPIC_MODEL").ok()),
            std::env::var("ANTHROPIC_BASE_URL").ok(),
        ))
    }
}

/// The Messages request body. `model` is `Some` for the native API (where the id
/// is a body field) and `None` for Bedrock (where the id is in the URL and the
/// body instead carries `anthropic_version`). Anthropic has no `response_format`
/// JSON mode, so `json_mode` appends a strict-JSON instruction to the system
/// prompt — the pipeline's extraction prompts already ask for JSON, so this only
/// hardens what they request.
pub(crate) fn messages_body(
    model: Option<&str>,
    anthropic_version: Option<&str>,
    req: &ChatRequest,
) -> Value {
    let system = if req.json_mode {
        format!(
            "{}\n\nRespond with a single valid JSON object and nothing else.",
            req.system
        )
    } else {
        req.system.clone()
    };
    let mut body = json!({
        "max_tokens": req.max_tokens,
        "system": system,
        "messages": [{"role": "user", "content": req.user}],
        "temperature": req.temperature,
    });
    if let Some(m) = model {
        body["model"] = json!(m);
    }
    if let Some(v) = anthropic_version {
        body["anthropic_version"] = json!(v);
    }
    body
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}
#[derive(Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}
#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

/// Parse a Messages response into `(text, input_tokens, output_tokens)`. Shared
/// by the native and Bedrock providers. Empty `content` is treated as "no answer"
/// (an error), mirroring the OpenAI-compat provider's empty-`choices` handling.
pub(crate) fn parse_messages_response(text: &str, what: &str) -> Result<(String, u64, u64)> {
    let parsed: AnthropicResponse =
        serde_json::from_str(text).with_context(|| format!("{what} parse"))?;
    let joined: String = parsed
        .content
        .iter()
        .filter(|b| b.kind == "text")
        .map(|b| b.text.as_str())
        .collect();
    if joined.is_empty() {
        anyhow::bail!("{what} returned 200 with no text content");
    }
    let usage = parsed.usage.unwrap_or_else(|| {
        tracing::warn!("{what} response omitted usage — recording 0 tokens for a billed call");
        AnthropicUsage {
            input_tokens: 0,
            output_tokens: 0,
        }
    });
    Ok((joined, usage.input_tokens, usage.output_tokens))
}

#[async_trait]
impl ChatProvider for AnthropicProvider {
    fn model_ref(&self) -> String {
        format!("anthropic:{}", self.model)
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        meter("anthropic", &self.model, async {
            let body = messages_body(Some(&self.model), None, req);
            let http_req = self
                .http
                .post(format!("{}/v1/messages", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", &self.version)
                .json(&body);
            let text = self.resilience.send(http_req, "anthropic").await?;
            let (content, input_tokens, output_tokens) =
                parse_messages_response(&text, "anthropic")?;
            Ok(ChatResponse {
                text: content,
                model_ref: self.model_ref(),
                input_tokens,
                output_tokens,
            })
        })
        .await
    }
}
