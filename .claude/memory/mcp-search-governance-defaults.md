# `memory_search` withholds by default — raw rows and both sides of an open contradiction

`crates/brainiac-server/src/mcp.rs` deliberately serves *less* than the store
can return, and each default was earned by a failed adversarial run:

- **`include_unreviewed` (default false)** — never-reviewed `raw` memories are
  excluded, so an agent sees only knowledge that cleared at least an automated
  gate. Before this, the review queue guarded nothing on the agent's main path.
  Rows below canonical that are still served carry a `governance_warning`.
- **`include_contested` (default false)** — memories in an **unresolved**
  contradiction are withheld from both sides; the response reports how many so
  the agent knows the area is contested and can escalate.
  `memory_context` quarantines them into a CONTESTED section outside the
  actionable bundle. Once the contradiction resolves they are served again.

**Why the contested default exists (do not "helpfully" relax it):** in a live
run against the real MCP server, an *unprovenanced* canonical decoy was
correctly resisted — the agent traced provenance, found none, discounted it.
But a **fully provenanced, more-recent** decoy sitting in an open contradiction
**fooled the agent**, which picked the poison over the true memory. Provenance
cuts both ways, and two canonicals in an unresolved contradiction are otherwise
served as two equal facts. Withholding turned that from silent poisoning →
flagged → refused-and-escalated.

Also load-bearing: the MCP tool descriptions were rewritten to induce
**mid-task** retrieval. The old "call once at the start" phrasing threw away
the mid-session-decay advantage, which is the main edge a memory service has
over a static instructions file. Don't reintroduce it.

Related invariant: `m.status::text` must be cast explicitly in SQL — the
Postgres enum will not decode into `Option<String>` directly.
