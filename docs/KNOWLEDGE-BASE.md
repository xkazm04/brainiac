# Brainiac — the knowledge base layer

**Status: v0.5 built end to end (KB0–KB5), and switched OFF.** The whole layer —
substrate, document core, read surfaces, publishing, the human round trip — is
merged and tested. Nothing is enabled for any real org: `kb_enabled` is false by
default, and no publish target exists until someone deliberately creates one.

One rule still gates the last step: **external publishing must not be turned on
for a real org until the extraction-recall workstream clears its gate.** A
composed page inherits the trustworthiness of the memories under it, and
publishing amplifies whatever is wrong down there. Everything is ready; turning
it on is a decision, not an upgrade.

The status table at the bottom is the honest map, and it mirrors the status log
in [`docs/KB-PLAN.md`](KB-PLAN.md) — that plan is the contract, this document is
the feature story. The public page is `/kb` in the console.

Nothing described here as shipped is unshipped. Where a capability is not built,
it says **roadmap**, in the same voice as the pitch page's "where we lose".

---

## 1. The problem the layer exists to fix

Every wiki rots, and it rots for one structural reason: **the page is where the
knowledge lives.** A page can therefore drift from reality without anything in
the system noticing, and it goes on being read — with institutional authority
attached — long after the thing it describes changed.

The classic mitigations do not work:

- *Review reminders* ask a human to re-derive whether the page is still true.
  That is the expensive part, and it is exactly the part nobody does.
- *AI-authored docs* increase the volume of prose, not the amount of knowledge.
  (DORA 2024: documentation quality rose while delivery stability fell.)
- *Search over the wiki* raises the ceiling of retrieval to the corpus — and the
  corpus is the stale thing.

## 2. What a composed page is

A page in Brainiac is **a projection over canonical memories, not a second source
of truth.** The canonical memory graph — facts, decisions, pitfalls and howtos
that a named human promoted through the review gate — is the only governed
substrate. A document is a compiled view over it.

A document is a sequence of sections, each of which is one of two kinds:

| Section | Owner | Behavior |
|---|---|---|
| **composed** | the machine | Bound to a memory query (entities, kinds, as-of, max items). Regenerated whenever a memory it depends on changes. Every claim cites its source inline as `[m:uuid]` — a footnote for a human, a structured ref for an agent. |
| **pinned** | a human | Owned prose. Regeneration never touches it. |

`document_dependencies` is the inverted index — which pages a given memory feeds.
So when a canonical memory is inserted, superseded, deprecated, or loses a
contradiction in the review queue, every page citing it is **marked dirty** and
the compose worker rebuilds it. **A contradiction resolution propagates to every
page that cited the losing memory.** Nobody schedules a doc review; there is
nothing to review, because there is no independently-authored artifact that could
have drifted.

Composition runs the binding through the same retrieval path agents use, as a
synthetic principal **capped at the page's visibility tier**. A team-private
memory cannot enter an org page — Postgres row-level security refuses, rather
than the composer remembering to filter.

## 3. The projection principle (and the asymmetry that enforces it)

The memory layer and the KB layer are separate, and the relationship between them
is deliberately **asymmetric**:

```
canonical memories ──compose──▶ page          (automatic, visibility-capped)
page ──human edit──▶ extraction ──▶ review gate ──▶ canonical memories
page ──────────────✗──────────────▶ canonical memories   (does not exist)
agent ─────────────✗──────────────▶ page                 (does not exist)
```

- A human **may** edit a composed section (`POST /v1/docs/{slug}/edit`, shipped).
  The edit is not saved as prose: it goes back through the **extraction
  pipeline** as candidate memories and faces the same review gate as any agent
  proposal. The API says the change was **captured**, never *saved* — because it
  wasn't, and a tool that says "saved" when it means "queued for someone else's
  approval" has lied to the person most likely to notice. Their stated *reason*
  travels with the edit, since the reason is exactly the knowledge a diff cannot
  recover. *A human editing the wiki is just another ingestion source.*
- A **pinned** section is the opposite: it is the human's own prose, it saves,
  and regeneration returns it byte-identically. Forever. The eval gates on it.
