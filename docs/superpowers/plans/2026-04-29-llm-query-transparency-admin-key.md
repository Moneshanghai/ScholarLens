# LLM Query Transparency and Admin Key Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add explicit LLM search controls, persist/display final query terms, and let admins replace generated keys with custom keys.

**Architecture:** The backend owns behavior and persistence: request-level `enable_llm` gates all LLM calls, and task completion stores a JSON `query_plan` summary. The frontend sends the flag, renders the returned summary, and calls a new authenticated admin endpoint to replace the current key.

**Tech Stack:** Rust/Axum/SQLite backend, Vite vanilla JS frontend, existing localStorage admin-key pattern.

---

### Task 1: Backend Tests

**Files:**
- Modify: `src/server/pipeline.rs`
- Modify: `src/db/api_keys.rs`

- [ ] Add failing tests for `enable_llm=false`, query-plan metadata, and API key replacement.
- [ ] Run targeted tests with `C:\Users\Shang\.cargo\bin\cargo.exe test pipeline::tests::test_pipeline_config_disable_request_llm db::api_keys::tests::test_replace_admin_key_with_custom_value db::api_keys::tests::test_replace_key_requires_admin --lib` and confirm they fail because code is missing.

### Task 2: Backend Implementation

**Files:**
- Modify: `src/server/pipeline.rs`
- Modify: `src/server/task.rs`
- Modify: `src/db/tasks.rs`
- Modify: `src/server/handlers.rs`
- Modify: `src/db/api_keys.rs`
- Modify: `src/server/admin.rs`
- Modify: `src/server/routes.rs`

- [ ] Add `PipelineRequest.enable_llm` and `PipelineConfig.enable_llm`.
- [ ] Gate translation, expansion, relevance filtering, and LLM scoring on `enable_llm`.
- [ ] Build and persist `query_plan` JSON in `TaskResult`.
- [ ] Return `query_plan` from `GET /tasks/{id}`.
- [ ] Add `api_keys::replace_key` and `PATCH /api/v1/admin/admin-key`.
- [ ] Run the targeted Rust tests and confirm they pass.

### Task 3: Frontend Implementation

**Files:**
- Modify: `front/src/pages/home.js`
- Modify: `front/src/pages/task.js`
- Modify: `front/src/pages/settings.js`
- Modify: `front/src/api/client.js`
- Modify: `front/src/styles/index.css`
- Create: `front/src/utils/query-plan.js`

- [ ] Add an enabled-by-default LLM enhancement checkbox to the search form.
- [ ] Send `enable_llm` in `createTask` payloads.
- [ ] Render `query_plan` on search results and task detail pages.
- [ ] Add settings controls for new custom Admin API Key and confirmation.
- [ ] Add API client function for `PATCH /api/v1/admin/admin-key`.
- [ ] Run frontend build/syntax verification with bundled Node if npm/node are available.

### Task 4: Final Verification

**Files:**
- All changed files

- [ ] Run targeted Rust tests.
- [ ] Run broader Rust tests if feasible.
- [ ] Run frontend build/syntax check if feasible.
- [ ] Review `git diff` for accidental secrets, unrelated edits, and malformed Chinese text.
