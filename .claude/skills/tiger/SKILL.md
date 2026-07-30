---
name: tiger
memory: vault
category: Testing
description: Hunts the highest-value surface of Brainiac — the LLM call sites themselves — and drives them to their potential across three lenses. (1) Code quality of the AI plumbing (the `meter()` chokepoint, LightTrack telemetry, caching/dedupe, JSON schema + validation + self-repair). (2) Business value, reusing /uat's Character method (developers with jobs-to-be-done + a senior-quality bar + time-saved) but TESTING ONLY THE LLM PIECES — does each prompt's grounding and output clear the bar. (3) Model optimization as an alternative scenario — benchmark the same character inputs across providers × models via `BRAINIAC_MODEL_*` to find quality degradation/upgrade vs cost/latency. Everything is memorized in the repo's `tiger/` overlay (an Obsidian-openable vault, one note per call site / character / model / session) so each scan builds on the last. Invoke with `/tiger init|scan|run|benchmark|recall|backlog [args]`.
---

# Tiger — hunt the LLM value (Brainiac)

> Brainiac's model calls are its apex surface: seven chat call sites in
> `crates/brainiac-pipeline/src/` decide what becomes org memory, how
> contradictions are adjudicated, and what a composed page claims. They cost the
> most, vary the most, and carry the most business value. Tiger stalks exactly
> those — ignores the CRUD, the RLS plumbing, the console — and never lets a
> high-value call site sit under-wrapped, under-grounded, or running an
> over-priced model. It is the LLM-focused sibling of `/uat`
> (`.claude/skills/uat/skill.md`): it reuses UAT's Character/JTBD/senior-bar/
> time-saved/impact-scoring method, but scoped to the prompts and their outputs,
> and it adds two lenses UAT doesn't have (AI code-quality and model
> benchmarking).

## What Tiger is (and isn't)

**Is:** a periodic, deliberate pass that (a) builds a durable inventory of every
LLM call site, (b) judges each on three lenses, and (c) emits one **session
backlog** — the prioritized work to get the most out of the model engines.
**Isn't:** a per-commit gate, a generic linter, or a test of non-AI code. If a
finding isn't about a model call (or the plumbing/value/economics of one), it's
out of scope — that's `/uat`, `/explorer`, `/code-review`, or `/simplify`.

**Real model calls are the point** for lenses 2 and 3 — that's what catches what
static reading can't (actual output quality, actual quality-vs-cost trade-off).
So Tiger is **cost-aware**: it samples, caches in the vault, and never re-runs an
identical (prompt, input, model) it already has a result for.

---

## Brainiac ground truth (verified 2026-07-26 — re-verify each `scan`)

This is the engine's map of the repo. Every line below was read from source; if a
`scan` finds it stale, **update this section in the same run**.

### The chokepoint

`meter()` — `crates/brainiac-gateway/src/providers/mod.rs:71`. Single telemetry
chokepoint: times the future, then calls `crate::lighttrack::record(provider,
model, None, input_tokens, output_tokens, latency_ms, ok)`. A failed call meters
as 0 tokens with `status=error` ("cost unknown ≠ cost zero").

All five chat providers route through it — each passing its own vendor tag:

| Provider | `meter(...)` call | tag |
|---|---|---|
| OpenAI-compatible (also Azure, Qwen/DashScope, Groq, Together, DeepSeek, Mistral, OpenRouter, Ollama/vLLM) | `providers/openai.rs:167` | `&self.provider_tag` |
| Anthropic Messages | `providers/anthropic.rs:147` | `anthropic` |
| Google Gemini | `providers/google.rs:151` | `google` |
| AWS Bedrock (SigV4) | `providers/bedrock.rs:84` | `bedrock` |
| Google Vertex AI | `providers/vertex.rs:226` | `vertex` |

Shared plumbing that is NOT per-provider: `build_http_client()`
(`providers/mod.rs:36`, connect + request timeouts via
`BRAINIAC_GATEWAY_CONNECT_TIMEOUT_SECS` / `_REQUEST_TIMEOUT_SECS`) and
`crates/brainiac-gateway/src/resilience.rs` (retry + circuit breaker + timeout).

