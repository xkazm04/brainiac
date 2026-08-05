//! OpenAI-compatible provider + embedder.
//!
//! One HTTP contract — `POST {base}/chat/completions` and `POST {base}/embeddings`
//! with an OpenAI-shaped body — serves every vendor that speaks it: OpenAI itself,
//! Azure OpenAI, Qwen/DashScope, Groq, Together, DeepSeek, Mistral, OpenRouter, and
//! local Ollama/vLLM. They differ only in base URL, auth header, model id, and
//! (Azure) an `api-version` query — all captured as fields here, so adding one of
//! these vendors is a preset constructor, not a new provider.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{build_http_client, first_env, meter, meter_embedding, EmbedUsage};
use crate::resilience::Resilience;
use crate::{ChatProvider, ChatRequest, ChatResponse};

/// How the API key rides on the request. OpenAI/Qwen/Groq/… use a bearer token;
/// Azure OpenAI (with an API key rather than an AAD token) uses an `api-key`
/// header instead.
#[derive(Debug, Clone)]
pub enum AuthStyle {
    Bearer,
    Header(&'static str),
}

/// A provider speaking the OpenAI `/chat/completions` protocol.
pub struct OpenAiCompatProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    /// LightTrack rollup + `model_ref` prefix, e.g. `openai` / `qwen` / `groq`.
    provider_tag: String,
    /// Error-context + resilience label, e.g. `dashscope` / `openai`.
    wire_label: String,
    auth: AuthStyle,
    /// Appended verbatim to the request path (Azure's `?api-version=…`); empty otherwise.
    query_suffix: String,
    resilience: Resilience,
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

impl OpenAiCompatProvider {
    pub const OPENAI_BASE: &'static str = "https://api.openai.com/v1";
    pub const OPENAI_DEFAULT_MODEL: &'static str = "gpt-4o-mini";
    pub const QWEN_BASE: &'static str = "https://dashscope-intl.aliyuncs.com/compatible-mode/v1";
    pub const QWEN_DEFAULT_MODEL: &'static str = "qwen-max";

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        provider_tag: impl Into<String>,
        wire_label: impl Into<String>,
        auth: AuthStyle,
        query_suffix: String,
    ) -> Self {
        Self {
            http: build_http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            provider_tag: provider_tag.into(),
            wire_label: wire_label.into(),
            auth,
            query_suffix,
            resilience: Resilience::from_env(),
        }
    }

    /// Qwen/DashScope preset (OpenAI-compatible mode). `wire_label` stays
    /// `dashscope` so error contexts read the same as before the generalization.
    pub fn qwen(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self::new(
            base_url.unwrap_or_else(|| Self::QWEN_BASE.to_string()),
            api_key,
            model.unwrap_or_else(|| Self::QWEN_DEFAULT_MODEL.to_string()),
            "qwen",
            "dashscope",
            AuthStyle::Bearer,
            String::new(),
        )
    }

    /// OpenAI (and any bearer-auth OpenAI-compatible vendor: Groq, Together,
    /// DeepSeek, Mistral, OpenRouter, Ollama/vLLM — point `base_url` at them).
    pub fn openai(
        api_key: String,
        model: Option<String>,
        base_url: Option<String>,
        tag: &'static str,
    ) -> Self {
        Self::new(
            base_url.unwrap_or_else(|| Self::OPENAI_BASE.to_string()),
            api_key,
            model.unwrap_or_else(|| Self::OPENAI_DEFAULT_MODEL.to_string()),
            tag,
            tag,
            AuthStyle::Bearer,
            String::new(),
        )
    }

    /// Azure OpenAI: `{endpoint}/openai/deployments/{deployment}` base, an
    /// `api-key` header, and a required `api-version` query.
    pub fn azure(
        endpoint: String,
        api_key: String,
        deployment: String,
        api_version: String,
    ) -> Self {
        let base = format!(
            "{}/openai/deployments/{}",
            endpoint.trim_end_matches('/'),
            deployment
        );
        Self::new(
            base,
            api_key,
            deployment,
            "azure",
            "azure-openai",
            AuthStyle::Header("api-key"),
            format!("?api-version={api_version}"),
        )
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.auth {
            AuthStyle::Bearer => req.bearer_auth(&self.api_key),
            AuthStyle::Header(name) => req.header(name, &self.api_key),
        }
    }
}

