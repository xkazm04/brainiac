//! OKF bundle harvest: someone else's wiki as an extraction SOURCE (Level 2).
//!
//! OpenWiki and a growing set of tools maintain OKF bundles (markdown + YAML
//! frontmatter) inside repos. Those wikis carry real knowledge our extraction
//! pipeline never sees. This module reads a bundle directory and feeds each
//! concept document into the SAME pipeline a transcript takes: extraction →
//! candidate memories → review gate. Never direct-to-canonical — a repo wiki
//! is a witness, not an authority, and a named human still signs what the org
//! ends up believing.
//!
//! Two refusals define the harvest:
//!
//! 1. **Our own projections are not evidence.** A page Brainiac itself
//!    published (recognizable by its `x_brainiac_*` frontmatter) is composed
//!    FROM memories; harvesting it back would launder model prose into new
//!    candidate memories and close a self-citation loop. Skipped, loudly.
//! 2. **Idempotent by content.** Each file becomes a source keyed by its path
//!    and content hash, so re-running a harvest re-ingests only what changed —
//!    every duplicate source would burn a full extraction LLM call.

use anyhow::{Context, Result};
use brainiac_store::Store;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A harvest ingests at most this many files — a bundle bigger than this is a
/// misconfiguration (someone pointed the harvest at a repo root), not a wiki.
pub const MAX_FILES: usize = 500;
/// Per-file size cap. A concept document is prose; anything larger is a data
/// file wearing a `.md` extension.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// One concept document, parsed leniently per the OKF v0.1 conformance rules:
/// unknown types tolerated, unknown fields ignored, missing optionals fine.
#[derive(Debug, Clone, PartialEq)]
pub struct OkfDoc {
    /// Path relative to the bundle root, forward-slashed — the stable identity.
    pub rel_path: String,
    /// OKF `type` (required by the spec; `None` tolerated on ingest).
    pub okf_type: Option<String>,
    /// Frontmatter `title`, or the filename stem when absent (per spec).
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// OKF `resource` — the URI of the asset the doc describes.
    pub resource: Option<String>,
    /// The markdown body after the frontmatter block.
    pub body: String,
    /// The doc carries `x_brainiac_*` frontmatter — it is one of OUR published
    /// pages and must not be harvested back.
    pub brainiac_origin: bool,
}

pub struct HarvestStats {
    /// Concept files seen (after reserved/hidden/non-md filtering).
    pub files: usize,
    /// New or changed → source inserted and queued for extraction.
    pub ingested: usize,
    /// Content unchanged since a prior harvest (idempotency key hit).
    pub unchanged: usize,
    /// Our own published pages, refused (see module docs).
    pub own_pages: usize,
    /// Unreadable / oversized / unparseable, skipped with a warning.
    pub invalid: usize,
}

/// Minimal YAML frontmatter reader for the OKF subset: `key: value` scalars
/// and block lists. Deliberately not a YAML parser — the fields OKF recommends
/// are flat, and a full YAML dependency for five keys would be surface area
/// without payoff. Unknown keys are read past, per the spec's "consumers must
/// tolerate unknown fields".
fn parse_frontmatter(text: &str) -> (Vec<(String, Vec<String>)>, &str) {
    let Some(rest) = text.strip_prefix("---") else {
        return (Vec::new(), text);
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), text);
    };
    let (fm, body) = (&rest[..end], &rest[end + 4..]);
    let body = body.strip_prefix('\n').unwrap_or(body);

    let mut fields: Vec<(String, Vec<String>)> = Vec::new();
    for line in fm.lines() {
        if let Some(item) = line
            .strip_prefix("  - ")
            .or_else(|| line.strip_prefix("- "))
        {
            if let Some(last) = fields.last_mut() {
                last.1.push(unquote(item));
            }
        } else if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim();
            if key.is_empty() || key.starts_with('#') {
                continue;
            }
            let values = if value.is_empty() {
                Vec::new() // a list header, or an empty scalar
            } else {
                vec![unquote(value)]
            };
            fields.push((key, values));
        }
    }
    (fields, body)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

fn scalar(fields: &[(String, Vec<String>)], key: &str) -> Option<String> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.first())
        .filter(|v| !v.is_empty())
        .cloned()
}

