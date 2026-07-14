> Context: Server: MCP Agent Surface
> Total: 5 (Critical: 0, High: 3, Medium: 2, Low: 0)

## 1. `as_of` silently degrades to "now", answering a historical question with live data
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: silent-failure
- **File**: crates/brainiac-server/src/mcp.rs:542-545 (also memory_context 689-692)
- **Scenario**: An agent calls `memory_search`/`memory_context` with `as_of: "2026-01-01"` (date only, no timezone) or any string `DateTime::<Utc>::from_str` rejects. The value is parsed with `.and_then(|s| s.parse().ok())`, so a parse failure yields `None`, and `None` is treated as "answer as of now."
- **Root cause**: Every other narrowing param (`scope`, `kinds`, `min_confidence`) is strictly validated to `-32602 InvalidParams`, but `as_of` alone is "leniently" parsed — the comment at 686-688 states this is deliberate ("a caller that hands a malformed timestamp still gets a live bundle"). RFC3339 requires an offset, so the very common date-only form silently fails to parse.
- **Impact**: The agent explicitly asked "what was true at time T" and receives "what is true now" with no error, no `note`, and no way to detect the substitution. In a temporal memory engine where `as_of` selects which facts were valid in their validity window, this returns confidently-wrong point-in-time results the agent will act on. A revoked/superseded fact reappears; a not-yet-valid fact is served.
- **Fix sketch**: Parse `as_of` strictly like the other params: on `Some(str)` that fails to parse, return `invalid("as_of must be an RFC3339 timestamp")` (`-32602`). If lenient behavior is truly wanted, at minimum surface an `as_of_ignored: true` marker in the payload so the degradation is visible.

