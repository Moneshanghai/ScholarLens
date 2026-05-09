//! arXiv API client.
//!
//! Endpoint: `https://export.arxiv.org/api/query`
//! Response: Atom XML feed (`<feed><entry>...</entry></feed>`)
//! Pagination: `start` + `max_results`
//!
//! Notes:
//! - This module performs keyword search and parses essential metadata.
//! - To be polite to arXiv infrastructure, requests are throttled by default.

use crate::error::{GscholarError, Result};
use crate::sources::{SourcePaper, SourceProvider};
use async_trait::async_trait;
use regex::Regex;
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{info, warn};

const ARXIV_API_URLS: [&str; 2] = [
    "https://arxiv.org/api/query",
    "https://export.arxiv.org/api/query",
];
const ARXIV_HTML_SEARCH_URL: &str = "https://arxiv.org/search/";
const ARXIV_MAX_PER_REQUEST: usize = 2000;
const ARXIV_RELAXED_SEARCH_THRESHOLD: usize = 20;
const ARXIV_USER_AGENT: &str =
    "ScholarLens/0.1 (https://github.com/Moneshanghai/ScholarLens; arxiv source)";
static ARXIV_SERIAL_REQUEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// arXiv search options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivQueryOptions {
    /// Total max results to collect across pages (clamped to 200 in pipeline layer).
    pub max_results: usize,
    /// Per-request page size (`max_results` query param in arXiv API).
    pub page_size: usize,
    /// Sort field (`relevance`, `submittedDate`, `lastUpdatedDate`).
    pub sort_by: String,
    /// Sort order (`ascending`, `descending`).
    pub sort_order: String,
    /// HTTP timeout seconds.
    pub timeout_secs: u64,
    /// Inter-request delay to keep polite RPS.
    pub request_delay_ms: u64,
}

impl Default for ArxivQueryOptions {
    fn default() -> Self {
        Self {
            max_results: 100,
            page_size: 100,
            sort_by: "relevance".to_string(),
            sort_order: "descending".to_string(),
            timeout_secs: 30,
            request_delay_ms: 3000,
        }
    }
}

/// arXiv normalized result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArxivResult {
    pub title: String,
    pub authors: String,
    pub year: String,
    pub doi: String,
    pub url: String,
    pub pdf_url: String,
    pub abstract_text: String,
}

/// arXiv provider implementing the unified source trait.
pub struct ArxivProvider {
    clients: Vec<ArxivHttpClient>,
}

struct ArxivHttpClient {
    route: ArxivNetworkRoute,
    client: Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ArxivNetworkRoute {
    Direct,
    Proxy(String),
}

impl ArxivNetworkRoute {
    fn label(&self) -> &str {
        match self {
            Self::Direct => "direct",
            Self::Proxy(proxy_url) => proxy_url.as_str(),
        }
    }
}

impl ArxivProvider {
    pub fn new(timeout_secs: u64) -> Result<Self> {
        let mut clients = Vec::new();
        let routes = arxiv_network_routes();

        for route in routes {
            match build_arxiv_client(timeout_secs, &route) {
                Ok(client) => {
                    info!(route = route.label(), "arXiv network route prepared");
                    clients.push(ArxivHttpClient { route, client });
                }
                Err(error) => {
                    warn!(
                        route = route.label(),
                        error = %error,
                        "Skipping invalid arXiv network route"
                    );
                }
            }
        }

        if clients.is_empty() {
            return Err(GscholarError::Config(
                "No usable arXiv network routes could be created".to_string(),
            ));
        }

        Ok(Self { clients })
    }
}

#[async_trait]
impl SourceProvider<ArxivQueryOptions> for ArxivProvider {
    fn source_name(&self) -> &'static str {
        "arxiv"
    }

    async fn search(&self, query: &str, options: &ArxivQueryOptions) -> Result<Vec<SourcePaper>> {
        let papers = search_with_route_clients(&self.clients, query, options).await?;
        Ok(papers
            .into_iter()
            .map(|p| SourcePaper {
                title: p.title,
                authors: p.authors,
                year: p.year,
                venue: "arXiv".to_string(),
                doi: p.doi,
                url: p.url,
                pdf_url: p.pdf_url,
                snippet: String::new(),
                abstract_text: p.abstract_text,
            })
            .collect())
    }
}

/// Search arXiv and return normalized results.
pub async fn search_papers(query: &str, options: &ArxivQueryOptions) -> Result<Vec<ArxivResult>> {
    let provider = ArxivProvider::new(options.timeout_secs)?;
    search_with_route_clients(&provider.clients, query, options).await
}

