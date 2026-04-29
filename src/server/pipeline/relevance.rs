use crate::error::{GscholarError, Result};
use std::cmp::Ordering;
use std::collections::HashSet;

use super::PaperResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SortBy {
    Relevance,
    ImpactFactor,
    HasPdfAsc,
    HasPdfDesc,
}

impl SortBy {
    pub(super) fn from_request(value: Option<&str>) -> Result<Self> {
        let Some(raw) = value.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(Self::default());
        };

        match raw.to_lowercase().replace('-', "_").as_str() {
            "relevance" | "relevant" | "related" => Ok(Self::Relevance),
            "impact_factor" | "if_score" | "if" | "sciif" => Ok(Self::ImpactFactor),
            "has_pdf" | "pdf" | "pdf_first" | "pdf_url" => Ok(Self::HasPdfDesc),
            "pdf_last" => Ok(Self::HasPdfAsc),
            _ => Err(GscholarError::Validation(format!(
                "Unsupported sort_by '{}'. Supported values: relevance, impact_factor, has_pdf",
                raw
            ))),
        }
    }
}

impl Default for SortBy {
    fn default() -> Self {
        Self::Relevance
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalRelevanceScore {
    pub score: u8,
    pub reason: String,
}

pub(super) fn local_relevance_score(
    keyword: &str,
    content_help: Option<&str>,
    paper: &PaperResult,
) -> LocalRelevanceScore {
    let keyword_terms = unique_terms(keyword);
    let focus_terms = unique_terms(content_help.unwrap_or_default());
    let query_terms: Vec<String> = keyword_terms
        .iter()
        .chain(focus_terms.iter())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if query_terms.is_empty() {
        return LocalRelevanceScore {
            score: 50,
            reason: "Fallback score: no searchable query terms were available.".to_string(),
        };
    }

    let title = paper.title.to_lowercase();
    let abstract_text = paper.abstract_text.to_lowercase();
    let venue = paper.venue.to_lowercase();
    let title_tokens = token_set(&title);
    let abstract_tokens = token_set(&abstract_text);
    let venue_tokens = token_set(&venue);

    let title_hits = count_hits(&keyword_terms, &title_tokens);
    let abstract_hits = count_hits(&keyword_terms, &abstract_tokens);
    let focus_hits =
        count_hits(&focus_terms, &title_tokens) + count_hits(&focus_terms, &abstract_tokens);
    let venue_hits = count_hits(&keyword_terms, &venue_tokens);

    let keyword_count = keyword_terms.len().max(1) as f64;
    let focus_count = focus_terms.len().max(1) as f64;

    let title_score = (title_hits as f64 / keyword_count) * 45.0;
    let abstract_score = (abstract_hits as f64 / keyword_count) * 25.0;
    let focus_score = if focus_terms.is_empty() {
        0.0
    } else {
        (focus_hits as f64 / (focus_count * 2.0)) * 20.0
    };
    let venue_score = (venue_hits as f64 / keyword_count) * 5.0;
    let phrase_boost = phrase_boost(keyword, content_help, &title, &abstract_text);
    let coverage_boost = coverage_boost(
        title_hits,
        abstract_hits,
        keyword_terms.len(),
        focus_hits,
        focus_terms.len(),
    );

    let score =
        (title_score + abstract_score + focus_score + venue_score + phrase_boost + coverage_boost)
            .round()
            .clamp(0.0, 100.0) as u8;

    LocalRelevanceScore {
        score,
        reason: format!(
            "Local fallback score from title/abstract term overlap: title {}/{}, abstract {}/{}, focus hits {}.",
            title_hits,
            keyword_terms.len(),
            abstract_hits,
            keyword_terms.len(),
            focus_hits
        ),
    }
}

pub(super) fn sort_papers(papers: &mut Vec<PaperResult>, sort_by: SortBy) {
    let mut indexed: Vec<(usize, PaperResult)> = papers.iter().cloned().enumerate().collect();

    indexed.sort_by(|(left_index, left), (right_index, right)| {
        let ordering = match sort_by {
            SortBy::Relevance => compare_relevance(left, right),
            SortBy::ImpactFactor => compare_impact_factor(left, right),
            SortBy::HasPdfAsc => compare_pdf(left, right, false),
            SortBy::HasPdfDesc => compare_pdf(left, right, true),
        };

        ordering.then_with(|| left_index.cmp(right_index))
    });

    *papers = indexed.into_iter().map(|(_, paper)| paper).collect();
}

fn compare_relevance(left: &PaperResult, right: &PaperResult) -> Ordering {
    desc_u8(left.relevance_score, right.relevance_score)
        .then_with(|| {
            desc_f64(
                parse_metric(left.if_score.as_deref()),
                parse_metric(right.if_score.as_deref()),
            )
        })
        .then_with(|| desc_i32(parse_year(&left.year), parse_year(&right.year)))
}

fn compare_impact_factor(left: &PaperResult, right: &PaperResult) -> Ordering {
    desc_f64(
        parse_metric(left.if_score.as_deref()),
        parse_metric(right.if_score.as_deref()),
    )
    .then_with(|| desc_u8(left.relevance_score, right.relevance_score))
    .then_with(|| desc_i32(parse_year(&left.year), parse_year(&right.year)))
}

fn compare_pdf(left: &PaperResult, right: &PaperResult, pdf_first: bool) -> Ordering {
    let left_has_pdf = !left.pdf_url.trim().is_empty();
    let right_has_pdf = !right.pdf_url.trim().is_empty();
    let ordering = if pdf_first {
        right_has_pdf.cmp(&left_has_pdf)
    } else {
        left_has_pdf.cmp(&right_has_pdf)
    };

    ordering
        .then_with(|| desc_u8(left.relevance_score, right.relevance_score))
        .then_with(|| {
            desc_f64(
                parse_metric(left.if_score.as_deref()),
                parse_metric(right.if_score.as_deref()),
            )
        })
        .then_with(|| desc_i32(parse_year(&left.year), parse_year(&right.year)))
}

fn desc_u8(left: Option<u8>, right: Option<u8>) -> Ordering {
    right.unwrap_or_default().cmp(&left.unwrap_or_default())
}

fn desc_i32(left: Option<i32>, right: Option<i32>) -> Ordering {
    right.unwrap_or_default().cmp(&left.unwrap_or_default())
}

fn desc_f64(left: Option<f64>, right: Option<f64>) -> Ordering {
    right
        .unwrap_or_default()
        .partial_cmp(&left.unwrap_or_default())
        .unwrap_or(Ordering::Equal)
}

fn parse_metric(value: Option<&str>) -> Option<f64> {
    value.and_then(|v| v.trim().parse::<f64>().ok())
}

fn parse_year(value: &str) -> Option<i32> {
    value.trim().parse::<i32>().ok()
}

fn unique_terms(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    tokenize(text)
        .into_iter()
        .filter(|term| seen.insert(term.clone()))
        .collect()
}

fn token_set(text: &str) -> HashSet<String> {
    tokenize(text).into_iter().collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(str::trim)
        .filter(|s| s.len() > 1)
        .map(str::to_string)
        .collect()
}

fn count_hits(terms: &[String], haystack: &HashSet<String>) -> usize {
    terms.iter().filter(|term| haystack.contains(*term)).count()
}

fn phrase_boost(
    keyword: &str,
    content_help: Option<&str>,
    title: &str,
    abstract_text: &str,
) -> f64 {
    let mut boost = 0.0;
    for phrase in [Some(keyword), content_help].into_iter().flatten() {
        let normalized = phrase.trim().to_lowercase();
        if normalized.len() < 4 {
            continue;
        }
        if title.contains(&normalized) {
            boost += 10.0;
        } else if abstract_text.contains(&normalized) {
            boost += 5.0;
        }
    }
    boost
}

fn coverage_boost(
    title_hits: usize,
    abstract_hits: usize,
    keyword_terms: usize,
    focus_hits: usize,
    focus_terms: usize,
) -> f64 {
    let mut boost = 0.0;
    if keyword_terms > 0 && title_hits * 5 >= keyword_terms * 4 {
        boost += 12.0;
    }
    if keyword_terms > 0 && (title_hits + abstract_hits) >= keyword_terms {
        boost += 8.0;
    }
    if focus_terms > 0 && focus_hits >= focus_terms {
        boost += 6.0;
    }
    boost
}
