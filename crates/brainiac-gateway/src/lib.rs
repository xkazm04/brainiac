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

pub mod resilience;

use resilience::Resilience;

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
    /// Ask the provider for a strict-JSON response when supported.
    pub json_mode: bool,
    pub max_tokens: u32,
    /// Sampling temperature. The pipeline's calls are all structured
    /// extraction/classification, not generation, so they pass 0.0 — high
    /// temperature was the dominant source of run-to-run variance AND of the
    /// malformed JSON that failed extraction (UAT extraction eval, 2026-07-14:
    /// the same transcript yielded 4–18 memories across runs at the API
    /// default). 0.0 makes the pipeline near-deterministic and schema-faithful.
    pub temperature: f32,
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
    resilience: Resilience,
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
            resilience: Resilience::from_env(),
        }
    }

    /// Construct from environment (DASHSCOPE_API_KEY or QWEN_API_KEY,
    /// QWEN_MODEL, QWEN_BASE_URL).
    pub fn from_env() -> Option<Self> {
        Some(Self::new(
            dashscope_key_from_env()?,
            std::env::var("QWEN_MODEL").ok(),
            std::env::var("QWEN_BASE_URL").ok(),
        ))
    }
}

fn dashscope_key_from_env() -> Option<String> {
    std::env::var("DASHSCOPE_API_KEY")
        .or_else(|_| std::env::var("QWEN_API_KEY"))
        .ok()
        .filter(|k| !k.trim().is_empty())
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
            "temperature": req.temperature,
        });
        if req.json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }
        let req = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body);
        let text = self.resilience.send(req, "dashscope").await?;
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

// ── Qwen embeddings via DashScope (OpenAI-compatible /embeddings) ───────

/// DashScope `text-embedding-v4` (served Qwen3-Embedding) behind the
/// [`brainiac_core::embed::Embedder`] seam. Matryoshka dims 64–2048; we pin
/// 1024 (the model default) unless overridden. DashScope caps this model at
/// 10 texts per request, so `embed_batch` chunks accordingly.
///
/// The API key is an env/vault reference only — never persisted to Postgres.
pub struct QwenEmbedder {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    dim: usize,
    resilience: Resilience,
}

impl QwenEmbedder {
    pub const DEFAULT_MODEL: &'static str = "text-embedding-v4";
    pub const DEFAULT_DIM: usize = 1024;
    const MAX_BATCH: usize = 10;

    pub fn new(api_key: String, base_url: Option<String>, dim: Option<usize>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.unwrap_or_else(|| QwenProvider::DEFAULT_BASE.to_string()),
            api_key,
            model: Self::DEFAULT_MODEL.to_string(),
            dim: dim.unwrap_or(Self::DEFAULT_DIM),
            resilience: Resilience::from_env(),
        }
    }

    /// Construct from environment (DASHSCOPE_API_KEY or QWEN_API_KEY,
    /// QWEN_BASE_URL, QWEN_EMBED_DIM).
    pub fn from_env() -> Option<Self> {
        Some(Self::new(
            dashscope_key_from_env()?,
            std::env::var("QWEN_BASE_URL").ok(),
            std::env::var("QWEN_EMBED_DIM")
                .ok()
                .and_then(|d| d.parse().ok()),
        ))
    }

    async fn request(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        #[derive(Deserialize)]
        struct EmbeddingRow {
            index: usize,
            embedding: Vec<f32>,
        }
        #[derive(Deserialize)]
        struct EmbeddingResponse {
            data: Vec<EmbeddingRow>,
        }
        let req = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "input": texts,
                "dimensions": self.dim,
                "encoding_format": "float",
            }));
        let body = self.resilience.send(req, "dashscope embeddings").await?;
        let parsed: EmbeddingResponse =
            serde_json::from_str(&body).context("dashscope embeddings parse")?;
        anyhow::ensure!(
            parsed.data.len() == texts.len(),
            "dashscope returned {} embeddings for {} inputs",
            parsed.data.len(),
            texts.len()
        );
        let mut rows = parsed.data;
        rows.sort_by_key(|r| r.index);
        for r in &rows {
            anyhow::ensure!(
                r.embedding.len() == self.dim,
                "dashscope returned dim {} (expected {})",
                r.embedding.len(),
                self.dim
            );
        }
        Ok(rows.into_iter().map(|r| r.embedding).collect())
    }
}

#[async_trait]
impl brainiac_core::embed::Embedder for QwenEmbedder {
    fn model_name(&self) -> &str {
        "qwen:text-embedding-v4"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // DashScope rejects empty input — degrade to a zero vector, matching
        // the deterministic embedder's behavior for blank text.
        if text.trim().is_empty() {
            return Ok(vec![0.0; self.dim]);
        }
        Ok(self
            .request(&[text])
            .await?
            .into_iter()
            .next()
            .expect("length checked"))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(Self::MAX_BATCH) {
            // Blank inputs are filtered per-chunk and re-inserted as zeros.
            let live: Vec<&str> = chunk
                .iter()
                .copied()
                .filter(|t| !t.trim().is_empty())
                .collect();
            let mut embedded = if live.is_empty() {
                Vec::new()
            } else {
                self.request(&live).await?
            }
            .into_iter();
            for t in chunk {
                if t.trim().is_empty() {
                    out.push(vec![0.0; self.dim]);
                } else {
                    out.push(embedded.next().expect("length checked"));
                }
            }
        }
        Ok(out)
    }
}

