# ScholarLens

[English](./README.md) | [中文](./README_zh.md)

ScholarLens 是一个基于 Rust + Vite 的学术文献检索与筛选系统。它可以从多个学术数据源检索论文，补全元数据，查询期刊指标，使用大模型改善中文检索和语义筛选，并导出 CSV / BibTeX 结果。

## 核心特性

- 多源检索：OpenAlex、Semantic Scholar、arXiv、PubMed、bioRxiv、medRxiv。
- 中文友好检索：中文/CJK 关键词会被翻译和扩展为英文检索词，同时保留中文原词作为补充检索。
- Web 配置大模型服务商：在 `/settings` 页面管理 OpenAI 兼容服务商，保存到 SQLite 后即时生效。
- 同时支持 OpenAI 兼容的 `/v1/chat/completions` 和 `/v1/responses` 接口。
- 异步任务流程：提交检索任务，轮询进度，完成后下载结构化结果。
- EasyScholar 期刊指标查询：支持 SQLite 缓存和多 key 调度。
- Crossref 与 Semantic Scholar DOI 批量补全元数据。
- 支持按相关性、影响因子、PDF 可用性排序。
- 支持导出任务 JSON、`results.csv` 和 BibTeX。

## 架构概览

```text
浏览器 / Vite 前端
        |
        v
Rust Axum API  ---- SQLite：任务、管理员 key、缓存、统计、LLM 服务商
        |
        +-- 检索源：OpenAlex / Semantic Scholar / arXiv / PubMed / xRxiv
        +-- 元数据补全：Crossref / Semantic Scholar DOI 批量查询
        +-- 期刊指标：EasyScholar key 池 + 缓存
        +-- 大模型运行时：关键词翻译、关键词扩展、相关性过滤
```

## 快速开始

### 1. 准备配置

```bash
cp config.example.toml config.toml
```

编辑 `config.toml`：

- 如果需要期刊指标和指标筛选，在 `[easyscholar].keys` 中填入至少一个 EasyScholar key。
- 如果需要在 Web 页面配置模型服务商，把 `[server].admin_enabled` 设置为 `true`。
- 不要把真实 key 提交到 git。`config.toml`、`data/`、`target/` 和前端构建产物默认会被忽略。

### 2. 启动应用

Windows PowerShell：

```powershell
.\start.ps1
```

Linux/macOS：

```bash
./start.sh
```

启动脚本会安装/构建前端，构建 Rust 后端，并在以下地址提供服务：

```text
http://localhost:3000
```

## 大模型服务商配置

大模型不是必须项，但强烈建议配置。它能显著改善中文检索召回，并支持语义相关性过滤。

### 通过 Web 页面配置

1. 在 `config.toml` 中启用管理接口：

```toml
[server]
admin_enabled = true
```

2. 创建第一个管理员 key：

```bash
cargo run -- init-admin --name Admin
```

3. 启动服务后打开：

```text
http://localhost:3000/settings
```

4. 粘贴管理员 key，新增模型服务商，点击“保存并测试连通性”。

服务商字段说明：

| 字段 | 说明 |
|---|---|
| `name` | 稳定的服务商名称，例如 `openai-compatible` 或 `modelverse` |
| `enabled` | 是否启用该服务商 |
| `interface_type` | `chat_completions` 或 `responses` |
| `endpoint` | 可填写完整 endpoint 或 Base URL，系统会自动规范化 |
| `model` | 发送给服务商的模型名称 |
| `api_key` | 服务商 API key。编辑已有服务商时留空会保留旧 key |
| `order` | 优先级，数字越小越先尝试 |

Endpoint 自动补全示例：

| 接口类型 | 输入 Endpoint | 实际请求 Endpoint |
|---|---|---|
| `chat_completions` | `https://api.example.com` | `https://api.example.com/v1/chat/completions` |
| `responses` | `https://api.example.com/v1` | `https://api.example.com/v1/responses` |

### 通过配置文件配置

仍然可以在 `config.toml` 的 `[llm]` 和 `[llm.registry.<name>]` 中配置启动默认服务商。通过 Web 页面保存到 SQLite 的服务商优先级更高，并且保存后会立即重建内存运行时。

## 中文检索如何工作

ScholarLens 会对中文/CJK 查询使用专门的检索计划：

