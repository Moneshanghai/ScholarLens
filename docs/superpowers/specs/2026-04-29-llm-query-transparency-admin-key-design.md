# LLM Search Transparency and Admin Key Design

## Goal
Make LLM usage explicit and controllable during searches, show the final search keywords used by each task, and allow an existing administrator to replace the generated Admin API Key with a custom value.

## Scope
- Add a per-search "LLM enhancement" switch on the home page.
- Persist and return a query plan summary with completed task results.
- Display the query plan on immediate results and historical task detail pages.
- Add an authenticated admin endpoint to replace the current Admin API Key.
- Add settings-page controls for entering a custom Admin API Key twice before saving.

## Behavior
- When LLM enhancement is enabled, the pipeline may use configured providers for keyword translation, keyword expansion, relevance filtering, and relevance scoring.
- When LLM enhancement is disabled, the pipeline must not call LLM providers for translation, expansion, filtering, or scoring; search uses the original keyword and local scoring only.
- Completed task results include a `query_plan` object with original keyword, primary keyword, OpenAlex query, non-OpenAlex query, fallback keywords, expanded terms, and booleans showing which LLM steps actually ran.
- The settings page uses the existing Admin API Key as the authenticated current key. The new custom key must be entered twice and match before submitting.
- The backend stores only the HMAC hash of the new key. The plaintext key is returned only to the browser through the user's own form state/localStorage update.

## Data Contract
`POST /tasks` accepts optional `enable_llm: boolean` and defaults to true.

Completed task `result.query_plan` contains:
- `llm_enabled`
- `original_keyword`
- `primary_keyword`
- `translated_keyword`
- `openalex_query`
- `source_query`
- `expanded_terms`
- `fallback_keywords`
- `used_llm_translation`
- `used_llm_expansion`
- `used_llm_filtering`
- `used_llm_scoring`

`PATCH /api/v1/admin/admin-key` accepts `{ "current_key": "...", "new_key": "..." }` and requires the normal `X-API-Key` admin header.

## Testing
- Unit test `PipelineConfig` to verify `enable_llm=false` disables relevance LLM behavior.
- Unit test query-plan JSON construction.
- Unit test Admin API Key replacement: old key stops validating, new key validates, non-admin keys cannot replace admin keys.
- Run targeted Rust tests and a frontend production build/syntax check when available.