- There is **no** direct write-back from a page into canonical memory, and no
  bidirectional sync with any external tool. That asymmetry *is* the anti-rot
  mechanism: it means the wiki can never become a second place where truth is
  decided.

## 4. The lifecycle split

The most common way a wiki lies is not by being wrong. It is by presenting a
**roadmap intent as shipped architecture**, in the same typeface, with no marker.
Half of a stale Confluence space is decisions that were made and never built.

So the substrate splits them. Every memory carries a `lifecycle` facet —
`shipped | in_flight | proposed` — populated by extraction, with a firewall that
coerces an unknown value to `shipped` rather than dropping the memory. Composed
pages render the split: *what is in the product* and *what is on its way* are
different sections with different stamps. (This is shipped — migration `0015`.)

Memories also carry an optional `detail_md`: the code block, the config table,
the snippet, preserved alongside the distilled one-sentence `content` and
redacted through the same secret firewall. A retry policy is a table, not a
clause; a page that can only render the clause has a quality ceiling. (Also
shipped — `0015`.)

## 5. Health-gated publishing (built — KB3 — and switched off)

The Knowledge Health composite is **live today** at `/health`: a score with four
pillars — consistency, currency, liquidity, governance — over the real corpus.

It is now also an **actuator**. Before any *external* publish, the currency and
governance pillars are consulted, and if either falls below its floor
(`PUBLISH_MIN_CURRENCY` 70, `PUBLISH_MIN_GOVERNANCE` 50), **the sync pauses.**
Pages hold their last published revision rather than broadcasting stale belief to
the whole company at machine speed. Silence beats confident staleness.

The formulas live in one place (`brainiac-core::health`) and are used by both the
leadership report and the breaker — a brake that disagreed with the dashboard it
is named after would be indefensible.

An auto-synced wiki is an amplifier. Our own UAT found the failure it amplifies:
a stalled review queue kept being served as truth and nothing went red. This is
what turns the health score from a report into a brake. It is tested: degrade the
corpus, and the live page keeps showing the last good revision while the new one
is held back.

## 6. Confluence: your wiki becomes a render target (built — KB3 — and switched off)

Publishing is a single `Publisher` trait with pluggable targets: **Git** (writes
markdown files into a checkout — it deliberately does not commit or push, because
branch protection and release policy are yours) and **Confluence** (Cloud REST v2,
PAT, update-in-place). You do not have to abandon the wiki your company already
reads — Brainiac keeps it honest, and Confluence stops being a competing source of
truth.

Markdown → Confluence storage format is escape-first and deliberately small:
anything unrecognized degrades to visible text, so nothing a model writes can
reach your wiki as live markup. Citations survive the trip as links back into the
console — strip those and you have published just another wiki page.

Hard invariants on every external target:

- **One-way, always.** Pages are pushed, never pulled. A published page carries a
  generated-content banner and provenance links back to the console. Direct edits
  in the external tool are overwritten on the next compose; harvesting them back
  as an ingestion source is a later increment, not a day-one promise.
- **`org`-visibility only (v1).** External publish leaves RLS behind entirely, so
  only `org`-visible canonical memories may compose into a synced page. Team and
  private knowledge renders in the console only. A leaked private memory in a
  company wiki is not a score deduction — it is an unrecoverable trust event, and
  the publish-path leak count is a **build failure at zero**, not a warning.
- **Health-gated** (§5).

## 7. How a team turns it on (built — KB3 — and deliberately manual)

The KB layer is designed as an **org-level capability flag** — optional, and
recommended only where it pays. Our own controlled trial says the memory layer is
dead weight on single-team work; a layer you do not need should be a layer you do
not pay for.

API tokens carry **KB scopes** alongside their memory scopes:

| Scope | Grants | Who should hold it |
|---|---|---|
| `kb:read` | Read composed pages (console + the MCP doc tools) | agents |
| `kb:publish` | Sign a page revision into the org's mouth (and, with a target configured, into its wiki) | a small number of humans |

