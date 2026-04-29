# Relevance Sorting Design

Date: 2026-04-28

## Goal

Add accurate relevance sorting for search results while preserving the existing impact-factor sort as an available option.

## Recommended Behavior

- Default API and frontend sorting is relevance-first.
- Relevance uses `keyword`, optional `content_help`, title, abstract, and venue.
- LLM-enabled deployments get semantic relevance scores from 0 to 100.
- If LLM scoring is unavailable or fails for an item, the system falls back to deterministic local scoring.
- Final order for relevance mode is `relevance_score desc`, then `if_score desc`, then `year desc`, then original result order.
- Existing IF sorting remains available through `sort_by = "impact_factor"` and the frontend table header.

## API Shape

Request field:

- `sort_by`: optional string, one of `relevance` or `impact_factor`; omitted defaults to `relevance`.

Response/CSV fields per paper:

- `relevance_score`: integer 0-100.
- `relevance_reason`: short text explaining LLM or fallback scoring.

## Backend Design

- Extend `PipelineRequest` and `PipelineConfig` with `sort_by`.
- Add a focused sorting/scoring module under `src/server/pipeline/relevance.rs`.
- The module owns:
  - `SortBy` enum validation and defaults.
  - local lexical fallback scoring.
  - stable result ordering helpers.
- Extend `LlmRelevanceFilter` with score-oriented methods while retaining the existing YES/NO filter behavior.
- Score prompt requires compact JSON with `score` and `reason`; parser accepts JSON inside markdown fences and clamps score to 0-100.
- Pipeline flow scores after metadata/ranking enrichment and before CSV save.

## Frontend Design

- Add a sort selector to the form, defaulting to relevance.
- Include `sort_by` in `POST /tasks`.
- Default completed results to relevance sorting.
- Add a sortable relevance column before IF.

## Testing

- Unit-test `SortBy` defaults and validation.
- Unit-test relevance sorting tie-breakers.
- Unit-test local fallback scoring ranks exact title/abstract matches above weak matches.
- Unit-test LLM score response parsing.
- Run targeted Rust tests, full Rust tests, and frontend production build.
