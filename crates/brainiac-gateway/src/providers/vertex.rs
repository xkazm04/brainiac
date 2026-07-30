//! Google Vertex AI — Gemini models on a regional Vertex endpoint.
//!
//! The request/response is identical to the native Gemini API (body + parser
//! reused from [`super::google`]); Vertex differs only in host and auth. Auth is a
//! Google OAuth bearer token, obtained one of two ways:
//!   1. `VERTEX_ACCESS_TOKEN` — a token minted out-of-band (e.g.
//!      `gcloud auth print-access-token`); used verbatim, no refresh.
//!   2. A service-account key (`GOOGLE_APPLICATION_CREDENTIALS` path or inline
//!      `VERTEX_SA_JSON`) — this provider mints a short-lived access token via the
//!      JWT-bearer grant (RS256-signed assertion → `oauth2.googleapis.com/token`)
//!      and caches it until shortly before expiry.
//!
//! RS256 signing uses the pure-Rust `rsa` crate (no ring / no C toolchain).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use rsa::pkcs8::DecodePrivateKey;
use rsa::{Pkcs1v15Sign, RsaPrivateKey};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::google::{generate_content_body, parse_generate_content};
use super::{build_http_client, first_env, meter};
use crate::resilience::Resilience;
use crate::{ChatProvider, ChatRequest, ChatResponse};

/// How the provider obtains its OAuth bearer token.
enum TokenSource {
    /// A pre-minted token, used as-is.
    Static(String),
    /// A service account that mints (and re-mints) tokens on demand.
    ServiceAccount(ServiceAccount),
}

struct ServiceAccount {
    client_email: String,
    private_key_pem: String,
    token_uri: String,
}

#[derive(Deserialize)]
struct SaKeyFile {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
}

pub struct VertexProvider {
    http: reqwest::Client,
    project: String,
    location: String,
    model: String,
    base_url: Option<String>,
    token_source: TokenSource,
    /// `(access_token, expires_at)` cache for the service-account path.
    cached: Mutex<Option<(String, Instant)>>,
    resilience: Resilience,
}

impl VertexProvider {
    pub const DEFAULT_MODEL: &'static str = "gemini-2.5-flash";
    pub const DEFAULT_LOCATION: &'static str = "us-central1";
    const SCOPE: &'static str = "https://www.googleapis.com/auth/cloud-platform";
    const DEFAULT_TOKEN_URI: &'static str = "https://oauth2.googleapis.com/token";

    pub fn from_env(model: Option<String>) -> Result<Self> {
        let project = first_env(&["GOOGLE_CLOUD_PROJECT", "VERTEX_PROJECT"])
            .context("provider `vertex` needs GOOGLE_CLOUD_PROJECT (or VERTEX_PROJECT)")?;
        let location =
            first_env(&["VERTEX_LOCATION"]).unwrap_or_else(|| Self::DEFAULT_LOCATION.to_string());
        let model = model
            .or_else(|| std::env::var("VERTEX_MODEL").ok())
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let token_source = Self::token_source_from_env()?;
        Ok(Self {
            http: build_http_client(),
            project,
            location,
            model,
            base_url: std::env::var("VERTEX_BASE_URL").ok(),
            token_source,
            cached: Mutex::new(None),
            resilience: Resilience::from_env(),
        })
    }

    fn token_source_from_env() -> Result<TokenSource> {
        if let Some(tok) = first_env(&["VERTEX_ACCESS_TOKEN"]) {
            return Ok(TokenSource::Static(tok));
        }
        // Service-account JSON: inline env, else a file path.
        let raw = match first_env(&["VERTEX_SA_JSON"]) {
            Some(json) => json,
            None => {
                let path = first_env(&["GOOGLE_APPLICATION_CREDENTIALS"]).context(
                    "provider `vertex` needs VERTEX_ACCESS_TOKEN, VERTEX_SA_JSON, or \
                     GOOGLE_APPLICATION_CREDENTIALS (service-account key path)",
                )?;
                std::fs::read_to_string(&path)
                    .with_context(|| format!("reading service-account key at {path}"))?
            }
        };
        let key: SaKeyFile =
            serde_json::from_str(&raw).context("parsing Vertex service-account JSON")?;
        Ok(TokenSource::ServiceAccount(ServiceAccount {
            client_email: key.client_email,
            private_key_pem: key.private_key,
            token_uri: key
                .token_uri
                .unwrap_or_else(|| Self::DEFAULT_TOKEN_URI.to_string()),
        }))
    }

