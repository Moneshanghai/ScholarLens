//! Generic OpenAI-compatible LLM provider.
//!
//! Supports both `/v1/chat/completions` and `/v1/responses` style APIs.

use super::provider_core::{to_openai_messages, ProviderRuntime, ProviderTrial};
use super::{ChatMessage, LlmProvider};
use crate::error::{GscholarError, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceType {
    ChatCompletions,
    Responses,
}

impl InterfaceType {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "chat_completions" | "chat" | "completions" => Ok(Self::ChatCompletions),
            "responses" | "response" => Ok(Self::Responses),
            other => Err(GscholarError::Config(format!(
                "Unsupported LLM interface_type '{}'. Supported: chat_completions, responses",
                other
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat_completions",
            Self::Responses => "responses",
        }
    }
}

pub fn normalize_endpoint_for_interface(endpoint: &str, interface_type: InterfaceType) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let lower = trimmed.to_lowercase();
    let target_suffix = match interface_type {
        InterfaceType::ChatCompletions => "chat/completions",
        InterfaceType::Responses => "responses",
    };
    let full_suffix = format!("/v1/{}", target_suffix);
    let short_suffix = format!("/{}", target_suffix);

    if lower.ends_with(&full_suffix) || lower.ends_with(&short_suffix) {
        return trimmed.to_string();
    }

    if lower.ends_with("/v1") {
        format!("{}/{}", trimmed, target_suffix)
    } else {
        format!("{}/v1/{}", trimmed, target_suffix)
    }
}

pub struct OpenAiCompatibleProvider {
    runtime: ProviderRuntime,
    interface_type: InterfaceType,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        name: &str,
        interface_type: InterfaceType,
        endpoint: &str,
        model: &str,
        api_key: &str,
    ) -> Result<Self> {
        if name.trim().is_empty() {
            return Err(GscholarError::Config(
                "LLM provider name is required".to_string(),
            ));
        }
        if endpoint.trim().is_empty() {
            return Err(GscholarError::Config(
                "LLM provider endpoint is required".to_string(),
            ));
        }
        if model.trim().is_empty() {
            return Err(GscholarError::Config(
                "LLM provider model is required".to_string(),
            ));
        }
        if api_key.trim().is_empty() {
            return Err(GscholarError::Config(
                "LLM provider API key is required".to_string(),
            ));
        }

        let endpoint = normalize_endpoint_for_interface(endpoint, interface_type);

        Ok(Self {
            runtime: ProviderRuntime::new(name.trim(), endpoint, api_key.trim(), model.trim())?,
            interface_type,
        })
    }

    pub fn new_for_test(
        name: &str,
        interface_type: InterfaceType,
        endpoint: &str,
        model: &str,
        api_key: &str,
    ) -> Result<Self> {
        Self::new(name, interface_type, endpoint, model, api_key)
    }

    pub fn build_payload_for_test(&self, prompt: &str) -> Value {
        self.build_payload(vec![ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }])
    }

    pub fn endpoint_for_test(&self) -> &str {
        &self.runtime.endpoint
    }

    pub fn parse_chat_response(body: &str) -> Result<String> {
        let parsed: Value = serde_json::from_str(body)
            .map_err(|e| GscholarError::Parse(format!("Failed to parse chat response: {}", e)))?;

        let content = parsed
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();

        if content.is_empty() {
            Err(GscholarError::Parse(
                "Empty chat completion response content".to_string(),
            ))
        } else {
            Ok(content)
        }
    }

    pub fn parse_responses_response(body: &str) -> Result<String> {
        let parsed: Value = serde_json::from_str(body).map_err(|e| {
            GscholarError::Parse(format!("Failed to parse responses API response: {}", e))
        })?;

        if let Some(text) = parsed
            .get("output_text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(text.to_string());
        }

        let mut parts = Vec::new();
        if let Some(output) = parsed.get("output").and_then(Value::as_array) {
            for item in output {
                if let Some(content) = item.get("content").and_then(Value::as_array) {
                    for block in content {
                        if let Some(text) = block
                            .get("text")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                        {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
        }

        let joined = parts.join("\n").trim().to_string();
        if joined.is_empty() {
            Err(GscholarError::Parse(
                "Empty responses API response content".to_string(),
            ))
        } else {
            Ok(joined)
        }
    }
}

#[async_trait]
impl ProviderTrial for OpenAiCompatibleProvider {
    fn runtime(&self) -> &ProviderRuntime {
        &self.runtime
    }

    fn build_payload(&self, messages: Vec<ChatMessage>) -> Value {
        match self.interface_type {
            InterfaceType::ChatCompletions => json!({
                "model": self.runtime.model,
                "messages": to_openai_messages(messages),
                "stream": false,
            }),
            InterfaceType::Responses => {
                let input = messages
                    .into_iter()
                    .map(|m| {
                        json!({
                            "role": m.role,
                            "content": [{ "type": "input_text", "text": m.content }]
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "model": self.runtime.model,
                    "input": input,
                })
            }
        }
    }

    fn parse_response_body(&self, body: &str) -> Result<String> {
        match self.interface_type {
            InterfaceType::ChatCompletions => Self::parse_chat_response(body),
            InterfaceType::Responses => Self::parse_responses_response(body),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        self.runtime.provider_name.as_str()
    }

    async fn chat_completion(&self, messages: Vec<ChatMessage>) -> Result<String> {
        self.execute_trial(messages).await
    }
}
