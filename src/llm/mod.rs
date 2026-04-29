//! LLM-based relevance filtering module.
//!
//! Uses LLM providers to judge if papers are relevant to search keywords.
//! Supports provider abstraction, config-driven initialization, and fallback scheduling.

pub mod keyword_expansion;
pub mod keyword_translation;
pub mod openai_compatible;
mod provider_core;

use crate::error::{GscholarError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

type ProviderBuilder = fn(&crate::server::config::ProviderConfig) -> Result<Arc<dyn LlmProvider>>;
pub(crate) type ProviderRegistry = HashMap<String, ProviderBuilder>;

mod providers {
    pub(crate) type ProviderRegistry = super::ProviderRegistry;
    include!(concat!(env!("OUT_DIR"), "/llm_providers_gen.rs"));
}

// Re-export built-in provider implementations
pub use openai_compatible::{InterfaceType, OpenAiCompatibleProvider};
pub use providers::aiping::AiPingProvider;
pub use providers::bigmodel::BigModelProvider;
pub use providers::siliconflow::SiliconFlowProvider;

/// Maximum concurrent requests
const MAX_CONCURRENT: usize = 6;

/// Paper info for relevance check
#[derive(Debug, Clone)]
pub struct PaperInfo {
    pub title: String,
    pub abstract_text: String,
    pub venue: String,
}

/// Relevance result
#[derive(Debug, Clone)]
pub struct RelevanceResult {
    pub is_relevant: bool,
    pub reason: Option<String>,
}

/// Numeric relevance score for sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelevanceScore {
    pub score: u8,
    pub reason: Option<String>,
}

/// Chat message for API
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// LLM Provider trait - all providers must implement this
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name for logging
    fn name(&self) -> &str;

    /// Send a chat completion request and get the response
    async fn chat_completion(&self, messages: Vec<ChatMessage>) -> Result<String>;
}

/// Build the prompt for relevance judgment (English)
///
/// # Arguments
/// * `keyword` - The search keyword
/// * `paper` - Paper information (title, abstract, venue)
/// * `content_filter_help` - Optional user description of desired research direction
fn build_prompt(keyword: &str, paper: &PaperInfo, content_filter_help: Option<&str>) -> String {
    let user_context = match content_filter_help {
        Some(help) if !help.trim().is_empty() => format!(
            "\nUser's Research Focus: {}\n(Consider this context when evaluating relevance)",
            help
        ),
        _ => String::new(),
    };

    format!(
        r#"Determine if the following paper is relevant to the search keyword.

Search Keyword: {}{}

Paper Title: {}
Abstract: {}
Journal/Venue: {}

Reply with ONLY "YES" or "NO".
- YES: The paper's topic is highly relevant to the keyword (and user's research focus if provided)
- NO: The paper's topic is unrelated or only weakly related

Answer:"#,
        keyword,
        user_context,
        paper.title,
        if paper.abstract_text.is_empty() {
            "(No abstract available)"
        } else {
            &paper.abstract_text
        },
        if paper.venue.is_empty() {
            "(Unknown)"
        } else {
            &paper.venue
        }
    )
}

/// Build the prompt for numeric relevance scoring.
fn build_score_prompt(
    keyword: &str,
    paper: &PaperInfo,
    content_filter_help: Option<&str>,
) -> String {
    let user_context = match content_filter_help {
        Some(help) if !help.trim().is_empty() => format!(
            "\nUser's Research Focus: {}\nUse this as the primary relevance criterion.",
            help.trim()
        ),
        _ => String::new(),
    };

    format!(
        r#"Score this academic paper's relevance to the search request.

Search Keyword: {}{}

Paper Title: {}
Abstract: {}
Journal/Venue: {}

Return ONLY compact JSON with this exact shape:
{{"score": <integer 0-100>, "reason": "<short reason>"}}

Scoring guide:
- 90-100: directly answers the search request and research focus
- 70-89: clearly related, useful, but not a perfect focus match
- 40-69: partially related or method/domain mismatch
- 0-39: weakly related or unrelated

JSON:"#,
        keyword,
        user_context,
        paper.title,
        if paper.abstract_text.is_empty() {
            "(No abstract available)"
        } else {
            &paper.abstract_text
        },
        if paper.venue.is_empty() {
            "(Unknown)"
        } else {
            &paper.venue
        }
    )
}