async fn search_with_route_clients(
    clients: &[ArxivHttpClient],
    query: &str,
    options: &ArxivQueryOptions,
) -> Result<Vec<ArxivResult>> {
    let mut last_error = None;

    for candidate in clients {
        info!(
            route = candidate.route.label(),
            query = query,
            "Trying arXiv network route"
        );
        match search_with_client(&candidate.client, query, options).await {
            Ok(papers) => {
                info!(
                    route = candidate.route.label(),
                    papers = papers.len(),
                    "arXiv network route succeeded"
                );
                return Ok(papers);
            }
            Err(error) => {
                warn!(
                    route = candidate.route.label(),
                    error = %error,
                    "arXiv network route failed"
                );
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| GscholarError::Api {
        code: 0,
        message: "No arXiv network route was available".to_string(),
    }))
}

fn build_arxiv_client(timeout_secs: u64, route: &ArxivNetworkRoute) -> Result<Client> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(ARXIV_USER_AGENT);

    if let ArxivNetworkRoute::Proxy(proxy_url) = route {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }

    Ok(builder.build()?)
}

fn arxiv_network_routes() -> Vec<ArxivNetworkRoute> {
    arxiv_network_routes_from_configured(arxiv_configured_proxy_urls())
}

fn arxiv_network_routes_from_configured(
    configured_proxy_urls: Vec<String>,
) -> Vec<ArxivNetworkRoute> {
    let mut routes = Vec::new();
    let mut seen = std::collections::HashSet::new();

    push_arxiv_route(&mut routes, &mut seen, ArxivNetworkRoute::Direct);
    for proxy_url in configured_proxy_urls {
        push_arxiv_route(&mut routes, &mut seen, ArxivNetworkRoute::Proxy(proxy_url));
    }
    for proxy_url in default_local_arxiv_proxy_urls() {
        push_arxiv_route(
            &mut routes,
            &mut seen,
            ArxivNetworkRoute::Proxy(proxy_url.to_string()),
        );
    }

    routes
}

fn push_arxiv_route(
    routes: &mut Vec<ArxivNetworkRoute>,
    seen: &mut std::collections::HashSet<ArxivNetworkRoute>,
    route: ArxivNetworkRoute,
) {
    if seen.insert(route.clone()) {
        routes.push(route);
    }
}