// ── provider registry & per-stage routing ───────────────────────────────

/// Pipeline stages that consume a chat provider. Extraction wants the
/// strongest model; resolution/contradiction adjudication are short
/// classification calls where a cheaper model is usually enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Extract,
    Resolve,
    Contradict,
    /// Document composition (§8): writes page prose from memories it must cite.
    /// Wants a strong writer — a weak one paraphrases past the evidence, and
    /// every unbacked sentence is a hallucination the eval will fail on.
    Compose,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Extract => "extract",
            Stage::Resolve => "resolve",
            Stage::Contradict => "contradict",
            Stage::Compose => "compose",
        }
    }

    fn env_key(self) -> &'static str {
        match self {
            Stage::Extract => "BRAINIAC_MODEL_EXTRACT",
            Stage::Resolve => "BRAINIAC_MODEL_RESOLVE",
            Stage::Contradict => "BRAINIAC_MODEL_CONTRADICT",
            Stage::Compose => "BRAINIAC_MODEL_COMPOSE",
        }
    }
}

/// Build a provider from a spec string: `qwen` (default model),
/// `qwen:<model>` (e.g. `qwen:qwen-turbo`), or `mock`. The single place new
/// providers plug into routing.
pub fn provider_from_spec(spec: &str) -> Result<std::sync::Arc<dyn ChatProvider>> {
    let spec = spec.trim();
    let (provider, model) = match spec.split_once(':') {
        Some((p, m)) => (p, Some(m.to_string())),
        None => (spec, None),
    };
    match provider {
        "qwen" => {
            let key = dashscope_key_from_env()
                .context("provider spec `qwen` needs DASHSCOPE_API_KEY or QWEN_API_KEY")?;
            Ok(std::sync::Arc::new(QwenProvider::new(
                key,
                model.or_else(|| std::env::var("QWEN_MODEL").ok()),
                std::env::var("QWEN_BASE_URL").ok(),
            )))
        }
        "mock" => Ok(std::sync::Arc::new(MockProvider::new(|_| {
            r#"{"memories":[]}"#.to_string()
        }))),
        other => anyhow::bail!("unknown provider `{other}` in spec `{spec}` (qwen|mock)"),
    }
}

/// Routes each pipeline stage to a provider. Defaults to one provider for
/// everything; `BRAINIAC_MODEL_EXTRACT` / `_RESOLVE` / `_CONTRADICT` /
/// `_COMPOSE` override a stage with a spec accepted by [`provider_from_spec`].
pub struct ProviderRouter {
    default: std::sync::Arc<dyn ChatProvider>,
    extract: Option<std::sync::Arc<dyn ChatProvider>>,
    resolve: Option<std::sync::Arc<dyn ChatProvider>>,
    contradict: Option<std::sync::Arc<dyn ChatProvider>>,
    compose: Option<std::sync::Arc<dyn ChatProvider>>,
}

impl ProviderRouter {
    /// One provider for every stage (previous behavior).
    pub fn single(provider: std::sync::Arc<dyn ChatProvider>) -> Self {
        Self {
            default: provider,
            extract: None,
            resolve: None,
            contradict: None,
            compose: None,
        }
    }

    /// Apply per-stage env overrides on top of `default`.
    pub fn from_env(default: std::sync::Arc<dyn ChatProvider>) -> Result<Self> {
        let mut router = Self::single(default);
        for stage in [
            Stage::Extract,
            Stage::Resolve,
            Stage::Contradict,
            Stage::Compose,
        ] {
            if let Ok(spec) = std::env::var(stage.env_key()) {
                if spec.trim().is_empty() {
                    continue;
                }
                let provider = provider_from_spec(&spec)
                    .with_context(|| format!("{} = `{spec}`", stage.env_key()))?;
                tracing::info!(stage = stage.as_str(), model = %provider.model_ref(), "stage model override");
                match stage {
                    Stage::Extract => router.extract = Some(provider),
                    Stage::Resolve => router.resolve = Some(provider),
                    Stage::Contradict => router.contradict = Some(provider),
                    Stage::Compose => router.compose = Some(provider),
                }
            }
        }
        Ok(router)
    }

    pub fn for_stage(&self, stage: Stage) -> &dyn ChatProvider {
        let slot = match stage {
            Stage::Extract => &self.extract,
            Stage::Resolve => &self.resolve,
            Stage::Contradict => &self.contradict,
            Stage::Compose => &self.compose,
        };
        slot.as_deref().unwrap_or(self.default.as_ref())
    }

    /// `stage=model` pairs for startup logging.
    pub fn describe(&self) -> String {
        [Stage::Extract, Stage::Resolve, Stage::Contradict]
            .iter()
            .map(|s| format!("{}={}", s.as_str(), self.for_stage(*s).model_ref()))
            .collect::<Vec<_>>()
            .join(" ")
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
                temperature: 0.0,
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
