# Embedding model options — bake-off candidates & deployment matrix

*Researched 2026-07-10 (web sweep). Feeds the EVAL.md §3.1 bake-off. The
deterministic bag-of-tokens embedder stays the zero-dependency plumbing
default; everything below plugs in behind `brainiac_core::embed::Embedder`.
Current headroom to close: **semantic stratum NDCG@10 0.422** and **czech
0.560** on the expanded corpus (deterministic baseline, results/history/).*

## Decision matrix (start here)

| Deployment scenario | Pick | Runner-up | Why |
|---|---|---|---|
| Self-hosted, CPU-only (Brainiac default) | **bge-m3** @1024-d | Qwen3-Embedding-0.6B @768-d (MRL) | MIT license, mature ONNX/fastembed path, dense+sparse output complements our FTS leg natively; Qwen3-0.6B wins on quality/context but its ONNX tooling is younger |
| Self-hosted, small GPU | **Qwen3-Embedding-4B** (MRL→1024) | snowflake-arctic-embed-l-v2.0 | Near-SOTA multilingual, Apache-2.0, TEI-servable; pair with Qwen3-Reranker-0.6B |
| Alibaba Cloud (hackathon deployment) | **DashScope text-embedding-v4** @1024-d | self-hosted Qwen3-0.6B on ECS | Same Qwen3 family as our BYOM chat engine, cheapest hosted option (~$0.07/1M tok intl), OpenAI-compatible endpoint we already speak, batch API |
| GCP | **gemini-embedding-001** truncated to 768/1536 | DashScope intl cross-cloud | Best hosted quality; batch halves cost to $0.075/1M; MRL truncation keeps pgvector rows small |

Reranker (post-RRF stage, not yet built): **bge-reranker-v2-m3** (Apache-2.0)
default; **Qwen3-Reranker-0.6B** (Apache-2.0, instruction-aware) as the
alternative. Avoid jina rerankers (CC-BY-NC).

## A) Open-source (self-hosted)

| Model | Dims (MRL?) | Ctx | Params | License | Notes |
|---|---|---|---|---|---|
| **bge-m3** | 1024 (no MRL; + sparse + ColBERT heads) | 8192 | ~568M | MIT | Best all-rounder for hybrid retrieval — built-in sparse output could complement or replace our FTS leg. Strong MIRACL multilingual; good Slavic reputation. ONNX/TEI/fastembed-ready. CPU: tens of docs/sec batched. |
| **Qwen3-Embedding 0.6B / 4B / 8B** | 1024 / 2560 / 4096, all MRL (64–2048+) | 32k | 0.6B–8B | Apache-2.0 | 8B = #1 MTEB multilingual (70.58); 100+ languages; instruction-aware. 0.6B is the CPU-viable one (GGUF via llama.cpp/Ollama; community ONNX). Effectively supersedes the gte-Qwen line. |
| **snowflake-arctic-embed-l-v2.0** | 1024, MRL (→256 with <3% loss) | 8192 | 568M | Apache-2.0 | MIRACL 55.8; built on the bge-m3 architecture; TEI/ONNX friendly. m-v2.0 (305M) is the lighter variant. |
| **nomic-embed-text-v2-moe** | 768→256 MRL | 512 | 475M (305M active) | Apache-2.0 | ~100 languages, MoE = fast/token. Short context is the main limit for transcript chunks. |
| **multilingual-e5-large(-instruct)** | 1024 | 512 | 560M | MIT | Battle-tested on Slavic languages but 2023-era; outclassed by bge-m3/Qwen3. Fallback only. |
| gte-large-en-v1.5 | 1024 | 8192 | 434M | Apache-2.0 | **English-only — disqualified** (Czech stratum). |
| jina-embeddings-v3 | 1024 MRL | 8192 | 570M | **CC-BY-NC-4.0** | **No commercial self-hosting** without a paid license. Avoid. |

**Czech caveat:** none of these publish Czech-labeled benchmark numbers
(MIRACL/CLEF exclude Czech). bge-m3, multilingual-e5, and Qwen3-Embedding all
include Czech in training data; community Slavic results are good but
unverified — which is exactly why our fixture corpus carries a Czech slice.
The bake-off will produce the first hard numbers we trust.

## B) Hosted services (spare the compute layer in cloud deployments)

- **Alibaba DashScope / Model Studio — `text-embedding-v4`** (served
  Qwen3-Embedding): dims 64–2048 (default 1024), 8192-token input, 10
  texts/request, dense+sparse output, 100+ languages. **$0.07/1M tokens**
  (Singapore/HK intl; $0.072 Beijing), 1M free tokens for 90 days (intl).
  OpenAI-compatible endpoint — same `dashscope-intl.aliyuncs.com/compatible-mode/v1`
  base our `QwenProvider` already targets — plus an OpenAI-compatible batch API.
- **Google Vertex / Gemini — `gemini-embedding-001`**: 3072-d default,
  MRL-truncatable to 1536/768 with minimal loss. $0.15/1M input tokens,
  **$0.075/1M batch**. Replaced text-embedding-005 / text-multilingual-embedding-002
  (legacy/retiring — verify deprecation dates before committing).
- Reference points: OpenAI text-embedding-3-large 3072-d MRL @ $0.13/1M;
  Voyage / Cohere embed-v4 in the ~$0.06–0.12/1M band (not verified this
  sweep — spot-check before quoting).

## pgvector implications

- 4 bytes/dim: 768-d ≈ 3 KB, 1024-d ≈ 4 KB, 3072-d ≈ 12 KB per row before
  HNSW overhead; index build time and RAM scale roughly linearly.
- pgvector indexes cap at **2000 dims** — gemini's 3072 must be truncated (or
  use `halfvec`). Matryoshka truncation to 768–1024 is the right lever
  everywhere it's supported (Qwen3, arctic, gemini, DashScope v4).
- **Never mix models/dims/versions in one vector column.** Our schema already
  stamps `embedding_versions(model, dim)` per row — the migration path is
  dual-write under a new version id, re-embed per org, cut retrieval over,
  then garbage-collect the old version.

## Bake-off plan (next step)

1. Implement `Embedder` adapters: ONNX-runtime local (bge-m3 first) and an
   HTTP adapter for DashScope v4 (reuses gateway auth patterns; API key stays
   an env/vault reference — never in Postgres).
2. Run the `retrieval` profile per candidate on the expanded corpus; append
   each report to `results/history/` with the model name in the filename.
3. Decide on the four cells of the matrix above with real NDCG deltas —
   semantic and czech strata are the discriminators; exact-identifier should
   stay ≥0.9 (FTS leg carries it regardless of embedder).

### Sources

Qwen3 Embedding blog (qwenlm.github.io) · Alibaba Model Studio embedding docs ·
Google Developers blog + gemini-embedding-001 pricing guides · HF model cards
(bge-m3, arctic-embed-l-v2.0, nomic-embed-text-v2-moe, Qwen3-Embedding-0.6B-GGUF) ·
Jina v3 announcement (license) · FutureAGI reranker comparison 2026 · OpenAI
embedding pricing page.