/// Parse one concept file. `rel_path` is bundle-relative with forward slashes.
pub fn parse_doc(rel_path: &str, text: &str) -> OkfDoc {
    let (fields, body) = parse_frontmatter(text);
    let stem = rel_path
        .rsplit('/')
        .next()
        .unwrap_or(rel_path)
        .trim_end_matches(".md")
        .to_string();
    OkfDoc {
        rel_path: rel_path.to_string(),
        okf_type: scalar(&fields, "type"),
        title: scalar(&fields, "title").unwrap_or(stem),
        description: scalar(&fields, "description"),
        tags: fields
            .iter()
            .find(|(k, _)| k == "tags")
            .map(|(_, v)| v.clone())
            .unwrap_or_default(),
        resource: scalar(&fields, "resource"),
        body: body.trim().to_string(),
        brainiac_origin: fields.iter().any(|(k, _)| k.starts_with("x_brainiac_")),
    }
}

/// Frame a doc for the extractor the way `doc_edit` frames a human's edit: as
/// a witness statement about the world, with enough context (title, type,
/// tags, provenance) that extracted candidates carry where they came from.
pub fn source_text(doc: &OkfDoc) -> String {
    let mut s = format!(
        "A document from the repository's knowledge wiki (OKF bundle), \"{}\"{}, at `{}`, states:\n\n{}\n",
        doc.title,
        doc.okf_type
            .as_deref()
            .map(|t| format!(" (type: {t})"))
            .unwrap_or_default(),
        doc.rel_path,
        doc.body
    );
    if !doc.tags.is_empty() {
        s.push_str(&format!("\nDocument tags: {}.", doc.tags.join(", ")));
    }
    if let Some(r) = &doc.resource {
        s.push_str(&format!("\nThe document describes the resource: {r}"));
    }
    s
}

/// FNV-1a 64 — a stable content fingerprint for the idempotency key. Stability
/// across processes and Rust versions is the requirement (std's DefaultHasher
/// guarantees neither); collision resistance beyond change-detection is not.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn idempotency_key(rel_path: &str, text: &str) -> String {
    format!("okf:{rel_path}#{:016x}", fnv1a64(text.as_bytes()))
}

/// Walk a bundle directory for concept files: `.md`, minus the reserved
/// `index.md` / `log.md` at every level, minus hidden entries. Recurses, and
/// refuses to follow the crossing of [`MAX_FILES`] — the cap is a symptom of
/// pointing the harvest at the wrong directory, and half-ingesting a repo
/// would be worse than stopping.
fn concept_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .with_context(|| format!("reading bundle dir {}", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !name.ends_with(".md") || name == "index.md" || name == "log.md" {
                continue;
            }
            out.push(path);
            anyhow::ensure!(
                out.len() <= MAX_FILES,
                "bundle has more than {MAX_FILES} concept files — is `{}` really a bundle?",
                root.display()
            );
        }
    }
    out.sort();
    Ok(out)
}

