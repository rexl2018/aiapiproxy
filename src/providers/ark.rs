//! Ark Provider implementation
//!
//! Supports OpenAI Responses API format with Bearer token authentication
//! Ark is a model service that provides access to various models including GLM

use super::{BoxStream, Provider};
use crate::config::{ModelConfig, ProviderConfig};
use crate::models::openai::*;
use crate::utils::logging::VERBOSE_REQUEST_LOGGING;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{debug, error, warn};

/// Create a filtered version of Responses API request for logging
fn create_log_responses_request(request: &ResponsesApiRequest) -> serde_json::Value {
    if VERBOSE_REQUEST_LOGGING {
        serde_json::to_value(request).unwrap_or(serde_json::json!({"error": "failed to serialize"}))
    } else {
        serde_json::json!({
            "model": request.model,
            "max_output_tokens": request.max_output_tokens,
            "temperature": request.temperature,
            "stream": request.stream,
            "input_count": request.input.len(),
            "tools_count": request.tools.as_ref().map(|t| t.len()).unwrap_or(0),
            "tools": "[omitted]",
            "instructions": "[omitted]",
        })
    }
}

// ====== Responses API Structures ======

/// Ark Responses API Request format
#[derive(Debug, Serialize)]
struct ResponsesApiRequest {
    model: String,
    /// Input can contain various types:
    /// - Messages: { type: "message", role: "user"|"assistant", content: [...], status: "completed" }
    /// - Function calls: { type: "function_call", call_id, name, arguments, status: "completed" }
    /// - Function results: { type: "function_call_output", call_id, output, status: "completed" }
    input: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
}

/// OpenAI Responses API Response format
#[derive(Debug, Deserialize)]
struct ResponsesApiResponse {
    id: String,
    #[serde(default)]
    model: Option<String>,
    output: Vec<ResponsesOutput>,
    #[serde(default)]
    usage: Option<ResponsesUsage>,
    status: String,
    #[serde(default)]
    created_at: Option<u64>,
    #[serde(default)]
    incomplete_details: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ResponsesOutput {
    #[serde(rename = "type")]
    output_type: String,
    #[serde(default)]
    content: Option<Vec<ResponsesContent>>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    status: Option<String>,
    // For tool_use output
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
    // For reasoning output
    #[serde(default)]
    summary: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ResponsesContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsesUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    total_tokens: Option<u32>,
}

/// Ark Provider
/// 
/// Uses OpenAI Responses API format with Bearer token authentication
/// Endpoint: /responses
pub struct ArkProvider {
    client: Client,
    stream_client: Client,
}

impl ArkProvider {
    /// Create a new Ark provider with default timeouts
    pub fn new() -> Result<Self> {
        Self::with_timeouts(30, 300)
    }
    
    /// Create a new Ark provider with custom timeouts
    pub fn with_timeouts(timeout_secs: u64, stream_timeout_secs: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("aiapiproxy/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;
        
        let stream_client = Client::builder()
            .timeout(Duration::from_secs(stream_timeout_secs))
            .user_agent("aiapiproxy/0.1.0")
            .build()
            .context("Failed to create streaming HTTP client")?;
        
        Ok(Self { client, stream_client })
    }
    
    /// Build request URL
    fn build_url(&self, provider_config: &ProviderConfig, endpoint: &str) -> String {
        let base_url = provider_config.base_url.trim_end_matches('/');
        format!("{}{}", base_url, endpoint)
    }
    
    /// Get API key from config or environment variable
    fn get_api_key(&self, provider_config: &ProviderConfig) -> String {
        if provider_config.api_key.is_empty() {
            std::env::var("ARK_API_KEY").unwrap_or_default()
        } else {
            provider_config.api_key.clone()
        }
    }
    
    /// Get the mode from model options, defaults to "responses"
    fn get_mode<'a>(&self, model_config: &'a ModelConfig) -> &'a str {
        model_config.options.mode.as_deref().unwrap_or("responses")
    }
    
