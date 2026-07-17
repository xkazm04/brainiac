//! The Git target: compiled pages as markdown files in the org's repo.
//!
//! This is the humblest publisher and, for a lot of orgs, the right one: docs
//! land next to the code they describe, review happens in the tool the team
//! already argues in, and the history is the repo's history.
//!
//! It writes files and stops there — it does not commit or push. Committing is a
//! decision about someone's branch protection, CI budget, and release process,
//! and a tool that guesses at those will be uninstalled by the first person it
//! surprises. The operator points this at a checkout and drives the commit from
//! their own pipeline, where the credentials and the policy already live.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;

use crate::{pointer, BundleToPublish, PageToPublish, Publisher};

#[derive(Debug, Deserialize)]
struct GitConfig {
    /// Path to a checkout of the org's repo.
    repo_path: String,
    /// Directory within it (default `docs/knowledge`).
    #[serde(default = "default_docs_dir")]
    docs_dir: String,
    /// Maintain AGENTS.md / CLAUDE.md pointer blocks at the repo root so
    /// coding agents consult the published pages with zero integration.
    /// OPT-IN for the git target (unlike `okf`, where it defaults on): this
    /// target may predate the pointer feature in an operator's repo, and a
    /// publisher that starts writing new files at the repo root unasked is
    /// exactly the surprise this module's header promises not to spring.
    #[serde(default)]
    agent_pointers: bool,
}

fn default_docs_dir() -> String {
    "docs/knowledge".into()
}

pub struct GitPublisher {
    root: PathBuf,
    repo_root: PathBuf,
    docs_dir: String,
    agent_pointers: bool,
}

impl GitPublisher {
    pub fn from_config(config: &serde_json::Value) -> Result<Self> {
        let cfg: GitConfig = serde_json::from_value(config.clone()).context("git target config")?;
        let repo_root = PathBuf::from(&cfg.repo_path);
        Ok(Self {
            root: repo_root.join(&cfg.docs_dir),
            repo_root,
            docs_dir: cfg.docs_dir,
            agent_pointers: cfg.agent_pointers,
        })
    }
}

#[async_trait]
impl Publisher for GitPublisher {
    fn kind(&self) -> &'static str {
        "git"
    }

    async fn publish(&self, page: &PageToPublish<'_>) -> Result<Option<String>> {
        // The slug comes from our own database and is constrained by the schema,
        // but a path is a path: refuse anything that could climb out of the docs
        // directory rather than trusting that it never will.
        let slug = &page.document.slug;
        anyhow::ensure!(
            !slug.contains("..") && !slug.contains('/') && !slug.contains('\\'),
            "refusing to write a page whose slug is not a bare filename: {slug}"
        );

        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let path = self.root.join(format!("{slug}.md"));
        std::fs::write(&path, page.markdown)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(Some(path.to_string_lossy().to_string()))
    }

    async fn finish(&self, bundle: &BundleToPublish<'_>) -> Result<()> {
        if self.agent_pointers {
            pointer::write_pointer_files(
                &self.repo_root,
                &self.docs_dir,
                bundle.console_url,
                false,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainiac_core::{DocKind, DocStatus, Document, Visibility};
    use uuid::Uuid;

    fn doc(slug: &str) -> Document {
        Document {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            team_id: None,
            slug: slug.into(),
            title: "T".into(),
            visibility: Visibility::Org,
            doc_kind: DocKind::TopicPage,
            status: DocStatus::Published,
            current_revision: None,
            project_id: None,
            dirty_at: None,
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn writes_the_page_as_markdown() {
        let tmp = std::env::temp_dir().join(format!("brainiac-git-{}", Uuid::new_v4()));
        let p = GitPublisher::from_config(&serde_json::json!({
            "repo_path": tmp.to_string_lossy(), "docs_dir": "docs"
        }))
        .expect("config");
        let d = doc("psp-gateway");
        let out = p
            .publish(&PageToPublish {
                document: &d,
                markdown: "# psp-gateway\n\nhello\n",
                external_ref: None,
                meta: &crate::PageMeta::default(),
            })
            .await
            .expect("publish");
        let path = out.expect("path");
        assert!(std::fs::read_to_string(&path)
            .expect("read")
            .contains("hello"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn refuses_a_slug_that_could_escape_the_docs_directory() {
        let tmp = std::env::temp_dir().join(format!("brainiac-git-{}", Uuid::new_v4()));
        let p =
            GitPublisher::from_config(&serde_json::json!({ "repo_path": tmp.to_string_lossy() }))
                .expect("config");
        let d = doc("../../.ssh/authorized_keys");
        assert!(p
            .publish(&PageToPublish {
                document: &d,
                markdown: "x",
                external_ref: None,
                meta: &crate::PageMeta::default(),
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn pointer_files_are_opt_in_and_written_only_when_asked() {
        let tmp = std::env::temp_dir().join(format!("brainiac-git-{}", Uuid::new_v4()));
        let bundle = crate::BundleToPublish {
            console_url: "https://console.test",
            entries: &[],
        };

        // Default: the repo root is left alone.
        let quiet =
            GitPublisher::from_config(&serde_json::json!({ "repo_path": tmp.to_string_lossy() }))
                .expect("config");
        quiet.finish(&bundle).await.expect("finish");
        assert!(!tmp.join("AGENTS.md").exists());

        // Opted in: both pointer files appear, pointing at the docs dir.
        let loud = GitPublisher::from_config(&serde_json::json!({
            "repo_path": tmp.to_string_lossy(), "agent_pointers": true
        }))
        .expect("config");
        loud.finish(&bundle).await.expect("finish");
        let agents = std::fs::read_to_string(tmp.join("AGENTS.md")).expect("AGENTS.md");
        assert!(agents.contains("docs/knowledge"), "{agents}");
        assert!(tmp.join("CLAUDE.md").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