#[async_trait]
impl ChatProvider for OpenAiCompatProvider {
    fn model_ref(&self) -> String {
        format!("{}:{}", self.provider_tag, self.model)
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        meter(&self.provider_tag, &self.model, async {
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
            let url = format!("{}/chat/completions{}", self.base_url, self.query_suffix);
            let http_req = self.authed(self.http.post(url).json(&body));
            let text = self.resilience.send(http_req, &self.wire_label).await?;
            let parsed: OpenAiResponse = serde_json::from_str(&text)
                .with_context(|| format!("{} parse", self.wire_label))?;
            // Empty `choices` (content-filter stub, or an error shaped as 200) is "no
            // answer", not "empty answer" — returning it as Ok would have the pipeline
            // extract from "" (0 memories) and nothing goes red. Fail so it retries /
            // surfaces instead of manufacturing a hollow success.
            let content = parsed
                .choices
                .first()
                .map(|c| c.message.content.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("{} returned 200 with no choices", self.wire_label)
                })?;
            // A missing `usage` means "cost unknown", not "cost zero" — never silently
            // meter a real billed call at 0. Log it so accounting can flag the drift.
            let usage = parsed.usage.unwrap_or_else(|| {
                tracing::warn!(
                    provider = %self.provider_tag,
                    model = %self.model,
                    "response omitted usage — recording 0 tokens for a billed call"
                );
                OpenAiUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                }
            });
            Ok(ChatResponse {
                text: content,
                model_ref: self.model_ref(),
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            })
        })
        .await
    }
}

// ── Embeddings (OpenAI-compatible /embeddings) ──────────────────────────────

/// A provider speaking the OpenAI `/embeddings` protocol behind the
/// [`brainiac_core::embed::Embedder`] seam. Covers DashScope `text-embedding-v4`
/// (served Qwen3-Embedding) and OpenAI `text-embedding-3-*`. DashScope caps at 10
/// texts per request; OpenAI is far higher — the cap is a field, so `embed_batch`
/// chunks to whatever the vendor allows.
pub struct OpenAiCompatEmbedder {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    model_name: String,
    dim: usize,
    max_batch: usize,
    wire_label: String,
    resilience: Resilience,
}