fn arxiv_configured_proxy_urls() -> Vec<String> {
    ["ARXIV_PROXY", "ARXIV_HTTPS_PROXY"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn default_local_arxiv_proxy_urls() -> [&'static str; 4] {
    [
        "socks5h://127.0.0.1:10808",
        "http://127.0.0.1:10808",
        "http://127.0.0.1:7890",
        "socks5h://127.0.0.1:7890",
    ]
}

async fn search_with_client(
    client: &Client,
    query: &str,
    options: &ArxivQueryOptions,
) -> Result<Vec<ArxivResult>> {
    match search_api_with_client(client, query, options).await {
        Ok(papers) => Ok(papers),
        Err(api_error) => {
            warn!(
                error = %api_error,
                "arXiv API search failed, trying HTML search fallback"
            );
            match search_html_fallback(client, query, options).await {
                Ok(papers) => Ok(papers),
                Err(fallback_error) => {
                    warn!(
                        api_error = %api_error,
                        fallback_error = %fallback_error,
                        "arXiv HTML search fallback failed"
                    );
                    Err(api_error)
                }
            }
        }
    }
}

async fn search_api_with_client(
    client: &Client,
    query: &str,
    options: &ArxivQueryOptions,
) -> Result<Vec<ArxivResult>> {
    search_api_with_endpoints(client, query, options, &ARXIV_API_URLS).await
}

async fn search_api_with_endpoints(
    client: &Client,
    query: &str,
    options: &ArxivQueryOptions,
    endpoints: &[&str],
) -> Result<Vec<ArxivResult>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let search_query = build_search_query(query);
    let max_results = options.max_results.max(1);
    let mut out = search_api_query(
        client,
        endpoints,
        query,
        &search_query,
        options,
        max_results,
    )
    .await?;

    if out.len() < max_results.min(ARXIV_RELAXED_SEARCH_THRESHOLD) {
        for relaxed_query in build_relaxed_search_queries(query) {
            if out.len() >= max_results {
                break;
            }
            let remaining = max_results - out.len();
            match search_api_query(client, endpoints, query, &relaxed_query, options, remaining)
                .await
            {
                Ok(extra) => {
                    let fetched = extra.len();
                    let added = append_arxiv_results_dedup(&mut out, extra);
                    info!(
                        query = query,
                        relaxed_query = %relaxed_query,
                        fetched = fetched,
                        added = added,
                        total = out.len(),
                        "arXiv relaxed query completed"
                    );
                }
                Err(error) => {
                    warn!(
                        query = query,
                        relaxed_query = %relaxed_query,
                        error = %error,
                        "arXiv relaxed query failed"
                    );
                }
            }
        }
    }

    out.truncate(max_results);
    info!(total = out.len(), "arXiv search completed");
    Ok(out)
}

async fn search_api_query(
    client: &Client,
    endpoints: &[&str],
    query: &str,
    search_query: &str,
    options: &ArxivQueryOptions,
    max_results: usize,
) -> Result<Vec<ArxivResult>> {
    let page_size = options.page_size.clamp(1, ARXIV_MAX_PER_REQUEST);

    info!(
        query = query,
        search_query = %search_query,
        max_results = max_results,
        page_size = page_size,
        sort_by = %options.sort_by,
        sort_order = %options.sort_order,
        "Starting arXiv search"
    );

    let mut out = Vec::new();
    let mut start = 0usize;

    while out.len() < max_results {
        let remaining = max_results - out.len();
        let batch_size = remaining.min(page_size);

        let params = [
            ("search_query", search_query.to_string()),
            ("start", start.to_string()),
            ("max_results", batch_size.to_string()),
            ("sortBy", options.sort_by.clone()),
            ("sortOrder", options.sort_order.clone()),
        ];

        let mut resp_text = None;
        let mut last_error = None;
        let mut rate_limited_after_secs = None;

        for endpoint in endpoints {
            let mut server_error_retries = 0u32;

            let page_resp = loop {
                match send_arxiv_request(client, endpoint, &params, options.request_delay_ms).await
                {
                    Ok(resp) => {
                        let status = resp.status();
                        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                            let headers = resp.headers().clone();
                            let error_text = resp.text().await.unwrap_or_default();
                            let retry_after = headers
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|value| value.to_str().ok())
                                .and_then(|value| value.trim().parse::<u64>().ok())
                                .unwrap_or(600);
                            last_error = Some("arXiv API rate limited".to_string());
                            rate_limited_after_secs = Some(retry_after);
                            warn!(
                                source = "arxiv",
                                endpoint = endpoint,
                                status = 429,
                                retry_after_secs = retry_after,
                                error = %error_text,
                                "arXiv API rate limited (429), switching to HTML fallback"
                            );
                            break None;
                        }
                        if status.is_server_error() {
                            server_error_retries += 1;
                            if server_error_retries > 5 {
                                warn!(source = "arxiv", endpoint = endpoint, status = %status,
                                    "arXiv server error after 5 retries, trying next endpoint");
                                break None;
                            }
                            warn!(
                                source = "arxiv", endpoint = endpoint, status = %status,
                                attempt = server_error_retries,
                                "arXiv server error, retrying in 2s"
                            );
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                        match resp.text().await {
                            Ok(text) => break Some(text),
                            Err(err) => {
                                last_error = Some(err.to_string());
                                warn!(
                                    endpoint = endpoint,
                                    start = start,
                                    batch_size = batch_size,
                                    error = %err,
                                    "arXiv response body read failed, trying next endpoint"
                                );
                                break None;
                            }
                        }
                    }
                    Err(err) => {
                        last_error = Some(err.to_string());
                        warn!(
                            endpoint = endpoint,
                            start = start,
                            batch_size = batch_size,
                            error = %err,
                            "arXiv request failed, trying next endpoint"
                        );
                        break None;
                    }
                }
            };

            if let Some(text) = page_resp {
                resp_text = Some(text);
                break;
            }
        }

        if resp_text.is_none() {
            if let Some(retry_after) = rate_limited_after_secs {
                return Err(GscholarError::RateLimited(retry_after));
            }
        }

        let resp_text = resp_text.ok_or_else(|| {
            crate::error::GscholarError::Parse(format!(
                "Failed to fetch arXiv response from all endpoints: {}",
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ))
        })?;

        let page = parse_atom_entries(&resp_text);
        let fetched = page.len();
        info!(
            start = start,
            requested = batch_size,
            fetched = fetched,
            accumulated = out.len() + fetched,
            "arXiv page fetched"
        );
        out.extend(page);

        if fetched == 0 || fetched < batch_size {
            break;
        }

        start += batch_size;
    }

    out.truncate(max_results);
    Ok(out)
}

async fn search_html_fallback(
    client: &Client,
    query: &str,
    options: &ArxivQueryOptions,
) -> Result<Vec<ArxivResult>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let limit = options.max_results.clamp(1, 200);
    let request_size = html_search_size(limit);
    let mut params = vec![
        ("query", trimmed.to_string()),
        ("searchtype", "all".to_string()),
        ("abstracts", "show".to_string()),
        ("size", request_size.to_string()),
    ];
    let order = html_search_order(options);
    if !order.is_empty() {
        params.push(("order", order));
    }

    info!(
        query = trimmed,
        size = request_size,
        limit = limit,
        "Starting arXiv HTML search fallback"
    );

    let resp = send_arxiv_request(
        client,
        ARXIV_HTML_SEARCH_URL,
        &params,
        options.request_delay_ms,
    )
    .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(GscholarError::RateLimited(600));
    }
    if !status.is_success() {
        return Err(GscholarError::Api {
            code: status.as_u16() as i32,
            message: "arXiv HTML search returned non-success status".to_string(),
        });
    }

    let html = resp.text().await?;
    let mut papers = parse_search_html_results(&html);
    papers.truncate(limit);
    info!(total = papers.len(), "arXiv HTML search fallback completed");
    Ok(papers)
}