/// Harvest a bundle directory into the extraction pipeline. Sources land under
/// `principal`'s RLS scope with kind `okf`; extraction and the review gate do
/// the rest. Returns per-file accounting — a harvest that silently skipped
/// half a bundle would read as "covered", so nothing here is silent.
pub async fn harvest(
    store: &Store,
    principal: &brainiac_core::Principal,
    team_id: Option<Uuid>,
    bundle_dir: &Path,
    recorded_by: Option<Uuid>,
) -> Result<HarvestStats> {
    let mut stats = HarvestStats {
        files: 0,
        ingested: 0,
        unchanged: 0,
        own_pages: 0,
        invalid: 0,
    };
    let files = concept_files(bundle_dir)?;
    let mut queued: Vec<Uuid> = Vec::new();

    let mut tx = store.scoped_tx(principal).await?;
    for path in &files {
        stats.files += 1;
        let rel = path
            .strip_prefix(bundle_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let meta = std::fs::metadata(path)?;
        if meta.len() > MAX_FILE_BYTES {
            tracing::warn!(file = %rel, bytes = meta.len(), "okf harvest: file too large, skipped");
            stats.invalid += 1;
            continue;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(file = %rel, error = %e, "okf harvest: unreadable, skipped");
                stats.invalid += 1;
                continue;
            }
        };
        let doc = parse_doc(&rel, &text);
        if doc.brainiac_origin {
            // Our own projection coming back as "evidence" — the loop we refuse.
            stats.own_pages += 1;
            continue;
        }
        if doc.body.is_empty() {
            stats.invalid += 1;
            continue;
        }

        let source_id = Uuid::new_v4();
        let inserted = brainiac_store::governance::insert_source_idempotent(
            &mut tx,
            source_id,
            principal.org_id,
            team_id,
            "okf",
            &source_text(&doc),
            recorded_by,
            &idempotency_key(&rel, &text),
            // External-wiki evidence is org-shared; no project claims it.
            None,
        )
        .await?;
        match inserted {
            Some(id) => {
                queued.push(id);
                stats.ingested += 1;
            }
            None => stats.unchanged += 1,
        }
    }
    tx.commit().await?;

    // Enqueue AFTER the sources committed: a queued job racing an uncommitted
    // source row would extract nothing.
    for id in &queued {
        crate::worker::enqueue_source(store, principal.org_id, *id).await?;
    }
    tracing::info!(
        bundle = %bundle_dir.display(),
        files = stats.files,
        ingested = stats.ingested,
        unchanged = stats.unchanged,
        own_pages = stats.own_pages,
        invalid = stats.invalid,
        "okf bundle harvested"
    );
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parses_the_okf_subset_and_tolerates_unknowns() {
        let text = "---\n\
            type: Runbook\n\
            title: \"PSP \\\"Gateway\\\"\"\n\
            description: How retries work.\n\
            unknown_field: ignored\n\
            tags:\n  - \"payments\"\n  - infra\n\
            resource: https://example.test/psp\n\
            ---\n\nThe body.\n";
        let doc = parse_doc("runbooks/psp.md", text);
        assert_eq!(doc.okf_type.as_deref(), Some("Runbook"));
        assert_eq!(doc.title, "PSP \"Gateway\"");
        assert_eq!(doc.description.as_deref(), Some("How retries work."));
        assert_eq!(doc.tags, vec!["payments", "infra"]);
        assert_eq!(doc.resource.as_deref(), Some("https://example.test/psp"));
        assert_eq!(doc.body, "The body.");
        assert!(!doc.brainiac_origin);
    }

    #[test]
    fn a_file_without_frontmatter_is_tolerated_and_titled_by_filename() {
        let doc = parse_doc("notes/deploy-notes.md", "Just prose, no frontmatter.");
        assert_eq!(doc.title, "deploy-notes");
        assert_eq!(doc.okf_type, None);
        assert_eq!(doc.body, "Just prose, no frontmatter.");
    }

    #[test]
    fn our_own_published_pages_are_recognized_and_refused() {
        let text = "---\ntype: Runbook\ntitle: \"X\"\nx_brainiac_policy: \"auto_published\"\n---\n\nComposed text.\n";
        assert!(parse_doc("x.md", text).brainiac_origin);
    }

    #[test]
    fn the_idempotency_key_moves_with_content_and_only_content() {
        let a = idempotency_key("a.md", "one");
        assert_eq!(a, idempotency_key("a.md", "one"), "stable across runs");
        assert_ne!(a, idempotency_key("a.md", "two"), "content change re-keys");
        assert_ne!(
            a,
            idempotency_key("b.md", "one"),
            "path is part of identity"
        );
    }

    #[test]
    fn source_text_frames_the_doc_as_a_witness_with_provenance() {
        let doc = parse_doc(
            "runbooks/psp.md",
            "---\ntype: Runbook\ntitle: PSP\ntags:\n  - payments\n---\n\nRetries cap at 30s.\n",
        );
        let s = source_text(&doc);
        assert!(s.contains("knowledge wiki"));
        assert!(s.contains("`runbooks/psp.md`"));
        assert!(s.contains("Retries cap at 30s."));
        assert!(s.contains("Document tags: payments."));
    }

    #[test]
    fn reserved_and_hidden_files_are_not_concepts() {
        let tmp = std::env::temp_dir().join(format!("brainiac-okf-ingest-{}", Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("sub")).expect("mkdir");
        for (name, content) in [
            ("index.md", "listing"),
            ("log.md", "history"),
            (".hidden.md", "x"),
            ("real.md", "concept"),
            ("sub/nested.md", "concept"),
            ("sub/index.md", "listing"),
            ("data.json", "{}"),
        ] {
            std::fs::write(tmp.join(name), content).expect("write");
        }
        let files = concept_files(&tmp).expect("walk");
        let names: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&tmp)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(names, vec!["real.md", "sub/nested.md"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