**Two verified plumbing facts to carry into Lens 1 (don't re-discover — extend):**
1. `meter()` passes `None` for LightTrack's `name` field
   (`providers/mod.rs:85`), so no event carries a **stage** rollup key
   (`extract` / `resolve` / `contradict` / `compose` / `divergence`) even though
   `Stage::as_str()` exists at `crates/brainiac-gateway/src/lib.rs:92` and
   `lighttrack::record`'s doc comment says `name` "is the stage/use-case rollup
   key when reachable" (`lighttrack.rs:75`). Cost cannot be attributed per stage today.
2. **Embeddings are not metered.** `meter()` returns `ChatResponse` and is called
   only from `complete()`; `OpenAiCompatEmbedder::embed` / `embed_batch`
   (`providers/openai.rs:354`, `:368`) bypass it entirely. Embedding spend is invisible.

### Telemetry

`crates/brainiac-gateway/src/lighttrack.rs` — vendored fire-and-forget LightTrack
client. `record()` at `lighttrack.rs:79` POSTs `{LIGHTTRACK_URL}/v1/events` from
one background thread with a 2s timeout, every failure swallowed. Env contract:
`LIGHTTRACK_URL` (unset ⇒ permanently disabled no-op), `LIGHTTRACK_KEY`
(optional bearer), `LIGHTTRACK_PROJECT` (optional project id). Wire shape:
`provider`, `model`, `operation: "chat"`, `usage.{input,output}`, `latency_ms`,
`status`, `source: "brainiac-gateway"`, optional `name` / `project_id`.
Deploy wiring lives in `.env.deploy.example` (`LIGHTTRACK_*`, commented out).

### The seven call sites (chat)

Every one goes through `ChatProvider::complete` (`gateway/src/lib.rs:67`) and all
pass `temperature: 0.0` — deliberate; see the doc comment at `lib.rs:44-53`
(temperature was the dominant source of extraction variance, UAT 2026-07-14).

| id | file:line | stage | json_mode | max_tokens | prompt const |
|---|---|---|---|---|---|
| `extract` | `crates/brainiac-pipeline/src/extract.rs:489` | Extract | yes | `MAX_EXTRACT_TOKENS` | `EXTRACT_SYSTEM_PROMPT_V1` |
| `extract-repair` | `crates/brainiac-pipeline/src/extract.rs:525` | Extract | yes | `MAX_EXTRACT_TOKENS` | same + parse-error echo (self-repair loop, `MAX_REPAIR_ATTEMPTS`, final-attempt escape hatch) |
| `resolve-adjudicate` | `crates/brainiac-pipeline/src/resolve.rs:147` | Resolve | yes | 128 | `ADJUDICATE_SYSTEM_PROMPT_V1` |
| `contradict` | `crates/brainiac-pipeline/src/contradict.rs:91` | Contradict | yes | 256 | `CONTRADICT_SYSTEM_PROMPT_V1` |
| `compose` | `crates/brainiac-pipeline/src/compose.rs:370` | Compose | **no** | 900 | `COMPOSE_SYSTEM_PROMPT_V1` |
| `faithfulness-judge` | `crates/brainiac-pipeline/src/faithfulness.rs:241` | (uses the composer's provider) | yes | 700 | `JUDGE_SYSTEM_PROMPT_V1` |
| `divergence` | `crates/brainiac-pipeline/src/divergence.rs:216` | Divergence | yes | `MAX_DIVERGENCE_TOKENS` | inline `system` (axis-dependent: team vs project) |

Parse/validation shapes already present: `parse_extraction` + the repair loop
(extract), `extract::extract_json_object` tolerant recovery reused by
`contradict.rs`, `resolve.rs`, `divergence.rs`; `parse_verdicts` in
`faithfulness.rs`. `compose` is the only free-prose output — which is exactly why
`faithfulness.rs` exists as its judge.

### Routing (the Lens-3 invocation recipe)

`ProviderRouter` (`gateway/src/lib.rs:186`) resolves a provider per `Stage`.

- Process default: `BRAINIAC_PROVIDER` (default `qwen`) + optional
  `BRAINIAC_MODEL`, via `default_chat_from_env()` (`lib.rs:166`).
- Per-stage override: `BRAINIAC_MODEL_EXTRACT` / `_RESOLVE` / `_CONTRADICT` /
  `_COMPOSE` / `_DIVERGENCE`, each a spec string parsed by
  `provider_from_spec()` (`lib.rs:122`): `<provider>` or `<provider>:<model>`,
  split on the FIRST colon only (so Bedrock ids like `…-v2:0` survive).
  Providers: `qwen | openai | azure | openai-compat | anthropic | google`
  (alias `gemini`) `| bedrock | vertex | mock`.
- Credential env per provider is documented at `lib.rs:159-165` and in
  `docs/deploy/providers.md`.
- `MockProvider` (`lib.rs:262`) is the deterministic offline responder — use it
  for Lens-1 harness work and for any Lens-2/3 dry run that must not spend money.

**So a Lens-3 matrix cell is: set `BRAINIAC_MODEL_<STAGE>=<provider>:<model>`,
run the matching eval profile, capture score + latency + tokens.** No bespoke
harness needed.

### The measurable quality harness (already in the repo)

`cargo run -p brainiac-server -- eval --fixtures fixtures/v1 --profile <p>
--baseline results/<p>-baseline.json --out results/ci-run.json`

Profiles: `retrieval | extraction | contradiction | resolution | pipeline | docs
| drift | divergence`. The **`extraction`** and **`docs`** profiles run a REAL
provider (see `crates/brainiac-server/src/main.rs` — the `docs` profile is
multi-sample; each sample costs a full real-model run), so they are Tiger's
cheapest honest cross-model signal for `extract` and `compose`. Committed
baselines live in `results/*-baseline.json`. **Never recalibrate a baseline from
a Tiger run** — `--write-baseline` is a deliberate, separately committed diff
(`.claude/CLAUDE.md` § Quality standards, `docs/EVAL.md` §3.2).

---

## The `tiger/` overlay — durable, linked memory

The `tiger/` overlay **is an Obsidian-openable vault** (a folder of markdown with
YAML frontmatter and `[[wikilinks]]`) and it lives **at the repo root, next to
`uat/`** — the repo's overlay convention. It is versioned with the code so a
call-site note and the call site itself move together. (This is deliberately
*not* the external-Obsidian path `/perfect` uses for loop control-state; Tiger's
memory is evidence about code, so it belongs beside the code.)

```
tiger/
  Tiger.md                 # home / Map-of-Content: headline state + links to everything
  config.md                # discovery globs, the model-invocation recipe (BRAINIAC_MODEL_*),
                           #   fixtures, what counts as a call site
  call-sites/<id>.md       # one note per LLM call site (the inventory — the core asset)
  characters/<name>.md     # durable LLM-focused Characters (JTBD + senior-bar + criteria)
  models/<model>.md        # per-model×stage benchmark rollups (quality/cost/latency)
  sessions/<date-slug>.md  # one note per run: scope, scores, backlog, deltas vs last run
  .gitignore               # ignore captures/ (raw model outputs can be large)
```

Every note carries frontmatter and links. The vault is **append-and-update**:
call-site notes are long-lived (status/score/recommended-model evolve); session
notes are immutable run records; the home note always reflects current truth.

Use the call-site `id`s from the table above (`extract`, `extract-repair`,
`resolve-adjudicate`, `contradict`, `compose`, `faithfulness-judge`,
`divergence`) — they are stable slugs; never renumber them.

### Note schemas

**Call site** (`call-sites/<id>.md`) — the unit of value:
```markdown
---
id: extract | extract-repair | resolve-adjudicate | contradict | compose | faithfulness-judge | divergence
type: tiger/call-site
modality: text
file: <path:line of the .complete(&ChatRequest{…}) call>
stage: Extract | Resolve | Contradict | Compose | Divergence | (none — inherits caller's provider)
wrapper: ChatProvider::complete → meter()   # the chokepoint; note any bypass
provider: <resolved by BRAINIAC_PROVIDER / BRAINIAC_MODEL_*>   model: <current model>
schema: <json_mode + parser fn + repair? and where>
grounding: <n/m sources that reach the prompt>
quality_score: <0–5 senior-bar>    code_score: <0–5 plumbing>
recommended_model: <from the last benchmark, or "—">
eval_profile: <the profile that measures it, or "none">
status: discovered | assessed | benchmarked | improved
last_scanned: <YYYY-MM-DD>
characters: ["[[char-a]]", "[[char-b]]"]
---
## What it does
<the job this call performs, in one line, + which worker/CLI path reaches it>
## Prompt & grounding
<system+user prompt summary; the REAL context that should feed it (memory content,
entity anchors, team/project axis, cited memories, org scope) and how many actually
reach it → grounding n/m, cite file:line>
## Code quality (wrapping · logging · caching)
<through meter()? typed schema + validate + normalize + self-repair? telemetry
(cost/latency/tokens/attempts/stage-name)? cache/dedupe by input hash? prompt bloat
(unbounded memory lists where a digest would do)? max_tokens sensible? — cite file:line>
## Findings
<impact-scored items across the 3 lenses; link [[session]] where raised>
```

**Character** (`characters/<name>.md`) — reuse `/uat`'s roster (the Meridian
company in `uat/company.md` + `uat/characters/`), narrowed to model outputs:
```markdown
---
name: <First role-tag>     type: tiger/character
maps_to: ["[[call-site]]", …]   # the LLM surfaces this Character exercises
references: [<url> — bar it sets]
---
## Who they are / Background / Voice  (per the /uat template — authentic texture)
## Jobs to be done  (what they hire the MODEL OUTPUT for)
## Senior-quality bar  (the floor: output ≥ what they'd write as a senior in role)
## Time-saved (motivation)  (LLM-less minutes → with-Brainiac minutes, as a NUMBER)
## Scored acceptance criteria (judged identically every run, applied to the OUTPUT)
- [ ] grounded in MY real context (names the real system/team/entity from the transcript, no placeholders)
- [ ] senior-grade (specific, correct, not generic)
- [ ] worth the latency/cost
```

**Model** (`models/<model>.md`): per call-site rows of `{quality, costUsd,
latencyMs, verdict}` per model spec, + the headline recommendation.

**Session** (`sessions/<date>.md`): scope, the inventory delta, per-lens findings,
the impact-ranked backlog, the model-opt recommendations, and the **value ledger**
(grounding + time-saved rolled up; what the engine *promises* vs *delivers*).

---

## The three lenses

### Lens 1 — Code quality of the AI plumbing (static, code-grounded)
For each call site, follow the chain (`pipeline` → `ChatProvider::complete` →
provider impl → `meter` → `lighttrack::record`) and score, citing `file:line`:
- **Wrapping / chokepoint:** does the call go through `ProviderRouter::for_stage`
  and land in `meter()`, or does it construct a provider ad hoc / bypass the
  router? Typed JSON **schema** + **parse** + **validate** + **self-repair**
  re-prompt (extract has one; do the short classification sites need one, or is
  "skip, never guess" — `contradict.rs:99` — the right call)? Is
  `json_mode`/`max_tokens`/`temperature` right for the job?
- **Logging / telemetry:** is each call recorded with enough to attribute cost
  (provider, model, tokens, latency, status — and the **stage name**, currently
  `None`)? Are repair attempts counted anywhere? Do failures surface in tracing
  AND Sentry (see `/sentry`)? Does an eval profile fingerprint the prompt so
  drift is caught?
- **Caching / efficiency:** is an identical (system, user, model) **cached /
  deduped** by input hash, or recomputed (e.g. `contradict` compares a memory
  against every anchor-sharing neighbor; `divergence` re-adjudicates the same
  canonical concept each sweep)? Is the prompt **bloated** — whole memory bodies
  where a digest would do (`render_memories` in `compose.rs`, the listed-per-team
  block in `divergence.rs`)? Is grounding text built once and reused across a batch?
- Emit code-quality findings + concrete fixes (thread `Stage` into `meter`, meter
  the embedder, add an input-hash cache, tighten a schema).

### Lens 2 — Business value (Character method, scoped to the output)
Reuse `/uat`'s engine but point it only at the prompts/outputs:
1. **Characters** (durable in `tiger/characters/`) — Brainiac's real target group
   is a multi-stack engineering org (`uat/company.md` — the Meridian fixture).
   Each Character `maps_to` the call sites their JTBD hits (a staff engineer
   hitting `extract` + `contradict`; a tech-writer/architect hitting `compose` +
   `faithfulness-judge`; a platform lead hitting `divergence`). Offer to adapt
   `/uat`'s existing roster rather than inventing a parallel one.
2. **L1 (theoretical, mass-parallel):** per `character × call-site`, read the
   prompt + grounding and judge the *designed* output against the Character's
   senior-bar + scored criteria + time-saved. Score **grounding n/m** (how much of
   the user's real world reaches the prompt) — Tiger's highest-yield finding
   type, fully visible in code. Example live question: `contradict`'s user message
   is literally `"A: {other.content}\nB: {memory.content}"`
   (`contradict.rs:93`) — no time, no author, no project axis. Is that enough for
   a senior engineer to accept the verdict?
   **Grounding bar (hard rule):** every verdict must **quote the actual prompt
   text** (the real `*_SYSTEM_PROMPT_V1` const / `format!` at `file:line`, not a
   paraphrase) **and at least one real sampled output** (from an eval run under
   `results/`, a fixture, a cached `tiger/captures/`, or an L2 live call). Never
   judge from the call-site *name*. If no output sample exists anywhere, say so
   and mark the verdict `ungrounded — needs L2 sample` instead of guessing.
3. **L2 (empirical, optional but ideal):** actually run the call with
   character-shaped inputs and judge the live output. Cheapest honest paths:
   `cargo run -p brainiac-server -- eval --fixtures fixtures/v1 --profile
   extraction|docs|divergence …` with a real `BRAINIAC_PROVIDER`, or a worker tick
   against the Meridian fixture (`cargo run -p brainiac-server -- worker`, needs
   `DATABASE_URL` + `docker compose up -d`). One confirmed live finding beats ten
   theoretical ones.
- Emit business-value findings (grounding gaps, senior-bar misses, "the memory it
  wrote is not worth storing") + suggested fixes.

### Lens 3 — Model optimization (the alternative scenario)
The Characters are the **consistent judgment harness** that makes cross-model
comparison fair. For the selected call sites:
1. Hold the prompt + character input fixed; **run it across a model matrix** by
   setting `BRAINIAC_MODEL_<STAGE>=<provider>:<model>` per cell (per `config.md`'s
   recipe) and re-running that stage's eval profile. Brainiac's calls are
   structured extraction/classification at `temperature: 0.0` — schema-checkable
   and verification-heavy — which is exactly where a model axis is measurable.
   `compose` is the one long-form output: benchmark it at most low vs medium
   effort, and judge it with `faithfulness`, not vibes.
2. Have the **same Character criteria** judge each output (blind to which model).
   Use **forced ranking with a named separator per adjacent pair** — absolute 0–5
   scoring saturates. Record **cost + latency** per cell (LightTrack's
   `usage`/`latency_ms` if `LIGHTTRACK_URL` is set; otherwise the eval run's own
   timings).
3. Find the frontier: the cheapest/fastest cell that still clears the senior-bar
   (a **downgrade** — money saved at equal quality) and any call site where a
   **stronger** model meaningfully lifts business quality (an **upgrade** worth
   the spend). Watch for **degradation** (a model that silently drops grounding or
   hallucinates past its citations — the `docs`/`faithfulness` gate catches this).
- Emit a per-call-site recommendation: `keep | downgrade to X | upgrade to Y`
  with quality/cost/latency evidence, written to `models/*` and the call-site
  note's `recommended_model`. A shipped recommendation is an env change
  (`BRAINIAC_MODEL_*` in `.env.deploy`), not a code change — say so in the backlog item.

> **Judging rules (measured, 2026-07):**
> Quality is judged by Characters, never by the model under test grading itself.
> - **Cross-family comparisons need ≥2 judge families or a human spot-check.**
>   Judges rank their own model family first (two judges disagreed at ρ = 0.50 and
>   each favored its own family). A single-judge verdict comparing, say, a Qwen
>   cell to an Anthropic cell is inadmissible; within-family comparisons from one
>   judge are fine.
> - **More effort is not better.** On long-form output (`compose`), quality
>   *inverted* above medium effort — the priciest run drifted its own
>   cross-references. Length is not insight; never recommend an upgrade on prose
>   evidence alone. For `compose`, the honest metric is the `docs` profile's
>   faithfulness/citation gates, not fluency.
> - **A hard output cap collapses the effort axis.** `resolve-adjudicate` (128
>   tokens) and `contradict` (256) are capped by design — don't benchmark
>   reasoning effort there; you'd pay for reasoning you don't get.
> - **When every cell disappoints, suspect the prompt framing before the model.**
>   The most replicated finding: all cells at every model missed the same thing.
>   Emit a *value* (Lens 2) finding against the `*_SYSTEM_PROMPT_V1` const, not an
>   *upgrade* recommendation.

---

## Modes

- **`init`** — scaffold the `tiger/` overlay + `config.md`; run the first
  **inventory scan** (rediscover call sites — `rg "\.complete\(&ChatRequest" crates/`
  plus the embedder path — and reconcile against the ground-truth table above);
  derive the target group from `uat/company.md` and **ask how many Characters
  (1/5/10)**; draft Characters (offer to adapt `/uat`'s); write `Tiger.md`. No
  lens runs yet.
- **`scan`** — re-inventory, **diff against the vault**: new / removed / changed
  call sites (prompt or schema drift vs the recorded fingerprint), update notes,
  flag regressions, refresh this file's ground-truth table if line numbers moved.
  Cheap; run often.
- **`run [--lens code|value|model|all] [--chars N] [--live]`** — the full pass.
  Default `--lens all`, L1 only; `--live` adds Lens-2 real generations and Lens-3
  benchmarking on the selected/highest-value call sites. **Mass-parallel:** one
  subagent per `call-site` (Lens 1) and per `character × call-site` (Lens 2).
  Writes a `sessions/<date>` note + refreshes call-site notes + the backlog.
- **`benchmark <call-site> [--models …]`** — Lens 3 only, deep, for one call site.
- **`recall`** — read the overlay and summarize current state (top call sites by
  value, open backlog, last session, model recommendations) without re-scanning.
- **`backlog`** — (re)emit the impact-ranked backlog from current findings.

## The session backlog (the deliverable)

One impact-ranked list (frequency × reachability × value, not raw severity — same
scoring as `/uat`), each item tagged by lens and linking its `[[call-site]]`:
- **code** — wrapping/logging/caching fix (e.g. "thread `Stage::as_str()` into
  `meter()`'s `name` so LightTrack can attribute cost per stage —
  `providers/mod.rs:85`"; "dedupe `contradict` pairs by (a,b) content hash").
- **value** — grounding/quality fix (e.g. "feed project axis + recorded_at into
  the contradiction prompt — grounding 2/5 → 4/5").
- **model** — model swap, shipped as a `BRAINIAC_MODEL_*` env change (e.g.
  "`resolve` → cheaper model: equal senior-bar on the `resolution` profile,
  −N% cost").
Plus a **value ledger** (grounding & time-saved rolled up; promised vs delivered)
and the **strengths to protect** (e.g. `temperature: 0.0` everywhere; the
extract repair loop; `faithfulness` as compose's judge).
The chat reply is the headline + sharpest findings, linking `file:line` and overlay notes.

## Concurrency & trust
- **Mass-parallel** Lens 1 + Lens 2-L1 (one subagent per unit). Lens 2-L2 and
  Lens 3 make real calls → serial-ish and **cost-bounded** (sample call sites;
  cache every result in `models/*` keyed by (call-site, model, input-hash) so
  re-runs are free).
- **Evidence or it didn't happen:** every finding cites `file:line` (static) or a
  captured output/metric (live).
- **Adversarial judging** for value + model verdicts; default to "not better"
  unless the output earns it. Forced ranking over absolute scores; cross-family
  verdicts need ≥2 judge families or a human spot-check.
- **Honest ceilings:** name what still isn't grounded/optimized after a fix.
- **Overlay hygiene:** call-site `id`s are stable across runs; never duplicate a
  note — update it. Record the prompt-const + model **fingerprint** so `scan`
  detects drift.
- **Vault-write verification:** a discovery/scan subagent may be unable to write
  files in some harnesses and will return note bodies inline instead. After any
  parallel scan the orchestrator MUST `ls` the target dir, diff against the
  expected `id` set, and **backfill** missing notes — don't trust "wrote N notes".
- **Don't spend blind.** Before any `--live` run, state the provider, the
  profile, the number of model calls, and get a go/no-go. `mock` is always a legal
  dry-run provider (`BRAINIAC_PROVIDER=mock`).
- **Repo gates still apply to any code Tiger ships:** `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace` (with `DATABASE_URL` and `docker compose up -d`, or the pg tests
  self-skip and nothing was verified). Never `git add -A`; stage the exact paths.
  Never recalibrate an eval baseline as a side effect.

## Using Tiger here
1. `/tiger init` → scaffolds `tiger/`, writes `config.md`, drafts Characters from
   the Meridian company. 2. Resolve `config.md`'s open questions (which models are
   actually reachable with the keys in `.env` — `docs/deploy/providers.md` has the
   credential matrix). 3. `/tiger run` for a cheap broad pass; `/tiger run --live`
   for real generation + benchmark. 4. Work the backlog; re-`scan`/`run` to
   measure the delta in the overlay.
