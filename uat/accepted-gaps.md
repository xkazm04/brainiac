# Accepted gaps — the baseline of known-and-accepted

Findings listed here are suppressed: the run acknowledges them and does not re-surface them as
new. **Append only when the user explicitly accepts one.** An entry needs a reason, not just an id.

> Nothing is accepted yet. The trial has not run.

Deliberately deferred per ARCHITECTURE §9 — `scope_note`, not defects, and not to be logged as
findings:

| deferred | note |
|---|---|
| OIDC / SCIM identity | v0 uses a static bearer-token → principal map (`auth.rs`). |
| Cedar policy engine | v0 uses typed Rust rules stored as data (PLAN.md deviation 2). |
| S3 / MinIO transcript blobs | v0 stores raw source text in Postgres (deviation 5). |
| Document layer (ARCHITECTURE §8) | v0.5. |
| Cross-encoder reranker | Seam only; no model ships (`core/rerank.rs`). |
| Apache AGE / Kafka | Recursive CTEs and the in-repo queue are the v0 choices. |

**But note the standing rule from `rubric.md`: "deferred" is not a defense against a harm
finding.** If the shipped shape leaks, poisons, or misleads *today*, it does so today, and a
roadmap does not protect a secret. Deferral excuses a *missing feature*; it does not excuse a
*live hazard*.
