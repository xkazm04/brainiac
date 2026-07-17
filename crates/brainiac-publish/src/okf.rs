//! The OKF target: the bundle as an Open Knowledge Format v0.1 directory.
//!
//! OKF (GoogleCloudPlatform/knowledge-catalog) is a minimal open convention for
//! LLM-readable wikis: markdown concepts with YAML frontmatter, an `index.md`
//! directory listing, a `log.md` changelog. OpenWiki and a growing set of
//! agent tools consume it. Publishing our pages in it is squarely this crate's
//! thesis — the bundle is one more RENDER TARGET kept honest, never a second
//! source of truth: memory stays canonical, pages regenerate, direct edits to
//! the bundle are overwritten.
//!
//! What makes a Brainiac bundle different from any other OKF producer's is the
//! extension frontmatter (`x_brainiac_*`): the exact memory ids a page was
//! compiled from, the policy decision that let it go live, and whether it is
//! currently stale. OKF consumers must preserve unknown fields (spec §consumers),
//! so our provenance survives any conformant tool — the only OKF bundles whose
//! claims arrive evidence-graded.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;

use crate::{pointer, BundleToPublish, PageToPublish, Publisher};

#[derive(Debug, Deserialize)]
struct OkfConfig {
    /// Path to a checkout of the org's repo (or any directory to own).
    repo_path: String,
    /// Bundle directory within it (default `docs/okf`).
    #[serde(default = "default_bundle_dir")]
    docs_dir: String,
    /// Maintain AGENTS.md / CLAUDE.md pointer blocks at the repo root so
    /// coding agents find the bundle with zero integration. Default ON — the
    /// pointer is most of the reason to publish OKF into a repo. Set `false`
    /// for a bundle that is consumed some other way.
    #[serde(default = "default_true")]
    agent_pointers: bool,
    /// Heading for the bundle's index.md (default "Knowledge bundle").
    #[serde(default)]
    bundle_title: Option<String>,
}

fn default_bundle_dir() -> String {
    "docs/okf".into()
}
fn default_true() -> bool {
    true
}

pub struct OkfPublisher {
    /// The bundle directory (repo_path/docs_dir).
    root: PathBuf,
    /// The repo checkout itself — where the pointer files live.
    repo_root: PathBuf,
    docs_dir: String,
    agent_pointers: bool,
    bundle_title: Option<String>,
}

impl OkfPublisher {
    pub fn from_config(config: &serde_json::Value) -> Result<Self> {
        let cfg: OkfConfig = serde_json::from_value(config.clone()).context("okf target config")?;
        let repo_root = PathBuf::from(&cfg.repo_path);
        Ok(Self {
            root: repo_root.join(&cfg.docs_dir),
            repo_root,
            docs_dir: cfg.docs_dir,
            agent_pointers: cfg.agent_pointers,
            bundle_title: cfg.bundle_title,
        })
    }
}

/// `doc_kind` → the OKF `type` string (a short human-readable concept name,
/// per spec examples like "BigQuery Table", "Playbook"). Unknown kinds — a
/// future variant this match has not met — degrade to "Page", because a
/// consumer must tolerate unknown types but we should still emit one.
fn okf_type(kind: &str) -> &'static str {
    match kind {
        "entity_page" => "Entity Page",
        "topic_page" => "Topic Page",
        "runbook" => "Runbook",
        "onboarding" => "Onboarding Guide",
        "digest" => "Digest",
        "standards_page" => "Coding Standards",
        _ => "Page",
    }
}

/// Section heading for a kind in index.md.
fn index_heading(kind: &str) -> &'static str {
    match kind {
        "entity_page" => "Services & systems",
        "topic_page" => "Topics",
        "runbook" => "Runbooks",
        "onboarding" => "Onboarding",
        "digest" => "Digests",
        "standards_page" => "Coding standards",
        _ => "Pages",
    }
}

/// Quote a free-text value for YAML: double-quoted, backslash and quote
/// escaped, newlines collapsed. Total — any model-written title stays a string.
fn yaml_quote(s: &str) -> String {
    let cleaned = s.replace(['\n', '\r'], " ");
    format!("\"{}\"", cleaned.replace('\\', "\\\\").replace('"', "\\\""))
}

fn frontmatter(page: &PageToPublish<'_>) -> String {
    let doc = page.document;
    let meta = page.meta;
    let mut fm = String::from("---\n");
    fm.push_str(&format!("type: {}\n", okf_type(doc.doc_kind.as_str())));
    fm.push_str(&format!("title: {}\n", yaml_quote(&doc.title)));
    if let Some(d) = &meta.description {
        fm.push_str(&format!("description: {}\n", yaml_quote(d)));
    }
    if let Some(r) = &meta.resource {
        fm.push_str(&format!("resource: {}\n", yaml_quote(r)));
    }
    if !meta.tags.is_empty() {
        fm.push_str("tags:\n");
        for t in &meta.tags {
            fm.push_str(&format!("  - {}\n", yaml_quote(t)));
        }
    }
    if let Some(ts) = &meta.timestamp {
        fm.push_str(&format!("timestamp: {}\n", yaml_quote(ts)));
    }
    // The extension fields — the part no other producer ships. Consumers must
    // preserve unknown frontmatter (OKF spec), so provenance survives the trip.
    if !meta.policy_decision.is_empty() {
        fm.push_str(&format!(
            "x_brainiac_policy: {}\n",
            yaml_quote(&meta.policy_decision)
        ));
    }
    fm.push_str(&format!("x_brainiac_stale: {}\n", meta.stale));
    if !meta.cited_memories.is_empty() {
        fm.push_str("x_brainiac_cited_memories:\n");
        for id in &meta.cited_memories {
            fm.push_str(&format!("  - {id}\n"));
        }
    }
    fm.push_str("---\n\n");
    fm
}

