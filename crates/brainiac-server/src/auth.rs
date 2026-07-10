//! Bearer-token identity. Two resolvers behind one seam:
//!
//! - **Env bootstrap** (v0 stub, PLAN.md deviation 3): static tokens mapped
//!   to principals via BRAINIAC_TOKENS. Full authority (all scopes) — these
//!   are the operator's break-glass credentials and what mints API tokens.
//! - **Managed API tokens** (`brk_…`, migrations/0003_api_tokens.sql):
//!   issued via POST /v1/tokens, stored hashed (sha256), scoped
//!   (read/write/admin), revocable, `last_used_at`-tracked. Team memberships
//!   resolve from `team_members` at auth time, so they stay current.
//!
//! Env format:
//! ```json
//! {"tok_abc": {"org": "<uuid>", "user": "<uuid>", "teams": ["<uuid>"]}}
//! ```

use std::collections::HashMap;

use anyhow::{Context, Result};
use brainiac_core::Principal;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct TokenEntry {
    org: Uuid,
    user: Uuid,
    #[serde(default)]
    teams: Vec<Uuid>,
}

#[derive(Clone, Default)]
pub struct TokenMap {
    tokens: HashMap<String, Principal>,
}

impl TokenMap {
    pub fn from_env() -> Result<Self> {
        let raw = std::env::var("BRAINIAC_TOKENS").unwrap_or_else(|_| "{}".into());
        Self::from_json(&raw)
    }

    pub fn from_json(raw: &str) -> Result<Self> {
        let entries: HashMap<String, TokenEntry> =
            serde_json::from_str(raw).context("parsing BRAINIAC_TOKENS")?;
        Ok(Self {
            tokens: entries
                .into_iter()
                .map(|(token, e)| {
                    (
                        token,
                        Principal {
                            org_id: e.org,
                            user_id: e.user,
                            team_ids: e.teams,
                        },
                    )
                })
                .collect(),
        })
    }

    /// Resolve `Authorization: Bearer <token>` to a principal.
    pub fn resolve(&self, bearer: &str) -> Option<&Principal> {
        self.tokens.get(bearer)
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

// ── managed API tokens ──────────────────────────────────────────────────

pub const TOKEN_PREFIX: &str = "brk_";
pub const SCOPES: [&str; 3] = ["read", "write", "admin"];

/// Where a request's authority came from. Env tokens carry every scope;
/// API tokens carry exactly what they were minted with (admin implies all).
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub principal: Principal,
    /// None = env bootstrap token (unrestricted).
    pub scopes: Option<Vec<String>>,
}

impl AuthContext {
    pub fn allows(&self, scope: &str) -> bool {
        match &self.scopes {
            None => true,
            Some(scopes) => scopes.iter().any(|s| s == scope || s == "admin"),
        }
    }
}

pub fn hash_token(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

/// Mint a fresh secret: `brk_` + 64 hex chars (2×UUIDv4 = 244 bits of
/// entropy, no extra rand dependency). Returns (secret, display_prefix).
pub fn mint_secret() -> (String, String) {
    let secret = format!(
        "{}{}{}",
        TOKEN_PREFIX,
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let prefix = secret.chars().take(TOKEN_PREFIX.len() + 8).collect();
    (secret, prefix)
}

/// Resolve a bearer token to an [`AuthContext`]: env map first (fast path,
/// no I/O), then the api_tokens table for `brk_…` secrets.
pub async fn resolve_bearer(
    tokens: &TokenMap,
    store: &brainiac_store::Store,
    bearer: &str,
) -> Result<Option<AuthContext>> {
    if let Some(principal) = tokens.resolve(bearer) {
        return Ok(Some(AuthContext {
            principal: principal.clone(),
            scopes: None,
        }));
    }
    if !bearer.starts_with(TOKEN_PREFIX) {
        return Ok(None);
    }
    let Some(resolved) = brainiac_store::tokens::resolve(store.pool(), &hash_token(bearer)).await?
    else {
        return Ok(None);
    };
    let team_ids =
        brainiac_store::tokens::team_ids_of(store.pool(), resolved.org_id, resolved.user_id)
            .await?;
    Ok(Some(AuthContext {
        principal: Principal {
            org_id: resolved.org_id,
            user_id: resolved.user_id,
            team_ids,
        },
        scopes: Some(resolved.scopes),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_gate_and_admin_implies_all() {
        let p = Principal {
            org_id: Uuid::nil(),
            user_id: Uuid::nil(),
            team_ids: vec![],
        };
        let env = AuthContext {
            principal: p.clone(),
            scopes: None,
        };
        assert!(env.allows("admin"));
        let read_only = AuthContext {
            principal: p.clone(),
            scopes: Some(vec!["read".into()]),
        };
        assert!(read_only.allows("read"));
        assert!(!read_only.allows("write"));
        assert!(!read_only.allows("admin"));
        let admin = AuthContext {
            principal: p,
            scopes: Some(vec!["admin".into()]),
        };
        assert!(admin.allows("read") && admin.allows("write") && admin.allows("admin"));
    }

    #[test]
    fn minted_secrets_are_unique_prefixed_and_hashable() {
        let (a, pa) = mint_secret();
        let (b, _) = mint_secret();
        assert_ne!(a, b);
        assert!(a.starts_with(TOKEN_PREFIX));
        assert_eq!(pa.len(), TOKEN_PREFIX.len() + 8);
        assert_eq!(hash_token(&a).len(), 32);
        assert_ne!(hash_token(&a), hash_token(&b));
    }

    #[test]
    fn parses_and_resolves() {
        let raw = r#"{"tok_a": {"org": "11111111-1111-8111-8111-111111111111",
                                 "user": "22222222-2222-8222-8222-222222222222",
                                 "teams": ["33333333-3333-8333-8333-333333333333"]}}"#;
        let map = TokenMap::from_json(raw).expect("parse");
        let p = map.resolve("tok_a").expect("resolve");
        assert_eq!(p.team_ids.len(), 1);
        assert!(map.resolve("tok_b").is_none());
    }
}