impl OpenAiCompatEmbedder {
    pub const QWEN_DEFAULT_MODEL: &'static str = "text-embedding-v4";
    pub const QWEN_DEFAULT_DIM: usize = 1024;
    pub const OPENAI_DEFAULT_MODEL: &'static str = "text-embedding-3-small";
    pub const OPENAI_DEFAULT_DIM: usize = 1536;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        model_name: impl Into<String>,
        dim: usize,
        max_batch: usize,
        wire_label: impl Into<String>,
    ) -> Self {
        Self {
            http: build_http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            model_name: model_name.into(),
            dim,
            max_batch,
            wire_label: wire_label.into(),
            resilience: Resilience::from_env(),
        }
    }

    pub fn qwen(api_key: String, base_url: Option<String>, dim: Option<usize>) -> Self {
        Self::new(
            base_url.unwrap_or_else(|| OpenAiCompatProvider::QWEN_BASE.to_string()),
            api_key,
            Self::QWEN_DEFAULT_MODEL.to_string(),
            "qwen:text-embedding-v4",
            dim.unwrap_or(Self::QWEN_DEFAULT_DIM),
            10,
            "dashscope embeddings",
        )
    }

    pub fn openai(
        api_key: String,
        model: Option<String>,
        base_url: Option<String>,
        dim: Option<usize>,
    ) -> Self {
        let model = model.unwrap_or_else(|| Self::OPENAI_DEFAULT_MODEL.to_string());
        let model_name = format!("openai:{model}");
        Self::new(
            base_url.unwrap_or_else(|| OpenAiCompatProvider::OPENAI_BASE.to_string()),
            api_key,
            model,
            model_name,
            dim.unwrap_or(Self::OPENAI_DEFAULT_DIM),
            256,
            "openai embeddings",
        )
    }

    /// LightTrack rollup key for this embedder's vendor.
    ///
    /// `model_name` is built as `<provider>:<model>` by every constructor above
    /// (`qwen:text-embedding-v4`, `openai:text-embedding-3-small`), so the
    /// vendor half is the tag — the same per-vendor tag the chat providers pass,
    /// never a generic label.
    fn provider_tag(&self) -> &str {
        self.model_name.split(':').next().unwrap_or("openai")
    }

    async fn request(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        #[derive(Deserialize)]
        struct EmbeddingRow {
            index: usize,
            embedding: Vec<f32>,
        }
        #[derive(Deserialize)]
        struct EmbeddingUsage {
            #[serde(default)]
            prompt_tokens: u64,
        }
        #[derive(Deserialize)]
        struct EmbeddingResponse {
            data: Vec<EmbeddingRow>,
            #[serde(default)]
            usage: Option<EmbeddingUsage>,
        }
        // Metered like every chat call: same chokepoint, same fire-and-forget
        // delivery, same environment tag — `operation="embedding"` so this
        // vendor-billed traffic rolls up on its own line. Before this, the
        // embedding lane hit a paid endpoint and reported nothing at all.
        Ok(meter_embedding(self.provider_tag(), &self.model, async {
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
            let body = self.resilience.send(req, &self.wire_label).await?;
            let parsed: EmbeddingResponse = serde_json::from_str(&body)
                .with_context(|| format!("{} parse", self.wire_label))?;
            anyhow::ensure!(
                parsed.data.len() == texts.len(),
                "{} returned {} embeddings for {} inputs",
                self.wire_label,
                parsed.data.len(),
                texts.len()
            );
            // A missing `usage` means "cost unknown", not "cost zero" — mirror
            // the chat path and log the drift instead of silently metering 0.
            let input_tokens = match parsed.usage.as_ref() {
                Some(u) => u.prompt_tokens,
                None => {
                    tracing::warn!(
                        provider = %self.provider_tag(),
                        model = %self.model,
                        "{} returned no usage block; metering 0 input tokens",
                        self.wire_label
                    );
                    0
                }
            };
            let mut rows = parsed.data;
            rows.sort_by_key(|r| r.index);
            for r in &rows {
                anyhow::ensure!(
                    r.embedding.len() == self.dim,
                    "{} returned dim {} (expected {})",
                    self.wire_label,
                    r.embedding.len(),
                    self.dim
                );
            }
            Ok(EmbedUsage {
                value: rows.into_iter().map(|r| r.embedding).collect::<Vec<_>>(),
                input_tokens,
            })
        })
        .await?
        .value)
    }
}

#[async_trait]
impl brainiac_core::embed::Embedder for OpenAiCompatEmbedder {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Vendors reject empty input — degrade to a zero vector, matching the
        // deterministic embedder's behavior for blank text.
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
        for chunk in texts.chunks(self.max_batch) {
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

// ── Backward-compatible presets ─────────────────────────────────────────────
//
// `QwenProvider` / `QwenEmbedder` remain the public Qwen entry points (call sites,
// integration tests, and the deploy default all name them). They are thin presets
// over the generalized OpenAI-compatible impls above.

pub(crate) fn qwen_key_from_env() -> Option<String> {
    first_env(&["DASHSCOPE_API_KEY", "QWEN_API_KEY"])
}

pub struct QwenProvider(OpenAiCompatProvider);

impl QwenProvider {
    pub const DEFAULT_BASE: &'static str = OpenAiCompatProvider::QWEN_BASE;
    pub const DEFAULT_MODEL: &'static str = OpenAiCompatProvider::QWEN_DEFAULT_MODEL;

    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self(OpenAiCompatProvider::qwen(api_key, model, base_url))
    }

    /// Construct from environment (DASHSCOPE_API_KEY or QWEN_API_KEY,
    /// QWEN_MODEL, QWEN_BASE_URL).
    pub fn from_env() -> Option<Self> {
        Some(Self::new(
            qwen_key_from_env()?,
            std::env::var("QWEN_MODEL").ok(),
            std::env::var("QWEN_BASE_URL").ok(),
        ))
    }
}

#[async_trait]
impl ChatProvider for QwenProvider {
    fn model_ref(&self) -> String {
        self.0.model_ref()
    }
    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        self.0.complete(req).await
    }
}

pub struct QwenEmbedder(OpenAiCompatEmbedder);

