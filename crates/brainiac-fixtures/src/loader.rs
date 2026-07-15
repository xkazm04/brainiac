//! Fixture tree loader. `load()` parses every file and immediately runs the
//! integrity validation — an invalid tree never reaches a caller.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::schema::*;
use crate::validate;

#[derive(Debug, Clone)]
pub struct Fixtures {
    pub root: PathBuf,
    pub org: OrgFile,
    pub entities: EntitiesFile,
    pub merges: MergesFile,
    pub memories: MemoriesFile,
    pub transcripts: Vec<TranscriptFx>,
    pub contradictions: ContradictionsFile,
    pub temporal: TemporalFile,
    pub qa: QaFile,
    pub leak: LeakFile,
    /// Composition gold (EVAL §2.6). Absent in older fixture trees — an empty
    /// list simply means the `docs` profile has nothing to score.
    pub documents: DocumentsFile,
    /// Docs-drift gold (Level 2). Same absence semantics as `documents`.
    pub drift: DriftFile,
}

fn read_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("reading fixture file {}", path.display()))?;
    serde_yaml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

/// Load and validate a fixture tree (e.g. `fixtures/v1`).
pub fn load(root: impl AsRef<Path>) -> Result<Fixtures> {
    let fixtures = load_unvalidated(root)?;
    let issues = validate::validate(&fixtures);
    if !issues.is_empty() {
        bail!(
            "fixture tree failed integrity validation ({} issue(s)):\n  - {}",
            issues.len(),
            issues.join("\n  - ")
        );
    }
    Ok(fixtures)
}

/// Parse a fixture tree WITHOUT the integrity bail — the `fixtures lint`
/// CLI wants every finding as a structured diagnostic, not one big error.
pub fn load_unvalidated(root: impl AsRef<Path>) -> Result<Fixtures> {
    let root = root.as_ref().to_path_buf();

    let transcripts_dir = root.join("transcripts");
    let mut transcripts: Vec<TranscriptFx> = Vec::new();
    let entries = fs::read_dir(&transcripts_dir)
        .with_context(|| format!("listing {}", transcripts_dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
        .collect();
    paths.sort();
    for p in paths {
        transcripts.push(read_yaml(&p)?);
    }

    let fixtures = Fixtures {
        org: read_yaml(&root.join("org.yaml"))?,
        entities: read_yaml(&root.join("entities/entities.yaml"))?,
        merges: read_yaml(&root.join("entities/merges.yaml"))?,
        memories: read_yaml(&root.join("memories/gold.yaml"))?,
        transcripts,
        contradictions: read_yaml(&root.join("contradictions/cases.yaml"))?,
        temporal: read_yaml(&root.join("temporal/asof.yaml"))?,
        qa: read_yaml(&root.join("retrieval/qa.yaml"))?,
        leak: read_yaml(&root.join("retrieval/leak.yaml"))?,
        documents: {
            // The `documents/` directory is the tree's declaration that it HAS a
            // docs profile. If it exists but pages.yaml doesn't (renamed, moved,
            // typo'd), loading an empty list would make every composition-gold
            // check AND the zero-tolerance leak gate iterate zero items and report
            // green — a vacuous pass, which validate.rs itself calls "worse than
            // having no gate at all, because it reports safety it never verified".
            // Only a tree with no `documents/` at all legitimately has no profile.
            let dir = root.join("documents");
            let p = dir.join("pages.yaml");
            if p.exists() {
                read_yaml(&p)?
            } else if dir.exists() {
                bail!(
                    "fixtures: {} exists but pages.yaml is missing — the composition \
                     and leak gates would validate vacuously. Restore the file, or \
                     remove the directory if this tree has no docs profile.",
                    dir.display()
                );
            } else {
                DocumentsFile { documents: vec![] }
            }
        },
        drift: {
            // Same vacuous-pass refusal as `documents/`: a present-but-empty
            // drift directory means someone moved the gold, not that the tree
            // has no drift profile.
            let dir = root.join("drift");
            let p = dir.join("docs.yaml");
            if p.exists() {
                read_yaml(&p)?
            } else if dir.exists() {
                bail!(
                    "fixtures: {} exists but docs.yaml is missing — the drift profile \
                     would score zero documents and report green. Restore the file, or \
                     remove the directory if this tree has no drift gold.",
                    dir.display()
                );
            } else {
                DriftFile::default()
            }
        },
        root,
    };
    Ok(fixtures)
}

/// Resolve the repo-relative default fixture root, tolerant of being invoked
/// from a crate directory (tests) or the workspace root (CLI).
pub fn default_root() -> PathBuf {
    let candidates = [
        PathBuf::from("fixtures/v1"),
        PathBuf::from("../../fixtures/v1"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/v1"),
    ];
    for c in &candidates {
        if c.join("org.yaml").exists() {
            return c.clone();
        }
    }
    PathBuf::from("fixtures/v1")
}