## 2. Unbounded stdin line read defeats the documented input-size caps (OOM/DoS)
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: resource-exhaustion
- **File**: crates/brainiac-server/src/mcp.rs:185-207 (`serve_stdio`, `next_line()` at 192; caps promise at 32-47)
- **Scenario**: A runaway/buggy agent writes a multi-gigabyte line with no `\n`. `BufReader::lines().next_line()` accumulates the entire line into a `String` with no length bound, then `process_line` hands the whole thing to `serde_json::from_str::<Value>`, allocating the parsed tree — all *before* any `within_cap`/`required_str` check runs.
- **Root cause**: The per-field caps (`MAX_QUERY_CHARS`, `MAX_CONTENT_CHARS`, …) are enforced inside each tool, i.e. post-parse. The transport layer has no frame-size limit, so the "trust boundary reached by autonomous agents; every free-text field is bounded so a runaway caller can never hand us an unbounded blob" promise (lines 32-36) is defeated one layer below where the caps live.
- **Impact**: A single frame can exhaust memory and crash the MCP process (taking down the developer's whole agent session), or stall it in a large allocation — the exact runaway-caller scenario the hardening claims to prevent. The caps give a false sense of protection.
- **Fix sketch**: Bound the line length before parsing — e.g. read via a length-limited reader (`AsyncBufRead::take(MAX_FRAME_BYTES)` per line, or a manual read loop that aborts a line exceeding a cap) and return a `-32700`/`-32600` frame (or drop the oversized line with a logged warning) instead of buffering unboundedly. A sane ceiling is roughly `MAX_CONTENT_CHARS`×4 plus JSON overhead.

## 3. `entity_lookup` serves below-canonical and open-contradiction memories with none of the governance/contradiction/trust warnings the other tools attach
- **Severity**: High
- **Lens**: bug-hunter
- **Category**: governance-bypass
- **File**: crates/brainiac-server/src/mcp.rs:1162-1176 (contrast memory_search 633-649, 639-648)
- **Scenario**: An agent resolves an entity (e.g. `entity_lookup { name: "kafka" }`). The tool returns `for_entities(...)` memories shaped as bare `{ id, kind, content }`. `for_entities` admits `status IN ('canonical','candidate')` — so `candidate` (not-human-certified) rows are returned unflagged, and it does **no** contradiction check, so a memory sitting in an OPEN, unresolved contradiction is handed over as plain fact.
- **Root cause**: The entire surface invests in tagging provisional rows (`governance_warning`) and *withholding* contested rows by default (the UAT-2026-07-13-l2 poison fix). `entity_lookup` is a second agent-reachable retrieval path that never received that treatment — it emits content with no `governance`, `contradicted`, or `feedback`/`disputed` signal.
- **Impact**: The contested-withholding defense that `memory_search`/`memory_context` enforce is trivially bypassed through `entity_lookup`: a well-provenanced poison anchored to a looked-up entity is served as authoritative, unwarned. Agents are told to trust `entity_lookup` output ("the strongest memories about it"), so provisional/contested knowledge is consumed as canonical.
- **Fix sketch**: Batch `open_contradictions_for` + `trust_for` over the returned ids (as `memory_search` does) and either withhold contested rows or attach the same `contradicted`/`governance`/`disputed` flags; tag every non-canonical row with the provisional warning. Restrict `for_entities` here to canonical, or surface the status per row.

## 4. Repeated RLS visibility-gate + trust/contradiction post-processing boilerplate
- **Severity**: Medium
- **Lens**: code-refactor
- **Category**: duplication
- **File**: crates/brainiac-server/src/mcp.rs:916-920, 1001-1007, 1188-1191 (and the trust+contradiction block 565-591)
- **Scenario**: Three tools open with the same hand-rolled "prove the caller can see this id under RLS, else not-found" gate: `knowledge_propose` (`SELECT status … WHERE id=$1 … ok_or rejected("memory not found")`), `memory_feedback` (`SELECT 1 … WHERE id=$1`), `memory_provenance` (`provenance_for_memory … else rejected`). Separately, the `trust_for` + `open_contradictions_for` + per-hit JSON shaping in `memory_search` (565-649) is a near-copy of the REST handler in `http.rs:349-402`, which even comments "the same parity the MCP surface attaches."
- **Root cause**: Each tool grew its own inline SQL/no-oracle gate rather than a shared `ensure_visible(tx, memory_id) -> Result<(), ToolError>` helper; the search enrichment was duplicated across the MCP and REST surfaces instead of a shared enricher (they diverge only in withholding policy).
- **Impact**: Three subtly different visibility gates are three places a future edit can weaken the no-existence-oracle guarantee inconsistently; the search-enrichment duplication means a fix to trust/contradiction handling must be made twice and can drift (it already differs in withholding).
- **Fix sketch**: Extract one `ensure_visible(&mut tx, id)` returning the not-found `ToolError`, and one enrich-hits helper the MCP and REST paths share, parameterized by whether contested rows are withheld.

## 5. The "leak-sensitive" test never exercises the cross-org RLS boundary — only intra-org team visibility
- **Severity**: Medium
- **Lens**: bug-hunter
- **Category**: test-coverage
- **File**: crates/brainiac-server/tests/mcp_pg.rs:50-60, 121-129, 282-294, 359-376, 718-732
- **Scenario**: The suite seeds exactly ONE org (`fx.org.org`) and constructs a single principal in it (comment: "the leak-sensitive case"). Every "leak" assertion — `mem-pay-0055` not surfacing through `memory_search`/`memory_feedback`/`knowledge_propose`/`memory_provenance` — uses a memory inserted under the *same* org id, differing only by team. No second org is ever created, so no assertion crosses the org (tenant) boundary.
- **Root cause**: `mem-pay-0055` (the payments team) was treated as the adversarial case, conflating team-level visibility with tenant isolation. The product's core guarantee ("an agent can never retrieve what its operator can't", mcp.rs:9-12) is org-level RLS, but the tests only prove team-level filtering within one tenant.
- **Impact**: A regression that broke org-level RLS on the MCP surface (e.g. a tool query missing an `org_id`/RLS scope, or a canonical-entity/alias table without an org policy) would pass this entire suite green. The highest-value property of a multi-tenant memory engine is unverified at the MCP boundary.
- **Fix sketch**: Seed a second org with its own memory, run the same principal, and assert the other org's memory is invisible to `memory_search`, `entity_lookup`, `memory_provenance`, and `doc_get`/`doc_search` — i.e. add a genuine cross-tenant case alongside the existing cross-team ones.
