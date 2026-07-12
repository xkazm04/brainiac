//! JSON Schema export for the fixture YAML files — generated from the serde
//! structs in [`crate::schema`] via schemars, so the editor-facing contract
//! can never drift from what the loader actually parses.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use schemars::{schema_for, Schema};

use crate::schema::*;

/// (output file, fixture file it validates, schema)
fn schemas() -> Vec<(&'static str, &'static str, Schema)> {
    vec![
        ("org.schema.json", "org.yaml", schema_for!(OrgFile)),
        (
            "entities.schema.json",
            "entities/entities.yaml",
            schema_for!(EntitiesFile),
        ),
        (
            "merges.schema.json",
            "entities/merges.yaml",
            schema_for!(MergesFile),
        ),
        (
            "memories.schema.json",
            "memories/gold.yaml",
            schema_for!(MemoriesFile),
        ),
        (
            "transcript.schema.json",
            "transcripts/*.yaml",
            schema_for!(TranscriptFx),
        ),
        (
            "contradictions.schema.json",
            "contradictions/cases.yaml",
            schema_for!(ContradictionsFile),
        ),
        (
            "temporal.schema.json",
            "temporal/asof.yaml",
            schema_for!(TemporalFile),
        ),
        ("qa.schema.json", "retrieval/qa.yaml", schema_for!(QaFile)),
        (
            "leak.schema.json",
            "retrieval/leak.yaml",
            schema_for!(LeakFile),
        ),
    ]
}

/// Write one JSON Schema per fixture file into `dir` plus a README wiring
/// them up for yaml-language-server. Returns the files written.
pub fn export_schemas(dir: &Path) -> Result<Vec<String>> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut written = Vec::new();
    let mut readme = String::from(
        "# Fixture YAML schemas\n\n\
         Generated from the serde structs in `crates/brainiac-fixtures/src/schema.rs`\n\
         (`brainiac fixtures schema`). Do not edit by hand — regenerate after any\n\
         schema.rs change.\n\n\
         Editor validation: either add a modeline as the first line of a fixture\n\
         file, e.g.\n\n\
         ```yaml\n\
         # yaml-language-server: $schema=../../schema/qa.schema.json\n\
         ```\n\n\
         or map globs once in VS Code settings:\n\n\
         ```json\n\
         \"yaml.schemas\": {\n",
    );
    for (out_name, target, schema) in schemas() {
        let path = dir.join(out_name);
        let json = serde_json::to_string_pretty(&schema)?;
        fs::write(&path, json + "\n").with_context(|| format!("writing {}", path.display()))?;
        readme.push_str(&format!(
            "  \"fixtures/schema/{out_name}\": \"fixtures/v1/{target}\",\n"
        ));
        written.push(out_name.to_string());
    }
    readme.push_str("}\n```\n");
    fs::write(dir.join("README.md"), readme)?;
    written.push("README.md".into());
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every schema must serialize and reject an obviously-wrong document —
    /// guards against a derive silently degenerating to `true` (accept-all).
    #[test]
    fn schemas_are_nontrivial() {
        for (name, _, schema) in schemas() {
            let v = serde_json::to_value(&schema).expect("serialize");
            assert!(
                v.get("properties").is_some() || v.get("$ref").is_some(),
                "schema {name} looks degenerate: {v}"
            );
        }
    }
}