#[async_trait]
impl Publisher for OkfPublisher {
    fn kind(&self) -> &'static str {
        "okf"
    }

    async fn publish(&self, page: &PageToPublish<'_>) -> Result<Option<String>> {
        // Same refusal as the git target: a slug is a filename, never a path.
        let slug = &page.document.slug;
        anyhow::ensure!(
            !slug.contains("..") && !slug.contains('/') && !slug.contains('\\'),
            "refusing to write a page whose slug is not a bare filename: {slug}"
        );

        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let path = self.root.join(format!("{slug}.md"));
        let contents = format!("{}{}", frontmatter(page), page.markdown);
        std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
        Ok(Some(path.to_string_lossy().to_string()))
    }

    async fn finish(&self, bundle: &BundleToPublish<'_>) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;

        // ── index.md — the bundle's front door, regenerated whole. Root
        // index.md is the ONE index allowed frontmatter, and only okf_version.
        let mut index = String::from("---\nokf_version: \"0.1\"\n---\n");
        index.push_str(&format!(
            "# {}\n\n",
            self.bundle_title.as_deref().unwrap_or("Knowledge bundle")
        ));
        index.push_str(&format!(
            "Generated by Brainiac from the organization's governed memories — do \
             not edit here. Every page's frontmatter carries the memory ids it was \
             compiled from; trace any claim in the console: {}\n",
            bundle.console_url
        ));
        // Stable group order: the reader-facing kinds first, digests last.
        const KIND_ORDER: [&str; 6] = [
            "entity_page",
            "topic_page",
            "runbook",
            "onboarding",
            "standards_page",
            "digest",
        ];
        let mut kinds: Vec<&str> = bundle.entries.iter().map(|e| e.doc_kind.as_str()).collect();
        kinds.sort_by_key(|k| {
            KIND_ORDER
                .iter()
                .position(|o| o == k)
                .unwrap_or(KIND_ORDER.len())
        });
        kinds.dedup();
        for kind in kinds {
            index.push_str(&format!("\n# {}\n", index_heading(kind)));
            for e in bundle.entries.iter().filter(|e| e.doc_kind == kind) {
                match &e.description {
                    Some(d) => index.push_str(&format!("* [{}]({}.md) - {}\n", e.title, e.slug, d)),
                    None => index.push_str(&format!("* [{}]({}.md)\n", e.title, e.slug)),
                }
            }
        }
        std::fs::write(self.root.join("index.md"), index).context("writing index.md")?;

        // ── log.md — the bundle changelog, date-grouped, newest first.
        let mut entries: Vec<&crate::LogEntry> =
            bundle.entries.iter().flat_map(|e| e.log.iter()).collect();
        entries.sort_by(|a, b| b.date.cmp(&a.date));
        entries.truncate(100);
        let mut log = String::new();
        let mut current_date = "";
        for e in &entries {
            if e.date != current_date {
                if !log.is_empty() {
                    log.push('\n');
                }
                log.push_str(&format!("## {}\n", e.date));
                current_date = &e.date;
            }
            let why = match e.trigger.as_str() {
                "memory_change" => "recomposed after a memory change",
                "manual" => "manually recomposed",
                "schedule" => "recomposed on schedule",
                other => other,
            };
            let outcome = match e.policy.as_str() {
                "auto_published" => "auto-published",
                "needs_review" => "held for human review",
                other => other,
            };
            log.push_str(&format!(
                "* **Update**: {} — {why}, {outcome}\n",
                e.page_title
            ));
        }
        std::fs::write(self.root.join("log.md"), log).context("writing log.md")?;

        // ── the pointer files — the reason a repo bundle actually gets read.
        if self.agent_pointers {
            pointer::write_pointer_files(
                &self.repo_root,
                &self.docs_dir,
                bundle.console_url,
                true,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BundleEntry, LogEntry, PageMeta};
    use brainiac_core::{DocKind, DocStatus, Document, Visibility};
    use uuid::Uuid;

    fn doc(slug: &str, kind: DocKind) -> Document {
        Document {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            team_id: None,
            slug: slug.into(),
            title: "PSP \"Gateway\"".into(),
            visibility: Visibility::Org,
            doc_kind: kind,
            status: DocStatus::Published,
            current_revision: None,
            project_id: None,
            dirty_at: None,
            updated_at: chrono::Utc::now(),
        }
    }

    fn publisher(tmp: &std::path::Path) -> OkfPublisher {
        OkfPublisher::from_config(&serde_json::json!({
            "repo_path": tmp.to_string_lossy(),
        }))
        .expect("config")
    }

    #[tokio::test]
    async fn a_page_lands_with_okf_frontmatter_and_provenance() {
        let tmp = std::env::temp_dir().join(format!("brainiac-okf-{}", Uuid::new_v4()));
        let p = publisher(&tmp);
        let d = doc("psp-gateway", DocKind::Runbook);
        let cited = Uuid::new_v4();
        let meta = PageMeta {
            description: Some("How the gateway retries.".into()),
            tags: vec!["PSP Gateway".into()],
            timestamp: Some("2026-07-17T00:00:00+00:00".into()),
            resource: Some("https://console.test/docs/psp-gateway".into()),
            cited_memories: vec![cited],
            policy_decision: "auto_published".into(),
            stale: false,
        };
        let out = p
            .publish(&PageToPublish {
                document: &d,
                markdown: "# PSP Gateway\n\nBody.\n",
                external_ref: None,
                meta: &meta,
            })
            .await
            .expect("publish")
            .expect("path");
        let contents = std::fs::read_to_string(&out).expect("read");
        assert!(contents.starts_with("---\ntype: Runbook\n"), "{contents}");
        // The quote in the title must be escaped, not break the YAML.
        assert!(
            contents.contains("title: \"PSP \\\"Gateway\\\"\""),
            "{contents}"
        );
        assert!(contents.contains("resource: \"https://console.test/docs/psp-gateway\""));
        assert!(contents.contains(&format!("  - {cited}")), "{contents}");
        assert!(contents.contains("x_brainiac_stale: false"));
        assert!(contents.contains("# PSP Gateway\n\nBody.\n"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn refuses_a_slug_that_could_escape_the_bundle() {
        let tmp = std::env::temp_dir().join(format!("brainiac-okf-{}", Uuid::new_v4()));
        let p = publisher(&tmp);
        let d = doc("../evil", DocKind::TopicPage);
        assert!(p
            .publish(&PageToPublish {
                document: &d,
                markdown: "x",
                external_ref: None,
                meta: &PageMeta::default(),
            })
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn bundle_entries() -> Vec<BundleEntry> {
        vec![
            BundleEntry {
                slug: "psp-gateway".into(),
                title: "PSP Gateway".into(),
                doc_kind: "entity_page".into(),
                description: Some("The org's payment gateway.".into()),
                log: vec![
                    LogEntry {
                        date: "2026-07-17".into(),
                        page_title: "PSP Gateway".into(),
                        trigger: "memory_change".into(),
                        policy: "auto_published".into(),
                    },
                    LogEntry {
                        date: "2026-07-15".into(),
                        page_title: "PSP Gateway".into(),
                        trigger: "manual".into(),
                        policy: "needs_review".into(),
                    },
                ],
            },
            BundleEntry {
                slug: "weekly".into(),
                title: "This week".into(),
                doc_kind: "digest".into(),
                description: None,
                log: vec![],
            },
        ]
    }

    #[tokio::test]
    async fn finish_writes_index_log_and_pointer_files() {
        let tmp = std::env::temp_dir().join(format!("brainiac-okf-{}", Uuid::new_v4()));
        let p = publisher(&tmp);
        let entries = bundle_entries();
        p.finish(&BundleToPublish {
            console_url: "https://console.test",
            entries: &entries,
        })
        .await
        .expect("finish");

        let index = std::fs::read_to_string(tmp.join("docs/okf/index.md")).expect("index");
        assert!(
            index.starts_with("---\nokf_version: \"0.1\"\n---\n"),
            "{index}"
        );
        assert!(index.contains("* [PSP Gateway](psp-gateway.md) - The org's payment gateway."));
        assert!(index.contains("# Services & systems"));
        // Digests group after the reader-facing kinds.
        assert!(
            index.find("# Services & systems") < index.find("# Digests"),
            "{index}"
        );

        let log = std::fs::read_to_string(tmp.join("docs/okf/log.md")).expect("log");
        assert!(log.starts_with("## 2026-07-17\n"), "{log}");
        assert!(log.contains("recomposed after a memory change, auto-published"));
        assert!(log.contains("## 2026-07-15"));
        assert!(log.contains("held for human review"));

        // The pointer files exist at the REPO root, not inside the bundle.
        let agents = std::fs::read_to_string(tmp.join("AGENTS.md")).expect("AGENTS.md");
        assert!(agents.contains("docs/okf/index.md"), "{agents}");
        assert!(std::fs::read_to_string(tmp.join("CLAUDE.md")).is_ok());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn pointer_files_can_be_switched_off() {
        let tmp = std::env::temp_dir().join(format!("brainiac-okf-{}", Uuid::new_v4()));
        let p = OkfPublisher::from_config(&serde_json::json!({
            "repo_path": tmp.to_string_lossy(),
            "agent_pointers": false,
        }))
        .expect("config");
        let entries = bundle_entries();
        p.finish(&BundleToPublish {
            console_url: "https://console.test",
            entries: &entries,
        })
        .await
        .expect("finish");
        assert!(!tmp.join("AGENTS.md").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
