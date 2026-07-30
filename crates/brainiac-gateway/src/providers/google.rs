//! Google Gemini via the Generative Language API (`…/models/{model}:generateContent`).
//!
//! Bespoke wire shape: `system_instruction`, `contents[].parts[].text`, a
//! `generationConfig` for sampling, and `usageMetadata` for tokens. Auth is an
//! `x-goog-api-key` header. The body builder and response parser are shared with
//! the Vertex provider, which posts the identical body to a regional Vertex AI
//! host under an OAuth bearer token.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{build_http_client, first_env, meter};
use crate::resilience::Resilience;
use crate::{ChatProvider, ChatRequest, ChatResponse};

pub struct GeminiProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    resilience: Resilience,
}

impl GeminiProvider {
    pub const DEFAULT_BASE: &'static str = "https://generativelanguage.googleapis.com";
    pub const DEFAULT_MODEL: &'static str = "gemini-2.5-flash";

    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            http: build_http_client(),
            base_url: base_url
                .unwrap_or_else(|| Self::DEFAULT_BASE.to_string())
                .trim_end_matches('/')
                .to_string(),
            api_key,
            model: model.unwrap_or_else(|| Self::DEFAULT_MODEL.to_string()),
            resilience: Resilience::from_env(),
        }
    }

    /// From env: `GEMINI_API_KEY` → `GOOGLE_API_KEY` → `GOOGLE_GENERATIVE_AI_API_KEY`,
    /// plus `GOOGLE_MODEL`, `GOOGLE_BASE_URL`.
    pub fn from_env(model: Option<String>) -> Result<Self> {
        let key = first_env(&[
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GOOGLE_GENERATIVE_AI_API_KEY",
        ])
        .context("provider `google` needs GEMINI_API_KEY (or GOOGLE_API_KEY)")?;
        Ok(Self::new(
            key,
            model.or_else(|| std::env::var("GOOGLE_MODEL").ok()),
            std::env::var("GOOGLE_BASE_URL").ok(),
        ))
    }
}

/// The `generateContent` request body, shared by the native Gemini and Vertex
/// providers. `json_mode` sets `responseMimeType: application/json`, Gemini's
/// native structured-output switch.
pub(crate) fn generate_content_body(req: &ChatRequest) -> Value {
    let mut generation_config = json!({
        "temperature": req.temperature,
        "maxOutputTokens": req.max_tokens,
    });
    if req.json_mode {
        generation_config["responseMimeType"] = json!("application/json");
    }
    json!({
        "system_instruction": {"parts": [{"text": req.system}]},
        "contents": [{"role": "user", "parts": [{"text": req.user}]}],
        "generationConfig": generation_config,
    })
}

#[derive(Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: String,
}
#[derive(Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}
#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}
#[derive(Deserialize)]
struct GeminiUsage {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: u64,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: u64,
}
#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
}

/// Parse a `generateContent` response into `(text, input_tokens, output_tokens)`.
/// Shared by the native and Vertex providers. A candidate with no text parts
/// (e.g. a safety block) is "no answer" and fails, matching the other providers.
pub(crate) fn parse_generate_content(text: &str, what: &str) -> Result<(String, u64, u64)> {
    let parsed: GeminiResponse =
        serde_json::from_str(text).with_context(|| format!("{what} parse"))?;
    let joined: String = parsed
        .candidates
        .first()
        .and_then(|c| c.content.as_ref())
        .map(|content| {
            content
                .parts
                .iter()
                .map(|p| p.text.as_str())
                .collect::<String>()
        })
        .unwrap_or_default();
    if joined.is_empty() {
        anyhow::bail!("{what} returned 200 with no text content (safety block or empty candidate)");
    }
    let usage = parsed.usage_metadata.unwrap_or_else(|| {
        tracing::warn!(
            "{what} response omitted usageMetadata — recording 0 tokens for a billed call"
        );
        GeminiUsage {
            prompt_token_count: 0,
            candidates_token_count: 0,
        }
    });
    Ok((
        joined,
        usage.prompt_token_count,
        usage.candidates_token_count,
    ))
}

#[async_trait]
impl ChatProvider for GeminiProvider {
    fn model_ref(&self) -> String {
        format!("google:{}", self.model)
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        meter("google", &self.model, async {
            let url = format!(
                "{}/v1beta/models/{}:generateContent",
                self.base_url, self.model
            );
            let http_req = self
                .http
                .post(url)
                .header("x-goog-api-key", &self.api_key)
                .json(&generate_content_body(req));
            let text = self.resilience.send(http_req, "gemini").await?;
            let (content, input_tokens, output_tokens) = parse_generate_content(&text, "gemini")?;
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
