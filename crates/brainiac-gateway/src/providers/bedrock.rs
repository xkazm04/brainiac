//! AWS Bedrock runtime — Anthropic (Claude) models through Bedrock's SigV4-signed
//! `InvokeModel` endpoint (`POST /model/{modelId}/invoke`).
//!
//! Bedrock speaks the model-native body, so for Claude this is exactly the
//! Anthropic Messages body (with `anthropic_version: bedrock-2023-05-31` in place
//! of a `model` field) and the Anthropic response parser — both reused from
//! [`super::anthropic`]. The only Bedrock-specific work is AWS Signature V4, hand-
//! rolled here over `hmac`+`sha2` (pure Rust, no AWS SDK / no ring). Credentials
//! come from the standard AWS env vars.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use super::anthropic::{messages_body, parse_messages_response};
use super::{build_http_client, first_env, meter};
use crate::resilience::Resilience;
use crate::{ChatProvider, ChatRequest, ChatResponse};

type HmacSha256 = Hmac<Sha256>;

const SERVICE: &str = "bedrock";
const BEDROCK_ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";

struct AwsCreds {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
    region: String,
}

impl AwsCreds {
    fn from_env() -> Result<Self> {
        Ok(Self {
            access_key: first_env(&["AWS_ACCESS_KEY_ID"])
                .context("provider `bedrock` needs AWS_ACCESS_KEY_ID")?,
            secret_key: first_env(&["AWS_SECRET_ACCESS_KEY"])
                .context("provider `bedrock` needs AWS_SECRET_ACCESS_KEY")?,
            session_token: first_env(&["AWS_SESSION_TOKEN"]),
            region: first_env(&["AWS_REGION", "AWS_DEFAULT_REGION"])
                .unwrap_or_else(|| "us-east-1".to_string()),
        })
    }
}

pub struct BedrockProvider {
    http: reqwest::Client,
    creds: AwsCreds,
    model: String,
    /// Full base, e.g. `https://bedrock-runtime.us-east-1.amazonaws.com`. Derived
    /// from the region unless `BEDROCK_BASE_URL` overrides it (tests, VPC endpoints).
    base_url: String,
    resilience: Resilience,
}

impl BedrockProvider {
    pub const DEFAULT_MODEL: &'static str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

    pub fn from_env(model: Option<String>) -> Result<Self> {
        let creds = AwsCreds::from_env()?;
        let base_url = std::env::var("BEDROCK_BASE_URL")
            .unwrap_or_else(|_| format!("https://bedrock-runtime.{}.amazonaws.com", creds.region));
        Ok(Self {
            http: build_http_client(),
            model: model
                .or_else(|| std::env::var("BEDROCK_MODEL").ok())
                .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string()),
            base_url: base_url.trim_end_matches('/').to_string(),
            creds,
            resilience: Resilience::from_env(),
        })
    }
}

#[async_trait]
impl ChatProvider for BedrockProvider {
    fn model_ref(&self) -> String {
        format!("bedrock:{}", self.model)
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse> {
        meter("bedrock", &self.model, async {
            let body = messages_body(None, Some(BEDROCK_ANTHROPIC_VERSION), req).to_string();
            let encoded_model = uri_encode(&self.model);
            let path = format!("/model/{encoded_model}/invoke");
            let url = format!("{}{path}", self.base_url);
            let host = host_of(&url).context("bedrock base URL has no host")?;

            let signed = sign_v4(&self.creds, &host, &path, body.as_bytes());

            let mut http_req = self
                .http
                .post(url)
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .header("x-amz-date", &signed.amz_date)
                .header("authorization", &signed.authorization)
                .body(body);
            if let Some(tok) = &self.creds.session_token {
                http_req = http_req.header("x-amz-security-token", tok);
            }

            let text = self.resilience.send(http_req, "bedrock").await?;
            let (content, input_tokens, output_tokens) = parse_messages_response(&text, "bedrock")?;
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

// ── SigV4 (AWS Signature Version 4) ─────────────────────────────────────────

struct SignedRequest {
    authorization: String,
    amz_date: String,
}

fn hmac(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// URI-encode a path segment per SigV4 (unreserved chars pass through; everything
/// else — including the `:` in Bedrock model ids like `…-v2:0` — is percent-encoded).
fn uri_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://")?.1;
    let authority = after_scheme.split(['/', '?']).next()?;
    Some(authority.to_string())
}

/// Derive the SigV4 signing key: HMAC chain over date → region → service →
/// `aws4_request`, seeded with `"AWS4" + secret`.
fn derive_signing_key(secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    hmac(&k_service, b"aws4_request")
}

/// Sign a Bedrock POST and return the `Authorization` + `x-amz-date` header values.
/// Signed header set: content-type, host, x-amz-date, and x-amz-security-token when
/// a session token is present.
fn sign_v4(creds: &AwsCreds, host: &str, path: &str, payload: &[u8]) -> SignedRequest {
    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(payload);

    // Canonical headers must be sorted by name; each signed header also appears on
    // the wire with exactly this value.
    let mut headers: Vec<(String, String)> = vec![
        ("content-type".into(), "application/json".into()),
        ("host".into(), host.to_string()),
        ("x-amz-date".into(), amz_date.clone()),
    ];
    if let Some(tok) = &creds.session_token {
        headers.push(("x-amz-security-token".into(), tok.clone()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    // Bedrock invoke has no query string.
    let canonical_request =
        format!("POST\n{path}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");

    let scope = format!("{date_stamp}/{}/{SERVICE}/aws4_request", creds.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let signing_key = derive_signing_key(&creds.secret_key, &date_stamp, &creds.region, SERVICE);
    let signature = hex(&hmac(&signing_key, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        creds.access_key
    );
    SignedRequest {
        authorization,
        amz_date,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AWS's published signing-key derivation example
    /// (docs: "Examples of how to derive a signing key for Signature Version 4").
    /// Proving the HMAC chain against the official vector validates the crypto
    /// independently of any live Bedrock call.
    #[test]
    fn signing_key_matches_aws_reference_vector() {
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        assert_eq!(
            hex(&key),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[test]
    fn uri_encode_percent_encodes_model_id_colon() {
        assert_eq!(
            uri_encode("anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "anthropic.claude-3-5-sonnet-20241022-v2%3A0"
        );
    }

    #[test]
    fn host_of_extracts_authority() {
        assert_eq!(
            host_of("https://bedrock-runtime.us-east-1.amazonaws.com/model/x/invoke").as_deref(),
            Some("bedrock-runtime.us-east-1.amazonaws.com")
        );
        assert_eq!(
            host_of("http://127.0.0.1:8787/model/x/invoke").as_deref(),
            Some("127.0.0.1:8787")
        );
    }
}
