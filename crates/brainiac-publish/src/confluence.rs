//! The Confluence target (Cloud REST v2, PAT / API-token auth).
//!
//! The pitch positions Confluence as the incumbent that indexes only what someone
//! remembered to write down. This target is the other half of that argument: keep
//! your wiki, and let the pages that matter maintain themselves. The page a team
//! already has bookmarked stops being a 2023 artifact and starts being a
//! projection of what the org currently believes.
//!
//! Strictly one-way (see the crate docs). A published page says so at the top,
//! because a generated page that looks hand-written is a trap for the first
//! person who tries to fix it.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use crate::render::markdown_to_storage;
use crate::{PageToPublish, Publisher};

#[derive(Debug, Deserialize)]
struct ConfluenceConfig {
    /// e.g. `https://acme.atlassian.net/wiki`
    base_url: String,
    /// The space these pages live in.
    space_id: String,
    /// Atlassian account email (basic auth pairs it with the API token).
    user_email: String,
    /// Where the console lives, so citations can link back to the memory.
    console_url: String,
}

pub struct ConfluencePublisher {
    http: reqwest::Client,
    cfg: ConfluenceConfig,
    token: String,
}

impl ConfluencePublisher {
    pub fn from_config(config: &serde_json::Value, token: String) -> Result<Self> {
        let cfg: ConfluenceConfig =
            serde_json::from_value(config.clone()).context("confluence target config")?;
        // reqwest has NO default timeout, so a hung Confluence (or an intercepting
        // proxy/LB that accepts the socket but never answers) would make .send()
        // never return — and publish_org holds a scoped DB transaction across the
        // call, so one dead sink pins a connection open indefinitely. Bound every
        // request with a connect + total timeout so a stall surfaces as an error.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("building confluence http client")?;
        Ok(Self { http, cfg, token })
    }

    fn api(&self, path: &str) -> String {
        format!(
            "{}/api/v2/{}",
            self.cfg.base_url.trim_end_matches('/'),
            path
        )
    }
}

#[derive(Deserialize)]
struct PageResponse {
    id: String,
    version: Option<VersionResponse>,
}

#[derive(Deserialize)]
struct VersionResponse {
    number: i64,
}

#[async_trait]
impl Publisher for ConfluencePublisher {
    fn kind(&self) -> &'static str {
        "confluence"
    }

    async fn publish(&self, page: &PageToPublish<'_>) -> Result<Option<String>> {
        let storage = markdown_to_storage(page.markdown, &self.cfg.console_url);
        let title = page.document.title.clone();

        // Update in place when we have published this page before — creating a
        // second page with the same title would leave a team with two "psp-gateway"
        // pages and no way to tell which one is lying.
        if let Some(id) = page.external_ref {
            // Confluence requires the NEXT version number, so read the current one.
            let current: PageResponse = self
                .http
                .get(self.api(&format!("pages/{id}")))
                .basic_auth(&self.cfg.user_email, Some(&self.token))
                .send()
                .await
                .context("confluence: fetching current page version")?
                .error_for_status()
                .context("confluence: page fetch rejected")?
                .json()
                .await
                .context("confluence: parsing page")?;
            let next = current.version.map(|v| v.number + 1).unwrap_or(2);

            let res = self
                .http
                .put(self.api(&format!("pages/{id}")))
                .basic_auth(&self.cfg.user_email, Some(&self.token))
                .json(&serde_json::json!({
                    "id": id,
                    "status": "current",
                    "title": title,
                    "body": { "representation": "storage", "value": storage },
                    "version": { "number": next, "message": "Recomposed by Brainiac" }
                }))
                .send()
                .await
                .context("confluence: updating page")?;
            if !res.status().is_success() {
                let status = res.status();
                let body = res.text().await.unwrap_or_default();
                bail!("confluence update failed ({status}): {body}");
            }
            return Ok(Some(id.to_string()));
        }

        let res = self
            .http
            .post(self.api("pages"))
            .basic_auth(&self.cfg.user_email, Some(&self.token))
            .json(&serde_json::json!({
                "spaceId": self.cfg.space_id,
                "status": "current",
                "title": title,
                "body": { "representation": "storage", "value": storage }
            }))
            .send()
            .await
            .context("confluence: creating page")?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            bail!("confluence create failed ({status}): {body}");
        }
        let created: PageResponse = res
            .json()
            .await
            .context("confluence: parsing created page")?;
        Ok(Some(created.id))
    }
}
