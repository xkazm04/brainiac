//! brainiac-gateway — BYOM: one trait, a spectrum of swappable providers.
//!
//! Chat providers (all behind [`ChatProvider`], all riding the shared
//! [`resilience`] + [`lighttrack`] path):
//! - [`OpenAiCompatProvider`] — the OpenAI `/chat/completions` protocol, which
//!   also serves Azure OpenAI, Qwen/DashScope, Groq, Together, DeepSeek, Mistral,
//!   OpenRouter, and local Ollama/vLLM by base-URL + auth swap. [`QwenProvider`]
//!   is the Qwen preset (the deploy default).
//! - [`AnthropicProvider`] — Anthropic Messages API.
//! - [`GeminiProvider`] — Google Gemini `generateContent`.
//! - [`BedrockProvider`] — AWS Bedrock (SigV4; Claude-on-Bedrock).
//! - [`VertexProvider`] — Google Vertex AI (service-account OAuth; Gemini).
//! - [`MockProvider`] — deterministic, injectable responder for pipeline tests.
//!
//! Selection is centralized in two functions: [`provider_from_spec`] (a
//! `provider[:model]` spec string) and [`default_chat_from_env`] (the process
//! default, keyed on `BRAINIAC_PROVIDER`). Everything downstream holds
//! `Arc<dyn ChatProvider>` via [`ProviderRouter`], so per-stage overrides
//! (`BRAINIAC_MODEL_*`) can mix providers freely.

use anyhow::{Context, Result};
use async_trait::async_trait;

pub mod resilience;

/// Fire-and-forget LLM-call telemetry (LightTrack). Disabled unless
/// `LIGHTTRACK_URL` is set; see [`lighttrack`].
pub(crate) mod lighttrack;

mod providers;

pub use providers::anthropic::AnthropicProvider;
pub use providers::bedrock::BedrockProvider;
pub use providers::google::GeminiProvider;
pub use providers::openai::{
    AuthStyle, OpenAiCompatEmbedder, OpenAiCompatProvider, QwenEmbedder, QwenProvider,
};
pub use providers::vertex::VertexProvider;

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
    /// Practice-divergence adjudication (the standardization sweep): decides
    /// whether several teams solve one practice in incompatible ways. A short
    /// classification call like resolve/contradict — a cheaper model is usually
    /// enough — but routable on its own so an operator can dial it independently.
    Divergence,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Extract => "extract",
            Stage::Resolve => "resolve",
            Stage::Contradict => "contradict",
            Stage::Compose => "compose",
            Stage::Divergence => "divergence",
        }
    }

    fn env_key(self) -> &'static str {
        match self {
            Stage::Extract => "BRAINIAC_MODEL_EXTRACT",
            Stage::Resolve => "BRAINIAC_MODEL_RESOLVE",
            Stage::Contradict => "BRAINIAC_MODEL_CONTRADICT",
            Stage::Compose => "BRAINIAC_MODEL_COMPOSE",
            Stage::Divergence => "BRAINIAC_MODEL_DIVERGENCE",
        }
    }
}