fn extract_json_object(raw: &str) -> Result<&str> {
    let start = raw.find('{').ok_or_else(|| {
        GscholarError::Parse("LLM relevance score response did not contain JSON object".to_string())
    })?;
    let end = raw.rfind('}').ok_or_else(|| {
        GscholarError::Parse(
            "LLM relevance score response did not contain JSON object end".to_string(),
        )
    })?;
    if end < start {
        return Err(GscholarError::Parse(
            "LLM relevance score response JSON object was malformed".to_string(),
        ));
    }
    Ok(&raw[start..=end])
}

fn parse_score_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

fn normalize_score(score: f64) -> u8 {
    score.round().clamp(0.0, 100.0) as u8
}

fn normalize_reason(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(240).collect())
}

/// Parse a numeric relevance score response from an LLM.
pub(crate) fn parse_relevance_score_response(raw: &str) -> Result<RelevanceScore> {
    let json_text = extract_json_object(raw)?;
    let value: Value = serde_json::from_str(json_text)?;
    let score = value
        .get("score")
        .and_then(parse_score_value)
        .ok_or_else(|| {
            GscholarError::Parse("LLM relevance score response missing numeric score".to_string())
        })?;

    Ok(RelevanceScore {
        score: normalize_score(score),
        reason: normalize_reason(value.get("reason")),
    })
}

/// LLM-based relevance filter using any provider
pub struct LlmRelevanceFilter {
    /// Available LLM providers (first is primary, rest are fallbacks)
    pub providers: Vec<Arc<dyn LlmProvider>>,
    semaphore: Arc<Semaphore>,
}