    fn endpoint(&self) -> String {
        match &self.base_url {
            Some(base) => format!(
                "{}/v1/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
                base.trim_end_matches('/'),
                self.project,
                self.location,
                self.model
            ),
            None => format!(
                "https://{loc}-aiplatform.googleapis.com/v1/projects/{proj}/locations/{loc}/publishers/google/models/{model}:generateContent",
                loc = self.location,
                proj = self.project,
                model = self.model
            ),
        }
    }

    /// A valid OAuth bearer token: the static one, a cached one, or a freshly
    /// minted one. The cache read never spans the mint `await`.
    async fn access_token(&self) -> Result<String> {
        let sa = match &self.token_source {
            TokenSource::Static(t) => return Ok(t.clone()),
            TokenSource::ServiceAccount(sa) => sa,
        };
        if let Some((tok, exp)) = self.cached.lock().expect("token cache").as_ref() {
            if Instant::now() < *exp {
                return Ok(tok.clone());
            }
        }
        let (token, ttl) = self.mint_token(sa).await?;
        // Refresh a minute early so an in-flight call never races expiry.
        let expires_at = Instant::now() + ttl.saturating_sub(Duration::from_secs(60));
        *self.cached.lock().expect("token cache") = Some((token.clone(), expires_at));
        Ok(token)
    }

    async fn mint_token(&self, sa: &ServiceAccount) -> Result<(String, Duration)> {
        let assertion = sign_jwt(sa, Self::SCOPE)?;
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(default = "default_ttl")]
            expires_in: u64,
        }
        fn default_ttl() -> u64 {
            3600
        }
        // Not through `resilience` (that layer is tuned for the model call and its
        // error taxonomy); a token mint is a simple form POST with its own context.
        let resp = self
            .http
            .post(&sa.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ])
            .send()
            .await
            .context("vertex token exchange request")?;
        let status = resp.status();
        let body = resp.text().await.context("vertex token exchange body")?;
        anyhow::ensure!(
            status.is_success(),
            "vertex token exchange {status}: {}",
            body.chars().take(300).collect::<String>()
        );
        let parsed: TokenResponse =
            serde_json::from_str(&body).context("vertex token exchange parse")?;
        Ok((parsed.access_token, Duration::from_secs(parsed.expires_in)))
    }
}

/// Build and RS256-sign the service-account assertion JWT for the JWT-bearer grant.
fn sign_jwt(sa: &ServiceAccount, scope: &str) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let header = json!({"alg": "RS256", "typ": "JWT"});
    let claims = json!({
        "iss": sa.client_email,
        "scope": scope,
        "aud": sa.token_uri,
        "iat": now,
        "exp": now + 3600,
    });
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let signing_input = format!(
        "{}.{}",
        b64(header.to_string().as_bytes()),
        b64(claims.to_string().as_bytes())
    );
    let key = RsaPrivateKey::from_pkcs8_pem(sa.private_key_pem.trim())
        .context("parsing service-account RSA private key (expected PKCS#8 PEM)")?;
    let digest = Sha256::digest(signing_input.as_bytes());
    let signature = key
        .sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
        .context("RS256-signing the Vertex assertion")?;
    Ok(format!("{signing_input}.{}", b64(&signature)))
}

#[async_trait]
impl ChatProvider for VertexProvider {
    fn model_ref(&self) -> String {
        format!("vertex:{}", self.model)
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        meter("vertex", &self.model, async {
            let token = self.access_token().await?;
            let http_req = self
                .http
                .post(self.endpoint())
                .bearer_auth(&token)
                .json(&generate_content_body(req));
            let text = self.resilience.send(http_req, "vertex").await?;
            let (content, input_tokens, output_tokens) = parse_generate_content(&text, "vertex")?;
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
