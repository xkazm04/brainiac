//! v0 identity stub (PLAN.md deviation 3): static bearer tokens mapped to
//! principals via the BRAINIAC_TOKENS env var. OIDC/SCIM replaces this
//! resolver behind the same shape.
//!
//! Format:
//! ```json
//! {"tok_abc": {"org": "<uuid>", "user": "<uuid>", "teams": ["<uuid>"]}}
//! ```

use std::collections::HashMap;

use anyhow::{Context, Result};
use brainiac_core::Principal;
use serde::Deserialize;
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

#[cfg(test)]
mod tests {
    use super::*;

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
