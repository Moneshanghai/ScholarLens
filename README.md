# ScholarLens

[English](./README.md) | [中文](./README_zh.md)

ScholarLens is a Rust + Vite application for academic literature discovery. It searches multiple scholarly sources, enriches metadata, ranks journals, optionally uses LLMs for translation/expansion/relevance filtering, and exports results as CSV or BibTeX.

## Highlights

- Multi-source literature search: OpenAlex, Semantic Scholar, arXiv, PubMed, bioRxiv, and medRxiv.
- Chinese-friendly search: Chinese/CJK keywords are translated and expanded into English queries while keeping the original keyword as fallback.
- Web-configurable LLM providers: manage OpenAI-compatible providers from `/settings` without restarting the server.
- Supports both OpenAI-compatible `/v1/chat/completions` and `/v1/responses` interfaces.
- Async task workflow: submit a search task, poll progress, then download structured results.
- Journal ranking filters powered by EasyScholar, with SQLite caching and key-pool scheduling.
- Metadata enrichment through Crossref and Semantic Scholar DOI lookups.
- Result sorting by relevance, impact factor, or PDF availability.
- Export formats: task JSON, `results.csv`, and BibTeX.

## Architecture At A Glance

```text
Browser / Vite UI
        |
        v
Rust Axum API  ---- SQLite: tasks, admin keys, cache, analytics, LLM providers
        |
        +-- Search sources: OpenAlex / Semantic Scholar / arXiv / PubMed / xRxiv
        +-- Enrichment: Crossref / Semantic Scholar DOI batch lookup
        +-- Ranking: EasyScholar key pool + cache
        +-- LLM runtime: keyword translation, keyword expansion, relevance filtering
```

## Quick Start

### 1. Prepare configuration

```bash
cp config.example.toml config.toml
```

Edit `config.toml`:

- Add at least one `[easyscholar].keys` value if you want journal ranking and ranking filters.
- Set `[server].admin_enabled = true` if you want to use the web settings page for LLM providers.
- Keep private keys out of git. `config.toml`, `data/`, `target/`, and frontend build output are ignored.

### 2. Start the app

Windows PowerShell:

```powershell
.\start.ps1
```

Linux/macOS:

```bash
./start.sh
```

The startup script installs/builds the frontend, builds the Rust backend, and serves the app at:

```text
http://localhost:3000
```

## LLM Provider Setup

LLMs are optional, but recommended for Chinese search and semantic filtering.

### Web setup

1. Enable admin routes in `config.toml`:

```toml
[server]
admin_enabled = true
```

2. Create the first admin password:

```bash
cargo run -- init-admin --name Admin --key 123456
```

On a fresh deployment, the server will also create the first admin password automatically when it starts. Set `SCHOLARLENS_ADMIN_PASSWORD` to use your own value; otherwise the initial password is `123456`.

3. Start the server and open:

```text
http://localhost:3000/settings
```

4. Paste the admin key, add a provider, and click **Save and Test Connection**.

Provider fields:

| Field | Description |
|---|---|
| `name` | Stable provider name, for example `openai-compatible` or `modelverse` |
| `enabled` | Whether the runtime can use this provider |
| `interface_type` | `chat_completions` or `responses` |
| `endpoint` | Full endpoint or base URL. Base URLs are normalized automatically. |
| `model` | Model name sent to the provider |
| `api_key` | Provider API key. Omit it when editing to keep the existing key. |
| `order` | Lower values are tried earlier |

Endpoint normalization examples:

| Interface | Input endpoint | Request endpoint |
|---|---|---|
| `chat_completions` | `https://api.example.com` | `https://api.example.com/v1/chat/completions` |
| `responses` | `https://api.example.com/v1` | `https://api.example.com/v1/responses` |

### Config-file setup

`config.toml` can still define startup defaults under `[llm]` and `[llm.registry.<name>]`. Providers saved in SQLite from the web UI take priority and are reloaded immediately after changes.

## Chinese Search Behavior

ScholarLens treats CJK queries differently from plain English queries:

1. Keep the original Chinese keyword.
2. Use the configured LLM runtime to translate it into an English academic query when available.
3. Generate related English expansion terms.
4. Search English-friendly sources with the translated/expanded terms.
5. Run low-cost fallback/supplemental searches with the original keyword where useful.
6. Merge and deduplicate results by DOI/title.
7. Use CJK-aware local tokenization for relevance scoring, so Chinese `content_help` is not treated as one long token.

This means a query such as `机器学习 岩石强度预测` can retrieve English papers even when a source performs poorly on Chinese keywords.

## LLM Usage In The Pipeline

LLMs are used for three independent jobs:

- **Keyword translation**: improves non-English search recall.
- **Keyword expansion**: adds related academic terms for broader coverage.
- **Relevance filtering**: when `content_help` is provided and providers are available, the LLM evaluates whether each paper matches the user's research intent.

LLM translation/expansion can run even if strict LLM relevance filtering is disabled.

## HTTP API Overview

Public endpoints:

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/health` | Health check |
| `POST` | `/tasks` | Create an async literature search task |
| `GET` | `/tasks` | List recent tasks |
| `GET` | `/tasks/{id}` | Read task status/result |
| `GET` | `/tasks/{id}/download` | Download CSV |
| `GET` | `/tasks/{id}/bibtex` | Download BibTeX |
| `GET` | `/sources` | List enabled search sources |

Admin endpoints use the `/api/v1/admin/*` prefix and require `X-API-Key` with an admin key. They include API key management, cache management, analytics, system status, and LLM provider management.

See [`docs/API.md`](./docs/API.md) for detailed request and response schemas.

## Search Request Example

```json
{
  "keyword": "机器学习 岩石强度预测",
  "ylo": 2021,
  "enable_crossref": true,
  "sciif": 3.0,
  "content_help": "只关注使用机器学习预测岩石单轴抗压强度的论文",
  "source_include": ["openalex", "semanticscholar", "arxiv", "pubmed"],
  "sort_by": "relevance"
}
```

Supported `source_include` / `source_exclude` values:

- `openalex`
- `semanticscholar`
- `arxiv`
- `pubmed`
- `biorxiv`
- `medrxiv`

## Pipeline Stages

1. Validate request and source selections.
2. Build a search query plan, including Chinese/CJK translation and expansion when applicable.
3. Search enabled sources in parallel.
4. Merge and deduplicate papers.
5. Enrich missing metadata through Crossref and Semantic Scholar.
6. Query EasyScholar rankings and apply ranking filters.
7. Optionally run LLM relevance filtering.
8. Score and sort final results.
9. Persist task data, save CSV, and expose download endpoints.

## Project Layout

```text
src/
  cli/                     CLI commands: search, server, init-admin
  db/                      SQLite schema and CRUD modules
  llm/                     LLM runtime, providers, translation, expansion
  ranking/                 EasyScholar client pool and ranking scheduler
  server/                  Axum routes, handlers, pipeline, admin APIs
  sources/                 Search and enrichment clients
front/                     Vite frontend
front/src/pages/settings.js Web LLM provider settings page
docs/                      API and architecture docs
tests/                     Unit, integration, and live tests
```

## Development

Frontend:

```bash
cd front
npm install
npm run build
```

Rust tests:

```bash
cargo test
```

Targeted LLM and Chinese-search tests:

```bash
cargo test --test llm_provider_and_zh_search
```

Clippy, if installed:

```bash
cargo clippy --all-targets -- -D warnings
```

## Persistence And Output

- SQLite database: `data/rscholar.db`
- CSV results: `output/{timestamp}_{sanitized_keyword}/results.csv`
- Task status and completed results survive server restarts.
- Interrupted running tasks are recovered as failed tasks after restart.

## Security Notes

- Admin routes require `X-API-Key` and admin privileges.
- Admin API keys are stored as HMAC-SHA256 hashes.
- LLM provider API keys are stored locally in SQLite and masked in list responses.
- Do not commit `config.toml`, local databases, or real API keys.

## Related Docs

- [`docs/API.md`](./docs/API.md)
- [`docs/API_zh.md`](./docs/API_zh.md)
- [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md)