Turning the layer on is deliberately manual: set `orgs.kb_enabled` and insert a
`publish_targets` row. It cannot happen by upgrading. Credentials are never stored
— a target holds the *name* of an env var, so a database dump can never contain a
token that writes to your wiki.

An agent's token can read every page it is permitted to see without ever being
able to publish one. **Agents write memories; pages follow from them.**

## 8. What this layer will never do

These are refusals, not gaps. Each one is a thing a competitor could ship next
quarter and call an improvement, and each one would put the rot back.

- **No bidirectional sync.** Not as a setting, not as an enterprise tier. The
  moment a page can write to truth without the review gate, the wiki is a second
  source of truth again.
- **No agent writing a page directly.** Agents propose memories. Pages are
  compiled from the memories that survived a human. An agent that can author a
  page can author an unsigned belief with institutional formatting on it.
- **No LLM-invented diagrams.** A hallucinated arrow between two services is
  indistinguishable from an architecture decision. The only diagrams on the
  roadmap are *deterministic* projections of the entity/edge graph — compiled
  from edges that already exist, zero model involvement. If LLM-proposed diagrams
  ever ship, they enter through the same review gate as prose and every edge must
  cite a memory.
- **No private memory on an external surface.** Org-visibility only, leak = 0 as
  a build failure. We would rather publish a thinner page.
- **No page that outranks the memory it came from.** If the two disagree, the
  memory is right and the page is stale — by construction, it is about to be
  rebuilt.

## 9. Status

Phases as defined in [`docs/KB-PLAN.md`](KB-PLAN.md); this table is a mirror of
its status log, not an independent claim.

| Phase | What it is | Status |
|---|---|---|
| **KB0** — substrate | Memory `lifecycle` facet + `detail_md` (migration `0015`) end-to-end: core types, extraction prompt + facet firewall, store, retrieval, fixtures/gold. Knowledge Health console page at `/health`. | **shipped** (2026-07-14) — extraction eval gate passed on real qwen-max: recall 0.381 / precision 0.727 vs a 0.417 / 0.806 baseline, inside the gate. One noisy sample: it shows no *detectable* regression, it does not prove the facets are free. |
| **KB1** — document layer core | `documents` / `document_sections` / `document_revisions` / `document_dependencies` + RLS; the compose worker; dirty-marking; `[m:uuid]` citations; diff + auto-publish policy. | **shipped** (2026-07-14) — the `docs` eval profile passed on real qwen-max: coverage 1.0, hallucination 0.0, and **zero** leaks, pin violations, staleness failures and auto-published hallucinations. Those four are build failures, not scores. |
| **KB2** — read surfaces | Console page reader (markdown sanitized *by construction* — the renderer has no raw-HTML node kind), per-claim provenance chips, revision history, MCP `doc_get` / `doc_search` (read-only: agents propose memories, never pages), entity-page auto-scaffolding at ≥4 org memories across ≥2 teams. | **shipped** (2026-07-14). Deterministic mermaid entity-neighborhood: still deferred. |
| **KB3** — publishing | `Publisher` trait, Git target, Confluence adapter (PAT, one-way, banner + backlinks), `kb:read` / `kb:publish` token scopes + org capability flag, health circuit breaker wired as an actuator. | **built, not enabled** (2026-07-14). Off by default; see the sequencing rule at the top. |
| **KB4** — round-trip & hardening | Human-edit reingestion end to end (`POST /v1/docs/{slug}/edit` — composed edits are *captured*, never *saved*), propagation SLA measured and surfaced, KB signals feeding Knowledge Health. | **shipped** (2026-07-14) |
| **KB5** — public surfaces | The `/kb` page, the pitch page's KB section, this document, the README. | **shipped** |

Deferred beyond the ladder (see KB-PLAN "Follow-ups"): LLM-authored diagrams,
cross-documentation intelligence (repo scans, docs-drift detection, Confluence
harvest), proactive digests, further publish targets, team-space mapping.

**Sequencing rule:** external publishing does not go live for a real org until
the extraction-recall workstream clears its gate. Composed pages inherit
substrate trust, and publishing amplifies whatever is wrong with it.