impl QwenEmbedder {
    pub const DEFAULT_MODEL: &'static str = OpenAiCompatEmbedder::QWEN_DEFAULT_MODEL;
    pub const DEFAULT_DIM: usize = OpenAiCompatEmbedder::QWEN_DEFAULT_DIM;

    pub fn new(api_key: String, base_url: Option<String>, dim: Option<usize>) -> Self {
        Self(OpenAiCompatEmbedder::qwen(api_key, base_url, dim))
    }

    /// Construct from environment (DASHSCOPE_API_KEY or QWEN_API_KEY,
    /// QWEN_BASE_URL, QWEN_EMBED_DIM).
    pub fn from_env() -> Option<Self> {
        Some(Self::new(
            qwen_key_from_env()?,
            std::env::var("QWEN_BASE_URL").ok(),
            std::env::var("QWEN_EMBED_DIM")
                .ok()
                .and_then(|d| d.parse().ok()),
        ))
    }
}

#[async_trait]
impl brainiac_core::embed::Embedder for QwenEmbedder {
    fn model_name(&self) -> &str {
        self.0.model_name()
    }
    fn dim(&self) -> usize {
        self.0.dim()
    }
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.0.embed(text).await
    }
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.0.embed_batch(texts).await
    }
}

/// Build an OpenAI chat provider from env (`OPENAI_API_KEY`, `OPENAI_BASE_URL`,
/// `OPENAI_MODEL`). Also the base for OpenAI-compatible vendors via `openai-compat`.
pub fn openai_from_env(model: Option<String>) -> Result<OpenAiCompatProvider> {
    let key = first_env(&["OPENAI_API_KEY"]).context("provider `openai` needs OPENAI_API_KEY")?;
    Ok(OpenAiCompatProvider::openai(
        key,
        model.or_else(|| std::env::var("OPENAI_MODEL").ok()),
        std::env::var("OPENAI_BASE_URL").ok(),
        "openai",
    ))
}

/// A generic OpenAI-compatible vendor: `BRAINIAC_OPENAI_COMPAT_BASE_URL` +
/// `BRAINIAC_OPENAI_COMPAT_KEY` (falls back to `OPENAI_API_KEY`). The escape hatch
/// for Groq/Together/DeepSeek/OpenRouter/Ollama without a dedicated preset.
pub fn openai_compat_from_env(model: Option<String>) -> Result<OpenAiCompatProvider> {
    let base = first_env(&["BRAINIAC_OPENAI_COMPAT_BASE_URL"]).context(
        "provider `openai-compat` needs BRAINIAC_OPENAI_COMPAT_BASE_URL (the vendor's /v1 base)",
    )?;
    let key = first_env(&["BRAINIAC_OPENAI_COMPAT_KEY", "OPENAI_API_KEY"])
        .context("provider `openai-compat` needs BRAINIAC_OPENAI_COMPAT_KEY or OPENAI_API_KEY")?;
    Ok(OpenAiCompatProvider::openai(
        key,
        model.or_else(|| std::env::var("BRAINIAC_OPENAI_COMPAT_MODEL").ok()),
        Some(base),
        "openai-compat",
    ))
}

/// Azure OpenAI from env: `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_API_KEY`,
/// `AZURE_OPENAI_DEPLOYMENT` (or the spec's `azure:<deployment>`),
/// `AZURE_OPENAI_API_VERSION` (default `2024-10-21`).
pub fn azure_from_env(deployment: Option<String>) -> Result<OpenAiCompatProvider> {
    let endpoint = first_env(&["AZURE_OPENAI_ENDPOINT"])
        .context("provider `azure` needs AZURE_OPENAI_ENDPOINT")?;
    let key = first_env(&["AZURE_OPENAI_API_KEY", "OPENAI_API_KEY"])
        .context("provider `azure` needs AZURE_OPENAI_API_KEY")?;
    let deployment = deployment
        .or_else(|| std::env::var("AZURE_OPENAI_DEPLOYMENT").ok())
        .context("provider `azure` needs AZURE_OPENAI_DEPLOYMENT (or spec `azure:<deployment>`)")?;
    let api_version =
        std::env::var("AZURE_OPENAI_API_VERSION").unwrap_or_else(|_| "2024-10-21".to_string());
    Ok(OpenAiCompatProvider::azure(
        endpoint,
        key,
        deployment,
        api_version,
    ))
}
