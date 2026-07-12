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
