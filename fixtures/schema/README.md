# Fixture YAML schemas

Generated from the serde structs in `crates/brainiac-fixtures/src/schema.rs`
(`brainiac fixtures schema`). Do not edit by hand — regenerate after any
schema.rs change.

Editor validation: either add a modeline as the first line of a fixture
file, e.g.

```yaml
# yaml-language-server: $schema=../../schema/qa.schema.json
```

or map globs once in VS Code settings:

```json
"yaml.schemas": {
  "fixtures/schema/org.schema.json": "fixtures/v1/org.yaml",
  "fixtures/schema/entities.schema.json": "fixtures/v1/entities/entities.yaml",
  "fixtures/schema/merges.schema.json": "fixtures/v1/entities/merges.yaml",
  "fixtures/schema/memories.schema.json": "fixtures/v1/memories/gold.yaml",
  "fixtures/schema/transcript.schema.json": "fixtures/v1/transcripts/*.yaml",
  "fixtures/schema/contradictions.schema.json": "fixtures/v1/contradictions/cases.yaml",
  "fixtures/schema/temporal.schema.json": "fixtures/v1/temporal/asof.yaml",
  "fixtures/schema/qa.schema.json": "fixtures/v1/retrieval/qa.yaml",
  "fixtures/schema/leak.schema.json": "fixtures/v1/retrieval/leak.yaml",
  "fixtures/schema/documents.schema.json": "fixtures/v1/documents/pages.yaml",
  "fixtures/schema/drift.schema.json": "fixtures/v1/drift/docs.yaml",
}
```