impl LlmRelevanceFilter {
    /// Create a new LLM relevance filter with a list of providers (primary first, then fallbacks)
    pub fn new(providers: Vec<Arc<dyn LlmProvider>>) -> Self {
        Self {
            providers,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT)),
        }
    }

    /// Create with AIPing provider (default)
    pub fn with_aiping(api_key: Option<&str>) -> Result<Self> {
        let provider = Arc::new(AiPingProvider::new(api_key)?);
        Ok(Self::new(vec![provider]))
    }

    /// Create with SiliconFlow provider
    pub fn with_siliconflow(api_key: Option<&str>) -> Result<Self> {
        let provider = Arc::new(SiliconFlowProvider::new(api_key)?);
        Ok(Self::new(vec![provider]))
    }

    /// Create with BigModel provider
    pub fn with_bigmodel(api_key: Option<&str>) -> Result<Self> {
        let provider = Arc::new(BigModelProvider::new(api_key)?);
        Ok(Self::new(vec![provider]))
    }

    /// Create from provider name string (for config file usage)
    pub fn from_provider_name(name: &str, api_key: Option<&str>) -> Result<Self> {
        match name.to_lowercase().as_str() {
            "aiping" => Self::with_aiping(api_key),
            "siliconflow" => Self::with_siliconflow(api_key),
            "bigmodel" => Self::with_bigmodel(api_key),
            _ => Err(GscholarError::Config(format!(
                "Unknown LLM provider: '{}'. Supported: aiping, siliconflow, bigmodel",
                name
            ))),
        }
    }

    /// Build filter from server config.
    ///
    /// This centralizes provider discovery/loading in LLM module so server only
    /// needs to call one function.
    pub fn build_from_config(
        llm_config: &crate::server::config::LlmSection,
    ) -> Result<Option<Arc<Self>>> {
        let order = llm_config.provider_order();
        if order.is_empty() {
            warn!("LLM filter enabled but no providers configured");
            return Ok(None);
        }

        info!(order = ?order, "Building LLM providers from config");

        let mut providers: Vec<Arc<dyn LlmProvider>> = Vec::new();
        let registry = provider_registry();

        for name in &order {
            let cfg = match llm_config.resolve_provider_config(name) {
                Some(c) => c,
                None => {
                    warn!(provider = %name, "Provider listed but config section missing, skipping");
                    continue;
                }
            };

            match build_provider(&registry, name, &cfg) {
                Ok(provider) => {
                    info!(
                        provider = %provider.name(),
                        configured_name = %name,
                        model = %cfg.model,
                        endpoint = ?cfg.endpoint,
                        "LLM provider initialized"
                    );
                    providers.push(provider);
                }
                Err(error) => {
                    warn!(
                        provider = %name,
                        model = %cfg.model,
                        error = %error,
                        "Failed to initialize LLM provider, skipping"
                    );
                }
            }
        }

        if providers.is_empty() {
            warn!("All configured LLM providers failed to initialize");
            return Ok(None);
        }

        info!(
            count = providers.len(),
            primary = %providers[0].name(),
            "LLM relevance filter enabled"
        );

        Ok(Some(Arc::new(Self::new(providers))))
    }

    /// Build runtime providers from Web/DB configuration.
    pub fn build_from_runtime_configs(
        configs: &[crate::db::llm_providers::RuntimeProvider],
    ) -> Result<Option<Arc<Self>>> {
        let mut providers: Vec<Arc<dyn LlmProvider>> = Vec::new();

        for cfg in configs {
            let interface_type = InterfaceType::parse(&cfg.interface_type)?;
            match OpenAiCompatibleProvider::new(
                &cfg.name,
                interface_type,
                &cfg.endpoint,
                &cfg.model,
                &cfg.api_key,
            ) {
                Ok(provider) => {
                    info!(
                        provider = %cfg.name,
                        interface_type = interface_type.as_str(),
                        model = %cfg.model,
                        "DB-configured LLM provider initialized"
                    );
                    providers.push(Arc::new(provider));
                }
                Err(error) => {
                    warn!(
                        provider = %cfg.name,
                        error = %error,
                        "Skipping invalid DB-configured LLM provider"
                    );
                }
            }
        }

        if providers.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Arc::new(Self::new(providers))))
        }
    }

    /// Check relevance of a single paper (with fallback)
    ///
    /// # Arguments
    /// * `keyword` - Search keyword
    /// * `paper` - Paper information
    /// * `content_filter_help` - Optional user description of desired research direction
    pub async fn check_relevance(
        &self,
        keyword: &str,
        paper: &PaperInfo,
        content_filter_help: Option<&str>,
    ) -> Result<RelevanceResult> {
        // Acquire semaphore permit for concurrency control
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| GscholarError::Parse(format!("Semaphore error: {}", e)))?;

        let prompt = build_prompt(keyword, paper, content_filter_help);

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }];

        let mut last_error = None;

        for provider in &self.providers {
            debug!(title = %paper.title, provider = %provider.name(), "Sending LLM relevance check");

            match provider.chat_completion(messages.clone()).await {
                Ok(answer) => {
                    let is_relevant = answer.trim().to_uppercase().starts_with("YES");

                    debug!(
                        title = %paper.title,
                        provider = %provider.name(),
                        answer = %answer,
                        relevant = is_relevant,
                        "LLM relevance result"
                    );

                    return Ok(RelevanceResult {
                        is_relevant,
                        reason: Some(answer),
                    });
                }
                Err(e) => {
                    warn!(
                        title = %paper.title,
                        provider = %provider.name(),
                        error = %e,
                        "LLM provider failed, trying next fallback if available"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(GscholarError::Api {
            code: 503,
            message: "All LLM providers failed".to_string(),
        }))
    }

    /// Score relevance of a single paper (with provider fallback).
    pub async fn score_relevance(
        &self,
        keyword: &str,
        paper: &PaperInfo,
        content_filter_help: Option<&str>,
    ) -> Result<RelevanceScore> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| GscholarError::Parse(format!("Semaphore error: {}", e)))?;

        let prompt = build_score_prompt(keyword, paper, content_filter_help);
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
        }];

        let mut last_error = None;

        for provider in &self.providers {
            debug!(title = %paper.title, provider = %provider.name(), "Sending LLM relevance score request");

            match provider.chat_completion(messages.clone()).await {
                Ok(answer) => match parse_relevance_score_response(&answer) {
                    Ok(score) => {
                        debug!(
                            title = %paper.title,
                            provider = %provider.name(),
                            score = score.score,
                            "LLM relevance score parsed"
                        );
                        return Ok(score);
                    }
                    Err(e) => {
                        warn!(
                            title = %paper.title,
                            provider = %provider.name(),
                            error = %e,
                            answer = %answer,
                            "LLM relevance score parse failed, trying next fallback if available"
                        );
                        last_error = Some(e);
                    }
                },
                Err(e) => {
                    warn!(
                        title = %paper.title,
                        provider = %provider.name(),
                        error = %e,
                        "LLM provider failed during scoring, trying next fallback if available"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(GscholarError::Api {
            code: 503,
            message: "All LLM providers failed".to_string(),
        }))
    }

    /// Batch score relevance for multiple papers (with concurrency limit).
    pub async fn batch_score_relevance(
        &self,
        keyword: &str,
        papers: Vec<PaperInfo>,
        content_filter_help: Option<&str>,
    ) -> Vec<Result<RelevanceScore>> {
        use futures::stream::{self, StreamExt};

        let count = papers.len();
        let has_context = content_filter_help.is_some();
        info!(
            count = count,
            keyword = keyword,
            has_context = has_context,
            primary_provider = %self.providers.first().map(|p| p.name()).unwrap_or("none"),
            "Starting batch LLM relevance scoring"
        );

        let context: Option<String> = content_filter_help.map(|s| s.to_string());

        let mut indexed_results: Vec<(usize, Result<RelevanceScore>)> = stream::iter(
            papers.into_iter().enumerate(),
        )
        .map(|(idx, paper)| {
            let keyword = keyword.to_string();
            let ctx = context.clone();
            async move {
                let result = self.score_relevance(&keyword, &paper, ctx.as_deref()).await;
                if let Err(error) = &result {
                    warn!(idx = idx, error = %error, "LLM relevance scoring failed for paper");
                }
                (idx, result)
            }
        })
        .buffer_unordered(MAX_CONCURRENT)
        .collect()
        .await;

        indexed_results.sort_by_key(|(idx, _)| *idx);
        indexed_results
            .into_iter()
            .map(|(_, result)| result)
            .collect()
    }

    /// Batch check relevance for multiple papers (with concurrency limit)
    ///
    /// # Arguments
    /// * `keyword` - Search keyword
    /// * `papers` - Vector of paper information
    /// * `content_filter_help` - Optional user description of desired research direction
    pub async fn batch_check_relevance(
        &self,
        keyword: &str,
        papers: Vec<PaperInfo>,
        content_filter_help: Option<&str>,
    ) -> Vec<RelevanceResult> {
        use futures::stream::{self, StreamExt};

        let count = papers.len();
        let has_context = content_filter_help.is_some();
        info!(
            count = count,
            keyword = keyword,
            has_context = has_context,
            primary_provider = %self.providers.first().map(|p| p.name()).unwrap_or("none"),
            "Starting batch LLM relevance check"
        );

        // Clone the context for use in async closures
        let context: Option<String> = content_filter_help.map(|s| s.to_string());

        let mut indexed_results: Vec<(usize, RelevanceResult)> =
            stream::iter(papers.into_iter().enumerate())
                .map(|(idx, paper)| {
                    let keyword = keyword.to_string();
                    let ctx = context.clone();
                    async move {
                        let result = match self
                            .check_relevance(&keyword, &paper, ctx.as_deref())
                            .await
                        {
                            Ok(result) => result,
                            Err(e) => {
                                warn!(idx = idx, error = %e, "LLM check failed, assuming relevant");
                                RelevanceResult {
                                    is_relevant: true, // Default to relevant on error
                                    reason: Some(format!("Error: {}", e)),
                                }
                            }
                        };
                        (idx, result)
                    }
                })
                .buffer_unordered(MAX_CONCURRENT)
                .collect()
                .await;

        indexed_results.sort_by_key(|(idx, _)| *idx);
        let results: Vec<RelevanceResult> = indexed_results
            .into_iter()
            .map(|(_, result)| result)
            .collect();

        let relevant_count = results.iter().filter(|r| r.is_relevant).count();
        info!(
            total = count,
            relevant = relevant_count,
            filtered_out = count - relevant_count,
            "Batch LLM relevance check complete"
        );

        results
    }
}

fn provider_registry() -> ProviderRegistry {
    let mut registry: ProviderRegistry = HashMap::new();
    providers::register_all(&mut registry);
    registry
}

fn build_provider(
    registry: &ProviderRegistry,
    name: &str,
    cfg: &crate::server::config::ProviderConfig,
) -> Result<Arc<dyn LlmProvider>> {
    let normalized = name.to_lowercase();
    if let Some(builder) = registry.get(&normalized) {
        return builder(cfg);
    }

    let mut supported: Vec<String> = registry.keys().cloned().collect();
    supported.sort();
    Err(GscholarError::Config(format!(
        "Unknown provider '{}' in LLM config. Supported: {}",
        name,
        supported.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::config::{LlmSection, ProviderConfig};
    use std::collections::HashMap;

    #[test]
    fn test_build_prompt_without_context() {
        let paper = PaperInfo {
            title: "Machine Learning for Rock".to_string(),
            abstract_text: "This paper studies ML.".to_string(),
            venue: "Nature".to_string(),
        };
        let prompt = build_prompt("machine learning", &paper, None);
        assert!(prompt.contains("machine learning"));
        assert!(prompt.contains("Machine Learning for Rock"));
        assert!(prompt.contains("YES"));
        assert!(prompt.contains("NO"));
        // Verify English prompt
        assert!(prompt.contains("Determine if the following paper"));
        // Should NOT contain user context section when None
        assert!(!prompt.contains("User's Research Focus"));
    }

    #[test]
    fn test_build_prompt_with_context() {
        let paper = PaperInfo {
            title: "Machine Learning for Rock".to_string(),
            abstract_text: "This paper studies ML.".to_string(),
            venue: "Nature".to_string(),
        };
        let context = "I need papers about real-time prediction methods";
        let prompt = build_prompt("machine learning", &paper, Some(context));

        // Should contain user context
        assert!(prompt.contains("User's Research Focus"));
        assert!(prompt.contains("real-time prediction methods"));
        assert!(prompt.contains("Consider this context"));
    }

    #[test]
    fn test_build_prompt_with_empty_context() {
        let paper = PaperInfo {
            title: "Test".to_string(),
            abstract_text: "Abstract".to_string(),
            venue: "Journal".to_string(),
        };
        // Empty string should be treated as no context
        let prompt = build_prompt("test", &paper, Some("   "));
        assert!(!prompt.contains("User's Research Focus"));
    }

    #[test]
    fn test_parse_relevance_score_response_json() {
        let parsed = parse_relevance_score_response(
            r#"{"score": 87, "reason": "Directly studies neural prediction of rock strength."}"#,
        )
        .expect("parse score response");

        assert_eq!(parsed.score, 87);
        assert_eq!(
            parsed.reason.as_deref(),
            Some("Directly studies neural prediction of rock strength.")
        );
    }

    #[test]
    fn test_parse_relevance_score_response_markdown_and_clamps() {
        let parsed = parse_relevance_score_response(
            "```json\n{\"score\": 133, \"reason\": \"Strong match\"}\n```",
        )
        .expect("parse markdown score response");

        assert_eq!(parsed.score, 100);
        assert_eq!(parsed.reason.as_deref(), Some("Strong match"));
    }

    #[test]
    fn test_build_from_config_disabled() {
        let cfg = LlmSection {
            default_provider: "".to_string(),
            enable_filter: false,
            strict_filter: false,
            providers: vec![],
            provider_configs: HashMap::new(),
        };
        let filter = LlmRelevanceFilter::build_from_config(&cfg).unwrap();
        assert!(filter.is_none());
    }

    #[test]
    fn test_build_from_config_with_bigmodel_provider_section() {
        let mut dynamic = HashMap::new();
        dynamic.insert(
            "bigmodel".to_string(),
            ProviderConfig {
                api_key: "bm-test".to_string(),
                model: "GLM-4.7-Flash".to_string(),
                endpoint: Some("https://open.bigmodel.cn/api/paas/v4/chat/completions".to_string()),
            },
        );
        let cfg = LlmSection {
            default_provider: "bigmodel".to_string(),
            enable_filter: true,
            strict_filter: false,
            providers: vec!["bigmodel".to_string()],
            provider_configs: dynamic,
        };

        let filter = LlmRelevanceFilter::build_from_config(&cfg).unwrap();
        assert!(filter.is_some());
        let filter = filter.unwrap();
        assert_eq!(filter.providers[0].name(), "BigModel");
    }
}
