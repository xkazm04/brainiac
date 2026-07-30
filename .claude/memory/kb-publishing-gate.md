# KB publishing is built but OFF by default — that is the rule, not an oversight

The whole knowledge-base line (KB0–KB5, plan in `docs/KB-PLAN.md`) is
implemented and tested, but external publishing is deliberately disabled:
`orgs.kb_enabled` defaults to **false** (`crates/brainiac-store/src/publishing.rs`,
checked in `crates/brainiac-publish/src/lib.rs`) and **no publish target
exists** until someone creates one on purpose. `crates/brainiac-server/tests/
publish_pg.rs` asserts "nothing publishes without opt-in".

Do not flip this on for a real org to "unblock" something: the standing gate is
that KB3 publishing stays off until the extraction-recall workstream clears its
own gate. The Library's cross-org exchange inherits the same posture (it needs
a sibling org capability flag before any rule leaves the building).

Settled architecture decisions that should **not** be re-litigated:

- Memory is the sole source of truth; the KB is a **projection with asymmetry**
  (pages never write back; pages→memory only through extraction). No
  bidirectional Confluence sync, no agent writing pages directly.
- Confluence/OKF are one-way publish targets behind a generic `Publisher`
  trait; OKF is *also* a harvest source (`brainiac okf-harvest` → extraction →
  review gate), and the harvester refuses our own published pages to prevent
  self-citation.
- Knowledge Health is the auto-publish circuit breaker.
- Diagrams are deferred except deterministic mermaid entity-neighborhoods.
- Standards pages are **projected, never composed**: no LLM, no temperature, no
  citation firewall — a rule's statement is the sentence a named human
  ratified, and asking a model to re-word it would fork the org's commitment.
  It is the first `doc_kind` that calls no LLM at all.
