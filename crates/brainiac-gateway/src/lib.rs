//! brainiac-gateway — BYOM: one trait, swappable providers.
//!
//! v0 providers:
//! - [`QwenProvider`]: Alibaba DashScope, OpenAI-compatible chat endpoint.
//!   The org brings its own key; the gateway records usage per call.
//! - [`MockProvider`]: deterministic, injectable responder — pipeline tests
//!   exercise plumbing (parsing, writes, provenance, governance) without a
//!   network or model variance. Extraction QUALITY is measured separately
//!   against real providers (EVAL.md pipeline profile, nightly).

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
    /// Ask the provider for a strict-JSON response when supported.
    pub json_mode: bool,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub text: String,
    pub model_ref: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn model_ref(&self) -> String;
    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse>;
}

// ── Qwen via DashScope (OpenAI-compatible mode) ─────────────────────────

pub struct QwenProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl QwenProvider {
    pub const DEFAULT_BASE: &'static str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";
    pub const DEFAULT_MODEL: &'static str = "qwen-max";

    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.unwrap_or_else(|| Self::DEFAULT_BASE.to_string()),
            api_key,
            model: model.unwrap_or_else(|| Self::DEFAULT_MODEL.to_string()),
        }
    }

    /// Construct from environment (DASHSCOPE_API_KEY, QWEN_MODEL, QWEN_BASE_URL).
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("DASHSCOPE_API_KEY").ok()?;
        Some(Self::new(
            key,
            std::env::var("QWEN_MODEL").ok(),
            std::env::var("QWEN_BASE_URL").ok(),
        ))
    }
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}
#[derive(Deserialize)]
struct OpenAiMessage {
    content: String,
}
#[derive(Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}
#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[async_trait]
impl ChatProvider for QwenProvider {
    fn model_ref(&self) -> String {
        format!("qwen:{}", self.model)
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user}
            ],
            "max_tokens": req.max_tokens,
        });
        if req.json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("dashscope request")?;
        let status = resp.status();
        let text = resp.text().await.context("dashscope body")?;
        if !status.is_success() {
            anyhow::bail!(
                "dashscope {status}: {}",
                text.chars().take(400).collect::<String>()
            );
        }
        let parsed: OpenAiResponse = serde_json::from_str(&text).context("dashscope parse")?;
        let content = parsed
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        let usage = parsed.usage.unwrap_or(OpenAiUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        });
        Ok(ChatResponse {
            text: content,
            model_ref: self.model_ref(),
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        })
    }
}

// ── Mock provider (tests / offline) ─────────────────────────────────────

type Responder = dyn Fn(&ChatRequest) -> String + Send + Sync;

pub struct MockProvider {
    responder: Box<Responder>,
}

impl MockProvider {
    pub fn new(responder: impl Fn(&ChatRequest) -> String + Send + Sync + 'static) -> Self {
        Self {
            responder: Box::new(responder),
        }
    }
}

#[async_trait]
impl ChatProvider for MockProvider {
    fn model_ref(&self) -> String {
        "mock:deterministic".into()
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let text = (self.responder)(req);
        Ok(ChatResponse {
            text,
            model_ref: self.model_ref(),
            input_tokens: (req.system.len() + req.user.len()) as u64 / 4,
            output_tokens: 64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_round_trips() {
        let p = MockProvider::new(|req| format!("echo:{}", req.user));
        let resp = p
            .complete(&ChatRequest {
                system: "s".into(),
                user: "hello".into(),
                json_mode: false,
                max_tokens: 10,
            })
            .await
            .expect("mock");
        assert_eq!(resp.text, "echo:hello");
        assert_eq!(resp.model_ref, "mock:deterministic");
    }

    #[test]
    fn qwen_provider_constructs_with_defaults() {
        let p = QwenProvider::new("key".into(), None, None);
        assert_eq!(p.model_ref(), "qwen:qwen-max");
    }
}