fn html_search_size(limit: usize) -> usize {
    match limit {
        0..=25 => 25,
        26..=50 => 50,
        51..=100 => 100,
        _ => 200,
    }
}

fn html_search_order(options: &ArxivQueryOptions) -> String {
    let newest_first = options.sort_order.eq_ignore_ascii_case("descending");
    match options.sort_by.as_str() {
        "submittedDate" => {
            if newest_first {
                "-submitted_date".to_string()
            } else {
                "submitted_date".to_string()
            }
        }
        "lastUpdatedDate" => {
            if newest_first {
                "-announced_date_first".to_string()
            } else {
                "announced_date_first".to_string()
            }
        }
        _ => String::new(),
    }
}

/// Convert user-entered plain keywords into explicit arXiv API syntax.
///
/// arXiv treats whitespace in a raw query as OR. For a normal search box, users
/// expect multi-word keywords to narrow results, so plain tokens are joined with
/// AND and prefixed with `all:`. Advanced arXiv syntax is preserved as-is.
pub fn build_search_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() || looks_like_advanced_arxiv_query(trimmed) {
        return trimmed.to_string();
    }

    let terms = plain_query_terms(trimmed);
    if terms.is_empty() {
        return trimmed.to_string();
    }

    build_terms_query(&terms, " AND ")
}

fn build_relaxed_search_queries(query: &str) -> Vec<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() || looks_like_advanced_arxiv_query(trimmed) {
        return Vec::new();
    }

    let terms = plain_query_terms(trimmed);
    if terms.len() < 2 {
        return Vec::new();
    }

    let strict_query = build_terms_query(&terms, " AND ");
    let mut queries = Vec::new();

    for keep_count in (2..terms.len()).rev() {
        queries.push(build_terms_query(&terms[..keep_count], " AND "));
    }

    let phrase_chunks = terms
        .chunks(2)
        .map(|chunk| chunk.join(" "))
        .collect::<Vec<_>>();
    if phrase_chunks.len() > 1 {
        queries.push(build_terms_query(&phrase_chunks, " OR "));
    }

    queries.push(build_terms_query(&terms, " OR "));
    queries.push(trimmed.to_string());
    unique_queries_excluding(queries, &strict_query)
}

fn unique_queries_excluding(queries: Vec<String>, excluded: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    queries
        .into_iter()
        .filter(|query| !query.is_empty() && query != excluded)
        .filter(|query| seen.insert(query.clone()))
        .collect()
}

fn build_terms_query(terms: &[String], separator: &str) -> String {
    terms
        .iter()
        .map(|term| format_arxiv_all_term(term))
        .collect::<Vec<_>>()
        .join(separator)
}

fn format_arxiv_all_term(term: &str) -> String {
    if term.chars().any(char::is_whitespace) {
        format!("all:\"{}\"", term.replace('"', "\\\""))
    } else {
        format!("all:{term}")
    }
}

fn looks_like_advanced_arxiv_query(query: &str) -> bool {
    let field_re = Regex::new(r"(?i)\b(all|ti|abs|au|cat|id|doi|jr|co|rn):")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let boolean_re = Regex::new(r"(?i)(\bAND\b|\bOR\b|\bANDNOT\b|\(|\))")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    field_re.is_match(query) || boolean_re.is_match(query)
}

fn plain_query_terms(query: &str) -> Vec<String> {
    let term_re = Regex::new(r#""([^"]+)"|'([^']+)'|[^\s,;，；]+"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let mut terms = Vec::new();
    for cap in term_re.captures_iter(query) {
        let raw = cap
            .get(1)
            .or_else(|| cap.get(2))
            .or_else(|| cap.get(0))
            .map(|m| m.as_str())
            .unwrap_or_default();
        let term = raw
            .trim()
            .trim_matches(|ch: char| matches!(ch, ',' | ';' | '，' | '；'))
            .trim();
        if !term.is_empty() {
            terms.push(term.to_string());
        }
    }
    terms
}

async fn send_arxiv_request(
    client: &Client,
    endpoint: &str,
    params: &[(&str, String)],
    request_delay_ms: u64,
) -> reqwest::Result<reqwest::Response> {
    let lock = ARXIV_SERIAL_REQUEST_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = lock.lock().await;
    let response = client.get(endpoint).query(params).send().await;
    if request_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(request_delay_ms)).await;
    }
    response
}

