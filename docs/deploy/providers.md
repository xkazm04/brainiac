# LLM providers — the BYOM spectrum

Brainiac's gateway (`crates/brainiac-gateway`) is provider-agnostic: every pipeline
LLM call goes through one `ChatProvider` trait, and the concrete provider is chosen
by environment variable. Qwen/DashScope is the default (the $0 Alibaba recipe), but
you can point Brainiac at any of the providers below without touching code.

All providers share the same resilience (retry + circuit-breaker + per-request
timeout) and the same LightTrack cost telemetry — see
[`gateway/src/resilience.rs`](../../crates/brainiac-gateway/src/resilience.rs) and
[`lighttrack.rs`](../../crates/brainiac-gateway/src/providers/mod.rs).

## Selecting the default provider

```bash
BRAINIAC_PROVIDER=anthropic     # qwen (default) | openai | azure | openai-compat
                                #  | anthropic | google | bedrock | vertex | mock
BRAINIAC_MODEL=claude-opus-4-8  # optional: pin the default model for that provider
```

`BRAINIAC_PROVIDER` picks the engine; `BRAINIAC_MODEL` optionally overrides its
default model. Set the credential(s) for the provider you chose (matrix below).

## Provider matrix

| `BRAINIAC_PROVIDER` | Required env | Optional env | Default model | Notes |
|---|---|---|---|---|
| `qwen` | `DASHSCOPE_API_KEY` or `QWEN_API_KEY` | `QWEN_MODEL`, `QWEN_BASE_URL` | `qwen-max` | OpenAI-compatible. The deploy default. |
| `openai` | `OPENAI_API_KEY` | `OPENAI_MODEL`, `OPENAI_BASE_URL` | `gpt-4o-mini` | Also the base for every OpenAI-compatible vendor — just set `OPENAI_BASE_URL`. |
| `openai-compat` | `BRAINIAC_OPENAI_COMPAT_BASE_URL`, `BRAINIAC_OPENAI_COMPAT_KEY` (or `OPENAI_API_KEY`) | `BRAINIAC_OPENAI_COMPAT_MODEL` | — | Explicit escape hatch for Groq / Together / DeepSeek / OpenRouter / Ollama / vLLM. |
| `azure` | `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_API_KEY`, `AZURE_OPENAI_DEPLOYMENT` | `AZURE_OPENAI_API_VERSION` (default `2024-10-21`) | = deployment | `api-key` header + `api-version` query. |
| `anthropic` | `ANTHROPIC_API_KEY` | `ANTHROPIC_MODEL`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_VERSION` | `claude-sonnet-4-5` | Messages API. |
| `google` (alias `gemini`) | `GEMINI_API_KEY` (or `GOOGLE_API_KEY` / `GOOGLE_GENERATIVE_AI_API_KEY`) | `GOOGLE_MODEL`, `GOOGLE_BASE_URL` | `gemini-2.5-flash` | Generative Language API. |
| `bedrock` | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | `AWS_REGION` (default `us-east-1`), `AWS_SESSION_TOKEN`, `BEDROCK_MODEL`, `BEDROCK_BASE_URL` | `anthropic.claude-3-5-sonnet-20241022-v2:0` | SigV4-signed; Claude-on-Bedrock. |
| `vertex` | `GOOGLE_CLOUD_PROJECT` + one of: `VERTEX_ACCESS_TOKEN`, `VERTEX_SA_JSON`, `GOOGLE_APPLICATION_CREDENTIALS` | `VERTEX_LOCATION` (default `us-central1`), `VERTEX_MODEL`, `VERTEX_BASE_URL` | `gemini-2.5-flash` | Service-account RS256 JWT → OAuth (cached), or a pre-minted token. Gemini models. |
| `mock` | — | — | — | Deterministic; dev/offline only. |

### Vendors reachable through `openai` / `openai-compat`

Point `OPENAI_BASE_URL` (or `BRAINIAC_OPENAI_COMPAT_BASE_URL`) at the vendor's `/v1`:

| Vendor | Base URL |
|---|---|
| Groq | `https://api.groq.com/openai/v1` |
| Together | `https://api.together.xyz/v1` |
| DeepSeek | `https://api.deepseek.com/v1` |
| Mistral | `https://api.mistral.ai/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| Ollama (local) | `http://localhost:11434/v1` |
| vLLM (self-host) | `http://<host>:8000/v1` |

## Per-stage routing (mix providers)

The pipeline has five LLM stages (`extract`, `resolve`, `contradict`, `compose`,
`divergence`). Each can override the default provider with any `provider:model`
spec — including a *different* provider — so you can spend on the stage that needs
quality and save on the rest:

```bash
BRAINIAC_PROVIDER=qwen                              # cheap default for adjudication
BRAINIAC_MODEL_EXTRACT=anthropic:claude-opus-4-8    # strong extractor
BRAINIAC_MODEL_COMPOSE=openai:gpt-4o                 # strong writer for KB pages
```

An empty override falls back to the default. The spec grammar is
`provider` or `provider:model`; the split is on the first `:` only, so Bedrock model
ids that contain a colon (`…-v2:0`) survive intact.

## Embeddings

`BRAINIAC_EMBEDDER` is independent of the chat provider:

| Value | Env | Model | Dim |
|---|---|---|---|
| `qwen` (default) | `DASHSCOPE_API_KEY`/`QWEN_API_KEY` | `text-embedding-v4` | 1024 |
| `openai` | `OPENAI_API_KEY` (+ `OPENAI_EMBED_MODEL`, `OPENAI_EMBED_DIM`) | `text-embedding-3-small` | 1536 |
| `deterministic` | — | none (model-free) | — |

> The serving embedder is pinned per deployment. Changing embedders changes the
> vector space, so re-embed the corpus (`brainiac reembed --embedder <name>`) after
> switching — mixing spaces silently degrades retrieval.

## Resilience & telemetry knobs (apply to every provider)

`BRAINIAC_GATEWAY_CONNECT_TIMEOUT_SECS` (10), `BRAINIAC_GATEWAY_REQUEST_TIMEOUT_SECS`
(60), `BRAINIAC_GATEWAY_RETRIES` (3), `BRAINIAC_GATEWAY_BASE_DELAY_MS` (500),
`BRAINIAC_GATEWAY_BREAKER_THRESHOLD` (5), `BRAINIAC_GATEWAY_BREAKER_COOLDOWN_SECS`
(30). LightTrack: `LIGHTTRACK_URL` (unset ⇒ disabled), `LIGHTTRACK_KEY`,
`LIGHTTRACK_PROJECT` — each call is metered with its real provider + model so the
LightTrack price book attributes cost per vendor.