/// Build a chat provider from a spec string: `<provider>` (that provider's
/// env-configured default model) or `<provider>:<model>`. The single place new
/// providers plug into routing.
///
/// Providers: `qwen`, `openai`, `azure`, `openai-compat`, `anthropic`,
/// `google` (alias `gemini`), `bedrock`, `vertex`, `mock`. Each reads its own
/// credentials from the environment (see [`default_chat_from_env`] for the key
/// matrix). `split_once(':')` splits only on the FIRST colon, so provider model
/// ids that themselves contain colons (e.g. Bedrock `…-v2:0`) survive intact.
pub fn provider_from_spec(spec: &str) -> Result<std::sync::Arc<dyn ChatProvider>> {
    let spec = spec.trim();
    let (provider, model) = match spec.split_once(':') {
        Some((p, m)) => (p.trim(), Some(m.trim().to_string())),
        None => (spec, None),
    };
    let arc: std::sync::Arc<dyn ChatProvider> = match provider {
        "qwen" => {
            let key = providers::openai::qwen_key_from_env()
                .context("provider spec `qwen` needs DASHSCOPE_API_KEY or QWEN_API_KEY")?;
            std::sync::Arc::new(QwenProvider::new(
                key,
                model.or_else(|| std::env::var("QWEN_MODEL").ok()),
                std::env::var("QWEN_BASE_URL").ok(),
            ))
        }
        "openai" => std::sync::Arc::new(providers::openai::openai_from_env(model)?),
        "azure" => std::sync::Arc::new(providers::openai::azure_from_env(model)?),
        "openai-compat" => std::sync::Arc::new(providers::openai::openai_compat_from_env(model)?),
        "anthropic" => std::sync::Arc::new(AnthropicProvider::from_env(model)?),
        "google" | "gemini" => std::sync::Arc::new(GeminiProvider::from_env(model)?),
        "bedrock" => std::sync::Arc::new(BedrockProvider::from_env(model)?),
        "vertex" => std::sync::Arc::new(VertexProvider::from_env(model)?),
        "mock" => std::sync::Arc::new(MockProvider::new(|_| r#"{"memories":[]}"#.to_string())),
        other => anyhow::bail!(
            "unknown provider `{other}` in spec `{spec}` \
             (qwen|openai|azure|openai-compat|anthropic|google|bedrock|vertex|mock)"
        ),
    };
    Ok(arc)
}

/// The process-default chat provider, selected by `BRAINIAC_PROVIDER`
/// (default `qwen`, so an unconfigured deploy keeps its previous behavior).
/// `BRAINIAC_MODEL` optionally pins the default model for that provider; per-stage
/// `BRAINIAC_MODEL_*` still override individual stages on top of this default.
///
/// Credential env by provider: `qwen` → DASHSCOPE_API_KEY|QWEN_API_KEY;
/// `openai` → OPENAI_API_KEY (+ OPENAI_BASE_URL/OPENAI_MODEL); `azure` →
/// AZURE_OPENAI_ENDPOINT/_API_KEY/_DEPLOYMENT; `openai-compat` →
/// BRAINIAC_OPENAI_COMPAT_BASE_URL/_KEY; `anthropic` → ANTHROPIC_API_KEY;
/// `google` → GEMINI_API_KEY|GOOGLE_API_KEY; `bedrock` → AWS_ACCESS_KEY_ID/
/// AWS_SECRET_ACCESS_KEY/AWS_REGION; `vertex` → GOOGLE_CLOUD_PROJECT +
/// VERTEX_ACCESS_TOKEN|VERTEX_SA_JSON|GOOGLE_APPLICATION_CREDENTIALS.
pub fn default_chat_from_env() -> Result<std::sync::Arc<dyn ChatProvider>> {
    let provider = std::env::var("BRAINIAC_PROVIDER")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "qwen".to_string());
    let spec = match std::env::var("BRAINIAC_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
    {
        Some(model) => format!("{provider}:{}", model.trim()),
        None => provider.clone(),
    };
    provider_from_spec(&spec).with_context(|| format!("BRAINIAC_PROVIDER = `{provider}`"))
}

/// Routes each pipeline stage to a provider. Defaults to one provider for
/// everything; `BRAINIAC_MODEL_EXTRACT` / `_RESOLVE` / `_CONTRADICT` /
/// `_COMPOSE` / `_DIVERGENCE` override a stage with a spec accepted by
/// [`provider_from_spec`].
pub struct ProviderRouter {
    default: std::sync::Arc<dyn ChatProvider>,
    extract: Option<std::sync::Arc<dyn ChatProvider>>,
    resolve: Option<std::sync::Arc<dyn ChatProvider>>,
    contradict: Option<std::sync::Arc<dyn ChatProvider>>,
    compose: Option<std::sync::Arc<dyn ChatProvider>>,
    divergence: Option<std::sync::Arc<dyn ChatProvider>>,
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
            divergence: None,
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
            Stage::Divergence,
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
                    Stage::Divergence => router.divergence = Some(provider),
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
            Stage::Divergence => &self.divergence,
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
