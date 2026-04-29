# Relevance Sorting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build relevance-first paper sorting with LLM semantic scoring and deterministic local fallback.

**Architecture:** The backend owns canonical scoring and result order. A new focused pipeline relevance module validates sort mode, computes fallback scores, and sorts results; the existing LLM filter gains score parsing and batch scoring. The frontend only sends the selected sort mode and displays/sorts returned `relevance_score` values.

**Tech Stack:** Rust 2021, Tokio, Serde/serde_json, existing LLM provider abstraction, Vite frontend JavaScript.

---

## File Map

- Modify: `src/llm/mod.rs` for `RelevanceScore` parsing and batch scoring.
- Create: `src/server/pipeline/relevance.rs` for `SortBy`, local scoring, and stable ordering.
- Modify: `src/server/pipeline.rs` for request/config fields, scoring stage, result serialization, and tests.
- Modify: `src/server/pipeline/export.rs` for CSV relevance columns.
- Modify: `front/src/pages/home.js` for sort mode selector and relevance table sorting.
- Modify: `docs/API.md`, `docs/API_zh.md`, `README.md`, `README_zh.md` for API and behavior docs.

## Task 1: Backend Failing Tests

- [ ] **Step 1: Add tests before production implementation**

Add tests that reference the intended public functions:

```rust
#[test]
fn test_relevance_sort_orders_by_score_then_if_then_year() {
    let mut papers = vec![paper("weak", 40, Some("30.0"), "2024"), paper("strong", 90, Some("1.0"), "2020")];
    sort_papers(&mut papers, SortBy::Relevance);
    assert_eq!(papers[0].title, "strong");
}
```

- [ ] **Step 2: Run targeted tests to confirm RED**

Run: `cargo test server::pipeline::tests::test_relevance_sort_orders_by_score_then_if_then_year --lib`
Expected: FAIL because `SortBy`, `sort_papers`, or relevance fields do not exist yet.

## Task 2: Backend Implementation

- [ ] **Step 1: Implement minimal relevance module**

Add `SortBy`, `local_relevance_score`, `sort_papers`, and helpers in `src/server/pipeline/relevance.rs`.

- [ ] **Step 2: Wire scoring into pipeline**

Add `sort_by` to request/config, `relevance_score` and `relevance_reason` to `PaperResult`, compute scores after filters, and sort before saving.

- [ ] **Step 3: Run targeted tests to confirm GREEN**

Run: `cargo test server::pipeline::tests::test_relevance_sort_orders_by_score_then_if_then_year --lib`
Expected: PASS.

## Task 3: LLM Score Parsing and Batch Scoring

- [ ] **Step 1: Add parser tests**

Test JSON, markdown-wrapped JSON, and score clamping for `parse_relevance_score_response`.

- [ ] **Step 2: Run parser test to confirm RED**

Run: `cargo test llm::tests::test_parse_relevance_score_response_json --lib`
Expected: FAIL because parser does not exist.

- [ ] **Step 3: Implement parser and batch scoring**

Add `RelevanceScore`, scoring prompt, `score_relevance`, and `batch_score_relevance` without removing existing filtering methods.

- [ ] **Step 4: Run parser and pipeline tests**

Run: `cargo test llm::tests::test_parse_relevance_score_response_json server::pipeline::tests::test_relevance_sort_orders_by_score_then_if_then_year --lib`
Expected: PASS.

## Task 4: Frontend and Docs

- [ ] **Step 1: Add frontend sort selector**

Add a `<select name="sort_by">` with relevance as default and impact factor as optional.

- [ ] **Step 2: Include sort mode in request and table**

Send `sort_by`, default displayed sort to relevance, add a `相关性` column, and make it sortable.

- [ ] **Step 3: Update documentation**

Document `sort_by`, `relevance_score`, `relevance_reason`, and default relevance ordering.

## Task 5: Verification

- [ ] **Step 1: Run Rust unit tests**

Run: `cargo test --lib`
Expected: exit 0.

- [ ] **Step 2: Run integration tests**

Run: `cargo test --tests`
Expected: exit 0 or clearly identify live/network tests if external credentials are missing.

- [ ] **Step 3: Build frontend**

Run: `npm run build` in `front`
Expected: exit 0.

- [ ] **Step 4: Review diff**

Run: `git diff --stat` and inspect touched files.