    /// Add Ark-specific headers (Bearer token auth)
    fn add_ark_headers(
        &self, 
        builder: reqwest::RequestBuilder, 
        provider_config: &ProviderConfig,
    ) -> reqwest::RequestBuilder {
        let api_key = self.get_api_key(provider_config);
        
        let mut builder = builder
            .header("Authorization", format!("Bearer {}", api_key))
            .header("HTTP-Referer", "https://aiapiproxy.local")
            .header("X-Title", "AIAPIProxy");
        
        // Add custom headers from config
        for (key, value) in &provider_config.options.headers {
            builder = builder.header(key, value);
        }
        
        builder
    }
    
    /// Convert OpenAI request to Responses API format
    fn convert_to_responses_api(&self, request: &OpenAIRequest, model_config: &ModelConfig) -> Result<ResponsesApiRequest> {
        let mut input: Vec<Value> = Vec::new();
        let mut system_instructions: Option<String> = None;
        
        // First pass: collect all tool result call_ids
        let mut tool_result_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for msg in &request.messages {
            if msg.role == "tool" {
                if let Some(id) = &msg.tool_call_id {
                    tool_result_ids.insert(id.clone());
                }
            }
        }
        
        // Debug: log message roles and tool info
        for (i, msg) in request.messages.iter().enumerate() {
            let has_tool_calls = msg.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0);
            let tool_call_id = msg.tool_call_id.as_ref().map(|s| s.as_str()).unwrap_or("none");
            debug!("Message {}: role={}, has_tool_calls={}, tool_call_id={}", 
                   i, msg.role, has_tool_calls, tool_call_id);
        }
        
        for msg in &request.messages {
            let role = msg.role.as_str();
            
            // Extract system message as instructions
            if role == "system" {
                if let Some(content) = &msg.content {
                    system_instructions = Some(content.extract_text());
                }
                continue;
            }
            
            // Handle tool role -> function_call_output
            if role == "tool" {
                if let Some(tool_call_id) = &msg.tool_call_id {
                    let output = msg.content.as_ref()
                        .map(|c| c.extract_text())
                        .unwrap_or_default();
                    debug!("Adding function_call_output with call_id={}", tool_call_id);
                    input.push(serde_json::json!({
                        "type": "function_call_output",
                        "call_id": tool_call_id,
                        "output": output,
                        "status": "completed",
                        "partial": false
                    }));
                } else {
                    warn!("Tool message without tool_call_id, skipping");
                }
                continue;
            }
            
            // Handle assistant with tool_calls -> function_call items
            if role == "assistant" {
                let has_tool_calls = msg.tool_calls.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
                
                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        if let Some(id) = &tc.id {
                            // Only add function_call if there's a matching function_call_output
                            if tool_result_ids.contains(id) {
                                debug!("Adding function_call with call_id={}, name={:?}", id, tc.function.name);
                                input.push(serde_json::json!({
                                    "type": "function_call",
                                    "call_id": id,
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments.clone().unwrap_or_default(),
                                    "status": "completed",
                                    "partial": false
                                }));
                            } else {
                                warn!("Skipping orphan function_call with call_id={} (no matching output)", id);
                            }
                        } else {
                            warn!("Tool call without id, skipping");
                        }
                    }
                }
                