1. 保留用户输入的中文原词。
2. 如果大模型可用，先翻译成更适合英文论文库的学术查询词。
3. 生成相关英文扩展词。
4. 使用英文翻译和扩展词作为主要检索词查询英文友好的数据源。
5. 在合适的数据源上使用中文原词做低成本补充检索或失败回退。
6. 按 DOI/title 合并去重。
7. 本地相关性评分支持 CJK 2-gram，避免中文 `content_help` 被当成一个超长 token。

例如输入 `机器学习 岩石强度预测`，系统可以检索到英文数据库中的相关论文，而不是只依赖数据源对中文关键词的原生支持。

## 大模型在项目里做什么

大模型在 ScholarLens 中主要用于三件事：

- **关键词翻译**：把中文或其他非英文关键词转成英文检索词，提高召回率。
- **关键词扩展**：生成相关学术术语，避免只搜用户输入的几个词。
- **相关性过滤**：当填写 `content_help` 且模型可用时，阅读论文标题/摘要/期刊信息，判断是否符合研究方向。

关键词翻译和扩展可以独立于 LLM 相关性过滤运行。也就是说，即使不想让大模型筛论文，也可以用它改善中文检索。

## HTTP API 概览

公开接口：

| 方法 | 路径 | 用途 |
|---|---|---|
| `GET` | `/health` | 健康检查 |
| `POST` | `/tasks` | 创建异步文献检索任务 |
| `GET` | `/tasks` | 获取最近任务列表 |
| `GET` | `/tasks/{id}` | 查询任务状态/结果 |
| `GET` | `/tasks/{id}/download` | 下载 CSV |
| `GET` | `/tasks/{id}/bibtex` | 下载 BibTeX |
| `GET` | `/sources` | 获取已启用检索源 |

管理接口使用 `/api/v1/admin/*` 前缀，并要求请求头 `X-API-Key` 为管理员 key。管理接口包括 API key 管理、缓存管理、统计信息、系统状态和 LLM 服务商管理。

完整接口说明见 [`docs/API.md`](./docs/API.md) 和 [`docs/API_zh.md`](./docs/API_zh.md)。

## 检索请求示例

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

`source_include` / `source_exclude` 支持的值：

- `openalex`
- `semanticscholar`
- `arxiv`
- `pubmed`
- `biorxiv`
- `medrxiv`

## Pipeline 阶段

1. 校验请求和检索源选择。
2. 构建检索计划，包括中文/CJK 翻译和扩展。
3. 并发查询已启用的数据源。
4. 合并和去重论文。
5. 通过 Crossref 和 Semantic Scholar 补全缺失元数据。
6. 查询 EasyScholar 指标并应用指标筛选。
7. 可选执行 LLM 相关性过滤。
8. 计算相关性分数并排序。
9. 持久化任务，保存 CSV，开放下载接口。

## 项目结构

```text
src/
  cli/                     CLI 命令：search、server、init-admin
  db/                      SQLite schema 与 CRUD 模块
  llm/                     LLM 运行时、服务商、翻译、扩展
  ranking/                 EasyScholar 客户端池和指标调度器
  server/                  Axum 路由、handler、pipeline、管理接口
  sources/                 检索和元数据补全客户端
front/                     Vite 前端
front/src/pages/settings.js Web 模型服务商配置页
docs/                      API 与架构文档
tests/                     单元、集成和在线测试
```

## 开发与测试

前端：

```bash
cd front
npm install
npm run build
```

Rust 测试：

```bash
cargo test
```

LLM 服务商和中文检索专项测试：

```bash
cargo test --test llm_provider_and_zh_search
```

如果已安装 clippy：

```bash
cargo clippy --all-targets -- -D warnings
```

## 持久化与输出

- SQLite 数据库：`data/rscholar.db`
- CSV 结果：`output/{timestamp}_{sanitized_keyword}/results.csv`
- 服务重启后仍可查询已完成任务和持久化结果。
- 中断中的运行任务会在重启恢复流程中标记为失败。

## 安全说明

- 管理接口需要 `X-API-Key` 和管理员权限。
- 管理员 API key 使用 HMAC-SHA256 哈希存储。
- LLM 服务商 API key 存储在本地 SQLite 中，列表接口只返回脱敏状态。
- 不要提交 `config.toml`、本地数据库或真实 API key。

## 相关文档

- [`docs/API.md`](./docs/API.md)
- [`docs/API_zh.md`](./docs/API_zh.md)
- [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md)
