//! Agent pointer files: `AGENTS.md` and `CLAUDE.md` managed blocks.
//!
//! The distribution trick that makes a repo-resident bundle actually get read:
//! coding agents already look for these two files at the repo root, so a short
//! managed block there means every agent consults the published knowledge with
//! zero integration — no MCP wiring, no prompt engineering by the operator.
//!
//! Only the text between our markers is ours. Everything a human wrote around
//! it is preserved byte-for-byte, because a tool that rewrites someone's
//! CLAUDE.md is a tool that gets uninstalled.

use anyhow::{Context, Result};
use std::path::Path;

pub const START: &str =
    "<!-- BRAINIAC:START — managed block, do not edit; your text outside it is preserved -->";
pub const END: &str = "<!-- BRAINIAC:END -->";

/// The block body: what an agent needs to know to USE the bundle, in the
/// imperative voice agents actually follow.
pub fn block_body(docs_dir: &str, console_url: &str, okf: bool) -> String {
    let retrieval = if okf {
        format!(
            "- Start at `{docs_dir}/index.md`. Every page carries YAML frontmatter \
             (`type`, `tags`, `description` — OKF) — filter on it instead of reading \
             every file.\n"
        )
    } else {
        format!("- Start at `{docs_dir}/` — one markdown file per page.\n")
    };
    format!(
        "## Organization knowledge base (Brainiac)\n\n\
         This repository receives a generated knowledge bundle at `{docs_dir}/`, \
         compiled from the organization's governed memory store. Consult it before \
         answering questions about services, architecture decisions, or team \
         practices — it is the org's settled, human-reviewed account.\n\n\
         {retrieval}\
         - Claims carry `[m:<uuid>]` citations tracing to a governed memory; treat \
         an uncited claim as unverified.\n\
         - NEVER edit files under `{docs_dir}/` — pages regenerate automatically and \
         edits are overwritten. Propose changes in the console instead: {console_url}\n\
         - A page whose banner says \"Verification pending\" is deliberately held at \
         an older version; weigh it accordingly.\n"
    )
}

/// Insert or replace the managed block in `existing`, preserving everything
/// outside the markers. With no prior block, the block is appended after the
/// existing content (their file, their top).
pub fn upsert_block(existing: Option<&str>, body: &str) -> String {
    let block = format!("{START}\n{body}{END}\n");
    match existing {
        None => block,
        Some(text) => match (text.find(START), text.find(END)) {
            (Some(s), Some(e)) if e >= s => {
                let after = &text[e + END.len()..];
                // Swallow the single newline our own block writes after END so
                // repeated upserts do not grow a ladder of blank lines.
                let after = after.strip_prefix('\n').unwrap_or(after);
                format!("{}{block}{}", &text[..s], after)
            }
            // A mangled or missing block: append, never guess at surgery.
            _ => {
                let sep = if text.is_empty() || text.ends_with("\n\n") {
                    ""
                } else if text.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                format!("{text}{sep}{block}")
            }
        },
    }
}

/// Maintain the pointer block in `AGENTS.md` and `CLAUDE.md` at the repo root.
pub fn write_pointer_files(
    repo_root: &Path,
    docs_dir: &str,
    console_url: &str,
    okf: bool,
) -> Result<()> {
    let body = block_body(docs_dir, console_url, okf);
    std::fs::create_dir_all(repo_root)
        .with_context(|| format!("creating {}", repo_root.display()))?;
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = repo_root.join(name);
        let existing = std::fs::read_to_string(&path).ok();
        let updated = upsert_block(existing.as_deref(), &body);
        std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_file_is_just_the_block() {
        let out = upsert_block(None, "content\n");
        assert!(out.starts_with(START));
        assert!(out.ends_with(&format!("{END}\n")));
    }

    #[test]
    fn user_content_outside_the_block_survives_an_update() {
        let v1 = upsert_block(Some("# My repo\n\nMy own notes.\n"), "old pointer\n");
        assert!(v1.contains("My own notes."));
        let v2 = upsert_block(Some(&v1), "new pointer\n");
        assert!(v2.contains("# My repo"));
        assert!(v2.contains("My own notes."));
        assert!(v2.contains("new pointer"));
        assert!(!v2.contains("old pointer"));
    }

    #[test]
    fn repeated_upserts_do_not_grow_the_file() {
        let base = upsert_block(Some("intro\n"), "pointer\n");
        let again = upsert_block(Some(&base), "pointer\n");
        assert_eq!(base, again);
    }

    #[test]
    fn content_after_the_block_is_preserved() {
        let file = format!("before\n\n{START}\nold\n{END}\n\nafter\n");
        let out = upsert_block(Some(&file), "new\n");
        assert!(out.starts_with("before\n"));
        assert!(out.contains("\nafter\n"), "{out}");
        assert!(out.contains("new"));
        assert!(!out.contains("old\n"));
    }

    #[test]
    fn the_body_tells_an_agent_the_rules_that_matter() {
        let b = block_body("docs/okf", "https://console.test", true);
        assert!(b.contains("NEVER edit"));
        assert!(b.contains("[m:<uuid>]"));
        assert!(b.contains("docs/okf/index.md"));
        assert!(b.contains("https://console.test"));
    }
}