                // Only add assistant text content if there are NO tool calls
                if !has_tool_calls {
                    if let Some(content) = &msg.content {
                        let text = content.extract_text();
                        if !text.is_empty() {
                            input.push(serde_json::json!({
                                "type": "message",
                                "role": "assistant",
                                "content": [{ "type": "output_text", "text": text }],
                                "status": "completed",
                                "partial": false
                            }));
                        }
                    }
                }
                continue;
            }
            
            // Handle user messages
            if role == "user" {
                let content = if let Some(c) = &msg.content {
                    match c {
                        OpenAIContent::Text(text) => {
                            vec![serde_json::json!({ "type": "input_text", "text": text })]
                        },
                        OpenAIContent::Array(parts) => {
                            parts.iter().map(|p| {
                                match p {
                                    OpenAIContentPart::Text { text } => {
                                        serde_json::json!({ "type": "input_text", "text": text })
                                    },
                                    OpenAIContentPart::ImageUrl { image_url } => {
                                        serde_json::json!({
                                            "type": "input_image",
                                            "image_url": image_url.url
                                        })
                                    },
                                }
                            }).collect()
                        }
                    }
                } else {
                    vec![serde_json::json!({ "type": "input_text", "text": "" })]
                };
                
                input.push(serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": content,
                    "status": "completed",
                    "partial": false
                }));
            }
        }
        
        // Convert tools to Responses API format
        let tools = request.tools.as_ref().map(|t| {
            t.iter().map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "name": tool.function.name,
                    "description": tool.function.description,
                    "parameters": tool.function.parameters
                })
            }).collect()
        });
        
        // Ensure max_output_tokens is reasonable
        let max_output_tokens = match (request.max_tokens, model_config.max_tokens) {
            (Some(req), Some(cfg)) => Some(req.max(cfg)),
            (Some(req), None) => Some(req.max(8192)),
            (None, Some(cfg)) => Some(cfg),
            (None, None) => Some(8192),
        };
        debug!("📊 Ark Responses API max_output_tokens: request={:?}, config={:?}, final={:?}",
               request.max_tokens, model_config.max_tokens, max_output_tokens);
        
        // Only include temperature if the model supports it
        // Reasoning models (o1, o3, etc.) don't support temperature
        let temperature = if model_config.options.supports_temperature {
            request.temperature.or(model_config.temperature)
        } else {
            debug!("📊 Model {} does not support temperature, skipping parameter", model_config.name);
            None
        };
        
        Ok(ResponsesApiRequest {
            model: model_config.name.clone(),
            input,
            max_output_tokens,
            temperature,
            stream: None,
            tools,
            instructions: system_instructions,
        })
    }
    
    /// Convert Responses API response to OpenAI format
    fn convert_from_responses_api(&self, response: ResponsesApiResponse) -> OpenAIResponse {
        let mut content_text = String::new();
        let mut tool_calls: Vec<OpenAIToolCall> = Vec::new();
        
        for output in &response.output {
            match output.output_type.as_str() {
                "message" => {
                    if let Some(contents) = &output.content {
                        for c in contents {
                            if c.content_type == "output_text" {
                                if let Some(text) = &c.text {
                                    content_text.push_str(text);
                                }
                            }
                        }
                    }
                },
                "function_call" | "tool_use" => {
                    if let (Some(name), Some(arguments)) = (&output.name, &output.arguments) {
                        tool_calls.push(OpenAIToolCall {
                            id: output.call_id.clone(),
                            tool_type: Some("function".to_string()),
                            function: OpenAIFunctionCall {
                                name: Some(name.clone()),
                                arguments: Some(arguments.clone()),
                            },
                            signature: None,
                            extra_content: None,
                        });
                    }
                },
                "reasoning" => {
                    debug!("Ark Responses API: got reasoning output with {} summary items", 
                           output.summary.as_ref().map(|s| s.len()).unwrap_or(0));
                },
                other => {
                    debug!("Ark Responses API: ignoring unknown output type: {}", other);
                }
            }
        }
        
        // Build choice
        let choice = OpenAIChoice {
            index: 0,
            message: OpenAIMessage {
                role: "assistant".to_string(),
                content: if content_text.is_empty() { None } else { Some(OpenAIContent::Text(content_text)) },
                tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                tool_call_id: None,
                name: None,
            },
            logprobs: None,
            finish_reason: Some(match response.status.as_str() {
                "completed" => "stop".to_string(),
                "cancelled" => "stop".to_string(),
                _ => "stop".to_string(),
            }),
        };
        
        let usage = response.usage.map(|u| OpenAIUsage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.total_tokens.unwrap_or(u.input_tokens + u.output_tokens),
        });
        
        OpenAIResponse {
            id: response.id,
            object: "chat.completion".to_string(),
            created: 0,
            model: response.model.unwrap_or_default(),
            choices: vec![choice],
            usage,
            system_fingerprint: None,
        }
    }
    
    /// Non-streaming request handler
    async fn responses_mode(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse> {
        debug!("Ark: Using Responses API mode");
        
        // Convert OpenAI request to Responses API format
        let responses_request = self.convert_to_responses_api(&request, model_config)?;
        
        let log_request = create_log_responses_request(&responses_request);
        if let Ok(req_json) = serde_json::to_string_pretty(&log_request) {
            debug!("📤 Ark Responses API Request:\n{}", req_json);
        }
        
        let url = self.build_url(provider_config, "/responses");
        
        let builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&responses_request);
        
        let response = self.add_ark_headers(builder, provider_config)
            .send()
            .await
            .context("Failed to send request to Ark")?;
        
        let status = response.status();
        
        if status.is_success() {
            let response_text = response.text().await
                .context("Failed to read Ark Responses API response body")?;
            
            debug!("📥 Ark Responses API Raw Response:\n{}", 
                   if response_text.len() > 1000 { &response_text[..1000] } else { &response_text });
            
            let responses_api_response: ResponsesApiResponse = serde_json::from_str(&response_text)
                .with_context(|| {
                    error!("Failed to parse Ark Responses API response. Raw response:\n{}", 
                           if response_text.len() > 2000 { &response_text[..2000] } else { &response_text });
                    "Failed to parse Ark Responses API response"
                })?;
            
            debug!("Ark Responses API request completed successfully");
            
            Ok(self.convert_from_responses_api(responses_api_response))
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("Ark API request failed: {} - {}", status, error_text);
            anyhow::bail!("Ark API request failed: {} - {}", status, error_text);
        }
    }
    
    /// Streaming request handler
    async fn responses_mode_stream(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        debug!("Ark: Using Responses API streaming mode");
        
        // Convert to Responses API format with stream=true
        let mut responses_request = self.convert_to_responses_api(&request, model_config)?;
        responses_request.stream = Some(true);
        
        let url = self.build_url(provider_config, "/responses");
        
        let builder = self.stream_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&responses_request);
        
        let response = self.add_ark_headers(builder, provider_config)
            .send()
            .await
            .context("Failed to send streaming request to Ark")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ark API request failed: {} - {}", status, error_text);
        }
        
        // Parse Responses API SSE stream and convert to OpenAI stream format
        let stream = response
            .bytes_stream()
            .filter_map(move |chunk_result| {
                match chunk_result {
                    Ok(chunk) => {
                        match std::str::from_utf8(&chunk) {
                            Ok(chunk_str) => {
                                Self::parse_responses_api_sse(chunk_str)
                            }
                            Err(e) => Some(Err(anyhow::anyhow!("Invalid UTF-8: {}", e))),
                        }
                    }
                    Err(e) => Some(Err(anyhow::anyhow!("Stream error: {}", e))),
                }
            });
        
        Ok(Box::pin(stream))
    }
    
    /// Parse Responses API SSE chunk and convert to OpenAI stream response
    fn parse_responses_api_sse(chunk_str: &str) -> Option<Result<OpenAIStreamResponse>> {
        for line in chunk_str.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    return None;
                }
                
                // Parse Responses API streaming event
                if let Ok(event) = serde_json::from_str::<Value>(data) {
                    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    
                    match event_type {
                        // Handle response start - send role to initialize the stream
                        "response.created" | "response.in_progress" => {
                            return Some(Ok(OpenAIStreamResponse {
                                id: event.get("response").and_then(|r| r.get("id")).and_then(|i| i.as_str()).unwrap_or("").to_string(),
                                object: "chat.completion.chunk".to_string(),
                                created: 0,
                                model: String::new(),
                                system_fingerprint: None,
                                choices: vec![OpenAIStreamChoice {
                                    index: 0,
                                    delta: OpenAIStreamDelta {
                                        role: Some("assistant".to_string()),
                                        content: None,
                                        tool_calls: None,
                                    },
                                    logprobs: None,
                                    finish_reason: None,
                                }],
                            }));
                        },
                        "response.output_text.delta" => {
                            if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                                return Some(Ok(OpenAIStreamResponse {
                                    id: event.get("response_id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                                    object: "chat.completion.chunk".to_string(),
                                    created: 0,
                                    model: String::new(),
                                    system_fingerprint: None,
                                    choices: vec![OpenAIStreamChoice {
                                        index: 0,
                                        delta: OpenAIStreamDelta {
                                            role: None,
                                            content: Some(delta.to_string()),
                                            tool_calls: None,
                                        },
                                        logprobs: None,
                                        finish_reason: None,
                                    }],
                                }));
                            }
                        },
                        "response.completed" | "response.done" => {
                            return Some(Ok(OpenAIStreamResponse {
                                id: event.get("response").and_then(|r| r.get("id")).and_then(|i| i.as_str()).unwrap_or("").to_string(),
                                object: "chat.completion.chunk".to_string(),
                                created: 0,
                                model: String::new(),
                                system_fingerprint: None,
                                choices: vec![OpenAIStreamChoice {
                                    index: 0,
                                    delta: OpenAIStreamDelta {
                                        role: None,
                                        content: None,
                                        tool_calls: None,
                                    },
                                    logprobs: None,
                                    finish_reason: Some("stop".to_string()),
                                }],
                            }));
                        },
                        _ => {
                            // Skip other event types
                        }
                    }
                }
            }
        }
        None
    }
}