fn parse_atom_entries(xml: &str) -> Vec<ArxivResult> {
    let entry_re = Regex::new(r"(?s)<entry>(.*?)</entry>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let title_re = Regex::new(r"(?s)<title>(.*?)</title>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let summary_re = Regex::new(r"(?s)<summary>(.*?)</summary>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let published_re = Regex::new(r"(?s)<published>(.*?)</published>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let id_re = Regex::new(r"(?s)<id>(.*?)</id>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let doi_re = Regex::new(r"(?s)<arxiv:doi[^>]*>(.*?)</arxiv:doi>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let author_re = Regex::new(r"(?s)<author>\s*<name>(.*?)</name>\s*</author>")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let link_re = Regex::new(r#"(?s)<link\b([^>]*)>"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));

    entry_re
        .captures_iter(xml)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str()))
        .map(|entry| {
            let title = capture_first(&title_re, entry).unwrap_or_default();
            let abstract_text = capture_first(&summary_re, entry).unwrap_or_default();
            let published = capture_first(&published_re, entry).unwrap_or_default();
            let id_url = capture_first(&id_re, entry).unwrap_or_default();
            let doi = capture_first(&doi_re, entry).unwrap_or_default();
            let (alt_url, pdf_url) = extract_link_urls(entry, &link_re);
            let authors = author_re
                .captures_iter(entry)
                .filter_map(|c| {
                    c.get(1)
                        .map(|m| decode_xml_entities(strip_tags(m.as_str()).trim()))
                })
                .collect::<Vec<_>>()
                .join(", ");

            let year = published.get(0..4).unwrap_or_default().to_string();
            let clean_title =
                decode_xml_entities(strip_tags(title.as_str()).trim()).replace('\n', " ");
            let clean_abstract =
                decode_xml_entities(strip_tags(abstract_text.as_str()).trim()).replace('\n', " ");

            ArxivResult {
                title: clean_title,
                authors,
                year,
                doi: decode_xml_entities(doi.trim()),
                url: if !alt_url.is_empty() { alt_url } else { id_url },
                pdf_url,
                abstract_text: clean_abstract,
            }
        })
        .filter(|paper| !paper.title.is_empty())
        .collect()
}

fn parse_search_html_results(html: &str) -> Vec<ArxivResult> {
    let result_re = Regex::new(r#"(?s)<li class="arxiv-result">(.*?)</li>"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let abs_re =
        Regex::new(r#"(?s)<p class="list-title[^"]*"[^>]*>\s*<a href="([^"]+)">arXiv:([^<]+)</a>"#)
            .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let pdf_re = Regex::new(r#"(?s)<a href="([^"]+)">pdf</a>"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let title_re = Regex::new(r#"(?s)<p class="title is-5 mathjax">\s*(.*?)\s*</p>"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let authors_re = Regex::new(r#"(?s)<p class="authors">\s*(.*?)\s*</p>"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let abstract_re =
        Regex::new(r#"(?s)<span class="abstract-full[^"]*"[^>]*>\s*(.*?)\s*<a class="is-size-7""#)
            .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let date_re = Regex::new(r#"(?s)<p class="is-size-7">(.*?)</p>"#)
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));
    let year_re = Regex::new(r"\b((?:19|20)\d{2})\b")
        .unwrap_or_else(|_| Regex::new("$^").expect("regex fallback"));

    result_re
        .captures_iter(html)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str()))
        .filter_map(|entry| {
            let abs_caps = abs_re.captures(entry)?;
            let url = abs_caps
                .get(1)
                .map(|m| absolute_arxiv_url(m.as_str()))
                .unwrap_or_default();
            let arxiv_id = abs_caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            let pdf_url = capture_first_raw(&pdf_re, entry)
                .map(|url| absolute_arxiv_url(&url))
                .unwrap_or_default();
            let title = capture_first_raw(&title_re, entry)
                .map(|text| clean_html_text(&text))
                .unwrap_or_default();
            if title.is_empty() {
                return None;
            }
            let authors = capture_first_raw(&authors_re, entry)
                .map(|text| {
                    clean_html_text(&text)
                        .trim_start_matches("Authors:")
                        .trim()
                        .to_string()
                })
                .unwrap_or_default();
            let abstract_text = capture_first_raw(&abstract_re, entry)
                .map(|text| clean_html_text(&text))
                .unwrap_or_default();
            let year = capture_first_raw(&date_re, entry)
                .and_then(|text| capture_first_raw(&year_re, &text))
                .unwrap_or_else(|| year_from_arxiv_id(arxiv_id));

            Some(ArxivResult {
                title,
                authors,
                year,
                doi: String::new(),
                url,
                pdf_url,
                abstract_text,
            })
        })
        .collect()
}

fn capture_first_raw(re: &Regex, text: &str) -> Option<String> {
    re.captures(text)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
}

fn absolute_arxiv_url(value: &str) -> String {
    let decoded = decode_html_entities(value.trim());
    if decoded.starts_with("http://") || decoded.starts_with("https://") {
        decoded
    } else {
        format!("https://arxiv.org{decoded}")
    }
}

fn clean_html_text(text: &str) -> String {
    decode_html_entities(&strip_tags(text))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_html_entities(input: &str) -> String {
    decode_xml_entities(input)
        .replace("&nbsp;", " ")
        .replace("&hellip;", "...")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
}

fn year_from_arxiv_id(arxiv_id: &str) -> String {
    let Some(prefix) = arxiv_id.get(0..2) else {
        return String::new();
    };
    let Ok(year) = prefix.parse::<u32>() else {
        return String::new();
    };
    if year >= 91 {
        format!("19{year:02}")
    } else {
        format!("20{year:02}")
    }
}

fn append_arxiv_results_dedup(base: &mut Vec<ArxivResult>, additional: Vec<ArxivResult>) -> usize {
    let mut url_set = base
        .iter()
        .filter_map(|paper| normalize_arxiv_url_for_dedup(&paper.url))
        .collect::<std::collections::HashSet<_>>();
    let mut title_set = base
        .iter()
        .filter_map(|paper| normalize_arxiv_title_for_dedup(&paper.title))
        .collect::<std::collections::HashSet<_>>();
    let mut added = 0usize;

    for paper in additional {
        let url_key = normalize_arxiv_url_for_dedup(&paper.url);
        let title_key = normalize_arxiv_title_for_dedup(&paper.title);
        let duplicated = url_key
            .as_ref()
            .map(|url| url_set.contains(url))
            .unwrap_or(false)
            || title_key
                .as_ref()
                .map(|title| title_set.contains(title))
                .unwrap_or(false);
        if duplicated {
            continue;
        }

        if let Some(url) = url_key {
            url_set.insert(url);
        }
        if let Some(title) = title_key {
            title_set.insert(title);
        }
        base.push(paper);
        added += 1;
    }

    added
}

fn normalize_arxiv_url_for_dedup(url: &str) -> Option<String> {
    let normalized = url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".pdf")
        .to_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalize_arxiv_title_for_dedup(title: &str) -> Option<String> {
    let normalized = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_link_urls(entry: &str, link_re: &Regex) -> (String, String) {
    let mut alt_url = String::new();
    let mut pdf_url = String::new();

    for cap in link_re.captures_iter(entry) {
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        let Some(href) = attr_value(attrs, "href") else {
            continue;
        };
        let rel = attr_value(attrs, "rel").unwrap_or_default().to_lowercase();
        let title = attr_value(attrs, "title")
            .unwrap_or_default()
            .to_lowercase();
        let media_type = attr_value(attrs, "type").unwrap_or_default().to_lowercase();

        if alt_url.is_empty() && rel == "alternate" {
            alt_url = href.clone();
        }
        if pdf_url.is_empty()
            && (title == "pdf" || media_type == "application/pdf" || href.contains("/pdf/"))
        {
            pdf_url = href;
        }
    }

    (alt_url, pdf_url)
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let escaped = regex::escape(name);
    let double_re = Regex::new(&format!(r#"{escaped}\s*=\s*"([^"]*)""#)).ok()?;
    if let Some(value) = double_re
        .captures(attrs)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
    {
        return Some(decode_xml_entities(&value));
    }

    let single_re = Regex::new(&format!(r#"{escaped}\s*=\s*'([^']*)'"#)).ok()?;
    single_re
        .captures(attrs)
        .and_then(|cap| cap.get(1).map(|m| decode_xml_entities(m.as_str())))
}

fn capture_first(re: &Regex, text: &str) -> Option<String> {
    re.captures(text).and_then(|cap| {
        cap.get(1)
            .map(|m| decode_xml_entities(strip_tags(m.as_str()).trim()))
    })
}

fn strip_tags(text: &str) -> String {
    let tag_re = Regex::new(r"(?s)<[^>]+>").unwrap_or_else(|_| {
        warn!("Failed to compile XML tag regex, returning raw text");
        Regex::new("$^").expect("regex fallback")
    });
    tag_re.replace_all(text, "").to_string()
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#10;", " ")
        .replace("&#xA;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Query, http::header::CONTENT_TYPE, response::IntoResponse, routing::get};
    use std::collections::HashMap;

    #[test]
    fn test_parse_atom_entries() {
        let xml = r#"
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry>
    <id>http://arxiv.org/abs/1234.5678v1</id>
    <published>2024-01-10T00:00:00Z</published>
    <title>Test &amp; Title</title>
    <summary>Sample abstract text.</summary>
    <author><name>Alice A</name></author>
    <author><name>Bob B</name></author>
    <arxiv:doi>10.1000/test</arxiv:doi>
    <link rel="alternate" href="http://arxiv.org/abs/1234.5678v1" />
    <link title="pdf" href="http://arxiv.org/pdf/1234.5678v1" />
  </entry>
</feed>
        "#;

        let parsed = parse_atom_entries(xml);
        assert_eq!(parsed.len(), 1);
        let p = &parsed[0];
        assert_eq!(p.year, "2024");
        assert_eq!(p.doi, "10.1000/test");
        assert!(p.authors.contains("Alice A"));
        assert!(p.pdf_url.contains("/pdf/"));
        assert!(p.title.contains("Test & Title"));
    }

    #[test]
    fn test_decode_xml_entities() {
        let s = decode_xml_entities("A &amp; B &lt; C");
        assert_eq!(s, "A & B < C");
    }

    #[test]
    fn test_build_search_query_plain_words_use_and_semantics() {
        let query = build_search_query("machine learning, materials");
        assert_eq!(query, "all:machine AND all:learning AND all:materials");
    }

    #[test]
    fn test_build_search_query_preserves_advanced_arxiv_query() {
        let query = build_search_query(r#"ti:"machine learning" AND cat:cs.LG"#);
        assert_eq!(query, r#"ti:"machine learning" AND cat:cs.LG"#);
    }

    #[test]
    fn test_relaxed_search_queries_for_sparse_plain_query() {
        let queries = build_relaxed_search_queries("ceramic glaze property prediction");

        assert!(queries.contains(&"all:ceramic AND all:glaze AND all:property".to_string()));
        assert!(queries.contains(&"all:ceramic AND all:glaze".to_string()));
        assert!(
            queries.contains(&r#"all:"ceramic glaze" OR all:"property prediction""#.to_string())
        );
        assert!(queries.contains(&"ceramic glaze property prediction".to_string()));
        assert!(!queries.contains(&build_search_query("ceramic glaze property prediction")));
    }

    #[test]
    fn test_append_arxiv_results_dedup() {
        let mut base = vec![ArxivResult {
            title: "A Sparse arXiv Result".to_string(),
            url: "https://arxiv.org/abs/2605.06641".to_string(),
            ..Default::default()
        }];
        let added = append_arxiv_results_dedup(
            &mut base,
            vec![
                ArxivResult {
                    title: "A Sparse arXiv Result".to_string(),
                    url: "https://arxiv.org/abs/2605.06641".to_string(),
                    ..Default::default()
                },
                ArxivResult {
                    title: "A Relaxed arXiv Result".to_string(),
                    url: "https://arxiv.org/abs/2605.06642".to_string(),
                    ..Default::default()
                },
            ],
        );

        assert_eq!(added, 1);
        assert_eq!(base.len(), 2);
    }

    #[test]
    fn test_arxiv_network_routes_try_direct_then_local_proxy_ports() {
        let routes = arxiv_network_routes_from_configured(vec![
            "http://127.0.0.1:10808".to_string(),
            "http://custom-proxy.local:8080".to_string(),
        ]);

        assert_eq!(
            routes,
            vec![
                ArxivNetworkRoute::Direct,
                ArxivNetworkRoute::Proxy("http://127.0.0.1:10808".to_string()),
                ArxivNetworkRoute::Proxy("http://custom-proxy.local:8080".to_string()),
                ArxivNetworkRoute::Proxy("socks5h://127.0.0.1:10808".to_string()),
                ArxivNetworkRoute::Proxy("http://127.0.0.1:7890".to_string()),
                ArxivNetworkRoute::Proxy("socks5h://127.0.0.1:7890".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn test_sparse_query_is_expanded_with_relaxed_api_results() {
        let endpoint = spawn_arxiv_api_stub().await;
        let endpoint_ref = endpoint.as_str();
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("test client");
        let options = ArxivQueryOptions {
            max_results: 10,
            page_size: 10,
            sort_by: "relevance".to_string(),
            sort_order: "descending".to_string(),
            timeout_secs: 10,
            request_delay_ms: 0,
        };

        let results = search_api_with_endpoints(
            &client,
            "ceramic glaze property prediction",
            &options,
            &[endpoint_ref],
        )
        .await
        .expect("stubbed arxiv search");

        assert_eq!(results.len(), 6);
        assert_eq!(results[0].title, "Strict Ceramic Glaze Property Prediction");
        assert!(results
            .iter()
            .any(|paper| paper.title == "Relaxed Ceramic Glaze Result 5"));
    }

    async fn spawn_arxiv_api_stub() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind arxiv stub");
        let addr = listener.local_addr().expect("stub local addr");
        let app = axum::Router::new().route("/api/query", get(arxiv_api_stub_handler));
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve arxiv stub");
        });
        format!("http://{addr}/api/query")
    }

    async fn arxiv_api_stub_handler(
        Query(params): Query<HashMap<String, String>>,
    ) -> impl IntoResponse {
        let search_query = params.get("search_query").cloned().unwrap_or_default();
        let max_results = params
            .get("max_results")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(10);

        let entries =
            if search_query == "all:ceramic AND all:glaze AND all:property AND all:prediction" {
                vec![stub_entry(
                    "2605.06641",
                    "Strict Ceramic Glaze Property Prediction",
                )]
            } else if search_query == "all:ceramic OR all:glaze OR all:property OR all:prediction"
                || search_query == "ceramic glaze property prediction"
            {
                (1..=5)
                    .map(|idx| {
                        stub_entry(
                            &format!("2605.0764{idx}"),
                            &format!("Relaxed Ceramic Glaze Result {idx}"),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
        let limited_entries = entries.into_iter().take(max_results).collect::<Vec<_>>();
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/" xmlns:arxiv="http://arxiv.org/schemas/atom" xmlns="http://www.w3.org/2005/Atom">
{}
</feed>"#,
            limited_entries.join("\n")
        );

        ([(CONTENT_TYPE, "application/atom+xml")], body)
    }

    fn stub_entry(id: &str, title: &str) -> String {
        format!(
            r#"<entry>
  <id>https://arxiv.org/abs/{id}</id>
  <published>2026-05-09T00:00:00Z</published>
  <title>{title}</title>
  <summary>Stub abstract.</summary>
  <author><name>Alice Example</name></author>
  <link href="https://arxiv.org/abs/{id}" rel="alternate" type="text/html"/>
  <link href="https://arxiv.org/pdf/{id}" rel="related" type="application/pdf" title="pdf"/>
</entry>"#
        )
    }

    #[test]
    fn test_parse_atom_entries_link_attributes_in_arxiv_order() {
        let xml = r#"
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry>
    <id>https://arxiv.org/abs/2501.02842v1</id>
    <published>2025-01-06T00:00:00Z</published>
    <title>Foundations of GenIR</title>
    <summary>Sample abstract text.</summary>
    <author><name>Alice A</name></author>
    <link href="https://arxiv.org/abs/2501.02842v1" rel="alternate" type="text/html"/>
    <link href="https://arxiv.org/pdf/2501.02842v1" rel="related" type="application/pdf" title="pdf"/>
  </entry>
</feed>
        "#;

        let parsed = parse_atom_entries(xml);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].url, "https://arxiv.org/abs/2501.02842v1");
        assert_eq!(parsed[0].pdf_url, "https://arxiv.org/pdf/2501.02842v1");
    }

    #[test]
    fn test_parse_search_html_results() {
        let html = r#"
<ol>
  <li class="arxiv-result">
    <p class="list-title is-inline-block"><a href="https://arxiv.org/abs/2605.06641">arXiv:2605.06641</a>
      <span>[<a href="https://arxiv.org/pdf/2605.06641">pdf</a>]</span>
    </p>
    <p class="title is-5 mathjax">
      GlazyBench: A Benchmark for Ceramic Glaze Property Prediction
    </p>
    <p class="authors">
      <span>Authors:</span>
      <a href="/search/?searchtype=author&amp;query=Zhai%2C+Z">Ziyu Zhai</a>,
      <a href="/search/?searchtype=author&amp;query=Li%2C+S">Siyou Li</a>
    </p>
    <p class="abstract mathjax">
      <span class="abstract-full has-text-grey-dark mathjax" id="2605.06641v1-abstract-full" style="display: none;">
        Developing ceramic glazes with <span class="search-hit">machine</span> learning.
        <a class="is-size-7">Less</a>
      </span>
    </p>
    <p class="is-size-7"><span>Submitted</span> 7 May, 2026;</p>
  </li>
</ol>
        "#;

        let parsed = parse_search_html_results(html);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].title,
            "GlazyBench: A Benchmark for Ceramic Glaze Property Prediction"
        );
        assert_eq!(parsed[0].authors, "Ziyu Zhai, Siyou Li");
        assert_eq!(parsed[0].year, "2026");
        assert_eq!(parsed[0].url, "https://arxiv.org/abs/2605.06641");
        assert_eq!(parsed[0].pdf_url, "https://arxiv.org/pdf/2605.06641");
        assert!(parsed[0].abstract_text.contains("machine learning"));
    }
}