#[async_trait]
impl Provider for ArkProvider {
    fn name(&self) -> &str {
        "ark"
    }
    
    async fn chat_complete(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse> {
        match self.get_mode(model_config) {
            "responses" => self.responses_mode(request, provider_config, model_config).await,
            other => {
                anyhow::bail!("Unsupported Ark mode: {}. Currently only 'responses' mode is supported.", other)
            }
        }
    }
    
    async fn chat_stream(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        match self.get_mode(model_config) {
            "responses" => self.responses_mode_stream(request, provider_config, model_config).await,
            other => {
                anyhow::bail!("Unsupported Ark mode: {}. Currently only 'responses' mode is supported.", other)
            }
        }
    }
}

impl Default for ArkProvider {
    fn default() -> Self {
        Self::new().expect("Failed to create default Ark provider")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderOptions;
    
    #[test]
    fn test_provider_creation() {
        let provider = ArkProvider::new();
        assert!(provider.is_ok());
    }
    
    #[test]
    fn test_provider_name() {
        let provider = ArkProvider::new().unwrap();
        assert_eq!(provider.name(), "ark");
    }
    
    #[test]
    fn test_build_url() {
        let provider = ArkProvider::new().unwrap();
        
        let config = ProviderConfig {
            provider_type: "ark".to_string(),
            base_url: "https://ark-ap-southeast.byteintl.net/api/v3".to_string(),
            api_key: "test-api-key".to_string(),
            options: ProviderOptions::default(),
            models: Default::default(),
        };
        
        let url = provider.build_url(&config, "/responses");
        assert_eq!(url, "https://ark-ap-southeast.byteintl.net/api/v3/responses");
    }
    
    #[test]
    fn test_get_api_key_from_config() {
        let provider = ArkProvider::new().unwrap();
        
        let config = ProviderConfig {
            provider_type: "ark".to_string(),
            base_url: "https://ark-ap-southeast.byteintl.net/api/v3".to_string(),
            api_key: "config-api-key".to_string(),
            options: ProviderOptions::default(),
            models: Default::default(),
        };
        
        let api_key = provider.get_api_key(&config);
        assert_eq!(api_key, "config-api-key");
    }
    
    #[test]
    fn test_get_api_key_from_env() {
        let provider = ArkProvider::new().unwrap();
        
        let config = ProviderConfig {
            provider_type: "ark".to_string(),
            base_url: "https://ark-ap-southeast.byteintl.net/api/v3".to_string(),
            api_key: "".to_string(), // Empty, should fallback to env
            options: ProviderOptions::default(),
            models: Default::default(),
        };
        
        // Set env var for test
        std::env::set_var("ARK_API_KEY", "env-api-key");
        let api_key = provider.get_api_key(&config);
        assert_eq!(api_key, "env-api-key");
        std::env::remove_var("ARK_API_KEY");
    }
}
