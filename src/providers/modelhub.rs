//! ModelHub Provider implementation
//!
//! Supports both OpenAI-compatible (responses) mode and Gemini mode

use super::{BoxStream, Provider};
use crate::config::{ModelConfig, ProviderConfig};
use crate::models::openai::*;
use crate::utils::logging::{create_request_log_summary, VERBOSE_REQUEST_LOGGING};
use crate::utils::thought_cache::{cache_thought_signature, get_cached_thought_signature};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{debug, error, warn};

/// Inject cached thought_signatures into tool_calls in the request
/// This is needed because Claude Code doesn't preserve our custom thought_signature field
fn inject_cached_thought_signatures(request: &mut OpenAIRequest) {
    for message in &mut request.messages {
        if message.role == "assistant" {
            if let Some(ref mut tool_calls) = message.tool_calls {
                for tc in tool_calls.iter_mut() {
                    // Skip if already has a signature
                    if tc.signature.is_some() {
                        continue;
                    }
                    
                    // Try to get cached signature
                    if let Some(id) = &tc.id {
                        if let Some(sig) = get_cached_thought_signature(id) {
                            debug!("💉 Injecting cached thought_signature for tool_call_id: {}", id);
                            tc.signature = Some(sig.clone());
                            tc.extra_content = Some(serde_json::json!({
                                "google": {
                                    "thought_signature": sig
                                }
                            }));
                        }
                    }
                }
            }
        }
    }
}

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

/// OpenAI Responses API Request format
#[derive(Debug, Serialize)]
struct ResponsesApiRequest {
    model: String,
    /// Input can contain various types:
    /// - Messages: { role: "user"|"assistant", content: [...] }
    /// - Function calls: { type: "function_call", call_id, name, arguments }
    /// - Function results: { type: "function_call_output", call_id, output }
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

/// Input message for Responses API
#[derive(Debug, Serialize)]
struct ResponsesInputMessage {
    role: String,
    content: Value,
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
    // Additional fields that may be present but we don't need
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

/// ModelHub Provider
/// 
/// Supports two modes:
/// - "responses": OpenAI-compatible pass-through
/// - "gemini": Gemini protocol adapter with request/response transformation
pub struct ModelHubProvider {
    client: Client,
    stream_client: Client,
}

impl ModelHubProvider {
    /// Create a new ModelHub provider with default timeouts
    pub fn new() -> Result<Self> {
        Self::with_timeouts(30, 300)
    }
    
    /// Create a new ModelHub provider with custom timeouts
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
    
    /// Build request URL with API key parameter
    fn build_url(&self, provider_config: &ProviderConfig, endpoint: &str) -> String {
        let base_url = provider_config.base_url.trim_end_matches('/');
        let mut url = format!("{}{}", base_url, endpoint);
        
        // Add API key as query parameter if configured
        if let Some(ref param_name) = provider_config.options.api_key_param {
            let api_key = if provider_config.api_key.is_empty() {
                std::env::var("MODELHUB_API_KEY").unwrap_or_default()
            } else {
                provider_config.api_key.clone()
            };
            
            if !api_key.is_empty() {
                url = format!("{}?{}={}", url, param_name, api_key);
            }
        }
        
        url
    }
    
    /// Get the mode from provider options
    fn get_mode<'a>(&self, provider_config: &'a ProviderConfig) -> &'a str {
        provider_config.options.mode.as_deref().unwrap_or("responses")
    }
    
    /// Add ModelHub-specific headers
    fn add_modelhub_headers(
        &self, 
        builder: reqwest::RequestBuilder, 
        provider_config: &ProviderConfig,
        session_id: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let mut builder = builder
            .header("HTTP-Referer", "https://aiapiproxy.local")
            .header("X-Title", "AIAPIProxy");
        
        // Add custom headers from config
        for (key, value) in &provider_config.options.headers {
            builder = builder.header(key, value);
        }
        
        // Add session_id in extra header for ModelHub server-side caching
        // Format: {"session_id": "XX"}
        if let Some(sid) = session_id {
            let extra_value = serde_json::json!({ "session_id": sid }).to_string();
            debug!("📎 Adding extra header for ModelHub: {}", extra_value);
            builder = builder.header("extra", extra_value);
        }
        
        builder
    }
    
    // =============================
    // OpenAI Responses Mode Methods
    // =============================
    
    async fn openai_responses_mode(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse> {
        debug!("ModelHub: Using Responses API mode");
        
        // Convert OpenAI request to Responses API format
        let responses_request = self.convert_to_responses_api(&request, model_config)?;
        
        let log_request = create_log_responses_request(&responses_request);
        if let Ok(req_json) = serde_json::to_string_pretty(&log_request) {
            debug!("📤 Responses API Request:\n{}", req_json);
        }
        
        let url = self.build_url(provider_config, "/responses");
        
        let builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&responses_request);
        
        let response = self.add_modelhub_headers(builder, provider_config, request.session_id.as_deref())
            .send()
            .await
            .context("Failed to send request")?;
        
        let status = response.status();
        
        if status.is_success() {
            // Get response text first for debugging
            let response_text = response.text().await
                .context("Failed to read Responses API response body")?;
            
            debug!("📥 Responses API Raw Response:\n{}", 
                   if response_text.len() > 1000 { &response_text[..1000] } else { &response_text });
            
            let responses_api_response: ResponsesApiResponse = serde_json::from_str(&response_text)
                .with_context(|| {
                    error!("Failed to parse Responses API response. Raw response:\n{}", 
                           if response_text.len() > 2000 { &response_text[..2000] } else { &response_text });
                    "Failed to parse Responses API response"
                })?;
            
            debug!("ModelHub Responses API request completed successfully");
            
            // Convert Responses API response back to OpenAI format
            Ok(self.convert_from_responses_api(responses_api_response))
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("ModelHub API request failed: {} - {}", status, error_text);
            anyhow::bail!("ModelHub API request failed: {} - {}", status, error_text);
        }
    }
    
    /// Convert OpenAI request to Responses API format
    fn convert_to_responses_api(&self, request: &OpenAIRequest, model_config: &ModelConfig) -> Result<ResponsesApiRequest> {
        // Convert messages to input format
        // Note: Responses API uses a different structure than chat completions
        // - User messages use role: "user" with content blocks
        // - Assistant messages use role: "assistant" with content blocks  
        // - Tool calls are separate "function_call" items
        // - Tool results are "function_call_output" items (NOT role: "tool")
        let mut input: Vec<Value> = Vec::new();
        let mut system_instructions: Option<String> = None;
        
        // First pass: collect all tool result call_ids
        // This is needed because Codex requires every function_call to have a matching function_call_output
        // But Claude Code may send incomplete tool call sequences (user can interrupt)
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
                        "output": output
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
                            // This handles the case where Claude Code sends incomplete tool call sequences
                            if tool_result_ids.contains(id) {
                                debug!("Adding function_call with call_id={}, name={:?}", id, tc.function.name);
                                input.push(serde_json::json!({
                                    "type": "function_call",
                                    "call_id": id,
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments.clone().unwrap_or_default()
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
                // (If there are tool calls, adding text here would break the function_call/function_call_output sequence)
                if !has_tool_calls {
                    if let Some(content) = &msg.content {
                        let text = content.extract_text();
                        if !text.is_empty() {
                            input.push(serde_json::json!({
                                "role": "assistant",
                                "content": [{ "type": "output_text", "text": text }]
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
                    "role": "user",
                    "content": content
                }));
            }
        }
        
        // Convert tools to Responses API format
        // OpenAI chat format: { type: "function", function: { name, description, parameters } }
        // Responses API format: { type: "function", name, description, parameters }
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
        // Take the max of request and config values to avoid Claude Code's low default (e.g., 1)
        let max_output_tokens = match (request.max_tokens, model_config.max_tokens) {
            (Some(req), Some(cfg)) => Some(req.max(cfg)),
            (Some(req), None) => Some(req.max(8192)), // default minimum for Codex
            (None, Some(cfg)) => Some(cfg),
            (None, None) => Some(8192),
        };
        debug!("📊 Responses API max_output_tokens: request={:?}, config={:?}, final={:?}",
               request.max_tokens, model_config.max_tokens, max_output_tokens);
        
        Ok(ResponsesApiRequest {
            model: model_config.name.clone(),
            input,
            max_output_tokens,
            temperature: request.temperature.or(model_config.temperature),
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
                    // Reasoning output doesn't contain text content, just internal reasoning
                    // We can safely ignore it as it's for debugging/transparency
                    debug!("Responses API: got reasoning output with {} summary items", 
                           output.summary.as_ref().map(|s| s.len()).unwrap_or(0));
                },
                other => {
                    debug!("Responses API: ignoring unknown output type: {}", other);
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
    
    async fn openai_responses_mode_stream(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        debug!("ModelHub: Using Responses API streaming mode");
        
        // Convert to Responses API format with stream=true
        let mut responses_request = self.convert_to_responses_api(&request, model_config)?;
        responses_request.stream = Some(true);
        
        let url = self.build_url(provider_config, "/responses");
        
        let builder = self.stream_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&responses_request);
        
        let response = self.add_modelhub_headers(builder, provider_config, request.session_id.as_deref())
            .send()
            .await
            .context("Failed to send streaming request")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("ModelHub API request failed: {} - {}", status, error_text);
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
                    // Handle different event types
                    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    
                    match event_type {
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
                            // Skip other event types (reasoning, etc.)
                        }
                    }
                }
            }
        }
        None
    }
    
    // ==================
    // Gemini Mode Methods
    // ==================
    // 
    // Gemini mode uses /v2/crawl endpoint with OpenAI chat format (NOT Gemini native format)
    // Reference: opencode/packages/opencode/src/provider/sdk/modelhub-gemini
    
    async fn chat_complete_gemini_mode(
        &self,
        mut request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse> {
        debug!("ModelHub: Using Gemini mode (OpenAI chat format to /v2/crawl)");
        
        // Log original max_tokens from request
        let original_max_tokens = request.max_tokens;
        
        // Update model name and apply defaults
        request.model = model_config.name.clone();
        
        // Use the maximum of request and config max_tokens to avoid too-small limits
        // Claude Code sometimes sends max_tokens=1 which causes immediate truncation
        request.max_tokens = match (request.max_tokens, model_config.max_tokens) {
            (Some(req), Some(cfg)) => Some(req.max(cfg)),
            (Some(req), None) => Some(req),
            (None, Some(cfg)) => Some(cfg),
            (None, None) => Some(8192), // Default fallback
        };
        
        if request.temperature.is_none() {
            request.temperature = model_config.temperature;
        }
        
        debug!("📊 max_tokens: original={:?}, config={:?}, final={:?}",
               original_max_tokens, model_config.max_tokens, request.max_tokens);
        
        // Sanitize tools if present (Gemini rejects some JSON Schema features)
        if let Some(ref mut tools) = request.tools {
            for tool in tools.iter_mut() {
                tool.function.parameters = sanitize_tool_schema(tool.function.parameters.take());
            }
        }
        
        // Inject cached thought_signatures into tool_calls
        inject_cached_thought_signatures(&mut request);
        
        let log_request = create_request_log_summary(&request);
        if let Ok(req_json) = serde_json::to_string_pretty(&log_request) {
            debug!("📤 Gemini Mode Request:\n{}", req_json);
        }
        
        let url = self.build_url(provider_config, "/v2/crawl");
        let session_id = request.session_id.clone();
        
        let builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request);
        
        let response = self.add_modelhub_headers(builder, provider_config, session_id.as_deref())
            .send()
            .await
            .context("Failed to send Gemini request")?;
        
        let status = response.status();
        
        if status.is_success() {
            // Get response as text first for debugging
            let response_text = response
                .text()
                .await
                .context("Failed to read Gemini response body")?;
            
            debug!("📥 Gemini Mode Raw Response:\n{}", &response_text);
            
            // Try to parse as OpenAI format
            let openai_response: OpenAIResponse = serde_json::from_str(&response_text)
                .with_context(|| {
                    error!("Failed to parse Gemini response. Raw response:\n{}", &response_text);
                    format!("Failed to parse Gemini response (OpenAI format). Response: {}", 
                            if response_text.len() > 500 { &response_text[..500] } else { &response_text })
                })?;
            
            // Debug: log tool_calls with thought_signature info and cache signatures
            if let Some(choice) = openai_response.choices.first() {
                if let Some(tool_calls) = &choice.message.tool_calls {
                    for tc in tool_calls {
                        // Try to extract thought_signature from multiple possible locations
                        let signature: Option<String> = tc.signature.clone()
                            .or_else(|| {
                                tc.extra_content.as_ref()
                                    .and_then(|ec| ec.get("google"))
                                    .and_then(|g| g.get("thought_signature"))
                                    .and_then(|ts| ts.as_str())
                                    .map(|s| s.to_string())
                            });
                        
                        debug!("🔧 Tool call: id={:?}, signature={:?}, extra_content={:?}",
                               tc.id, signature, tc.extra_content);
                        
                        // Cache the thought_signature if present
                        if let (Some(id), Some(sig)) = (&tc.id, &signature) {
                            cache_thought_signature(id, sig);
                        }
                    }
                }
            }
            
            debug!("ModelHub Gemini mode request completed successfully");
            Ok(openai_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("ModelHub Gemini API request failed: {} - {}", status, error_text);
            anyhow::bail!("ModelHub Gemini API request failed: {} - {}", status, error_text);
        }
    }
    
    async fn chat_stream_gemini_mode(
        &self,
        mut request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        debug!("ModelHub: Using Gemini streaming mode (OpenAI chat format to /v2/crawl)");
        
        // Log original max_tokens from request
        let original_max_tokens = request.max_tokens;
        
        // Update model name and apply defaults
        request.model = model_config.name.clone();
        request.stream = Some(true);
        
        // Use the maximum of request and config max_tokens to avoid too-small limits
        // Claude Code sometimes sends max_tokens=1 which causes immediate truncation
        request.max_tokens = match (request.max_tokens, model_config.max_tokens) {
            (Some(req), Some(cfg)) => Some(req.max(cfg)),
            (Some(req), None) => Some(req),
            (None, Some(cfg)) => Some(cfg),
            (None, None) => Some(8192), // Default fallback
        };
        
        if request.temperature.is_none() {
            request.temperature = model_config.temperature;
        }
        
        debug!("📊 max_tokens: original={:?}, config={:?}, final={:?}",
               original_max_tokens, model_config.max_tokens, request.max_tokens);
        
        // Sanitize tools if present (Gemini rejects some JSON Schema features)
        if let Some(ref mut tools) = request.tools {
            for tool in tools.iter_mut() {
                tool.function.parameters = sanitize_tool_schema(tool.function.parameters.take());
            }
        }
        
        // Inject cached thought_signatures into tool_calls
        inject_cached_thought_signatures(&mut request);
        
        let log_request = create_request_log_summary(&request);
        if let Ok(req_json) = serde_json::to_string_pretty(&log_request) {
            debug!("📤 Gemini Streaming Request:\n{}", req_json);
        }
        
        let url = self.build_url(provider_config, "/v2/crawl");
        let session_id = request.session_id.clone();
        
        let builder = self.stream_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request);
        
        let response = self.add_modelhub_headers(builder, provider_config, session_id.as_deref())
            .send()
            .await
            .context("Failed to send Gemini streaming request")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("ModelHub Gemini API request failed: {} - {}", status, error_text);
        }
        
        // Response is in OpenAI streaming format
        let stream = response
            .bytes_stream()
            .filter_map(move |chunk_result| {
                match chunk_result {
                    Ok(chunk) => {
                        match std::str::from_utf8(&chunk) {
                            Ok(chunk_str) => {
                                Self::parse_openai_sse(chunk_str)
                            }
                            Err(e) => Some(Err(anyhow::anyhow!("Invalid UTF-8: {}", e))),
                        }
                    }
                    Err(e) => Some(Err(anyhow::anyhow!("Stream error: {}", e))),
                }
            });
        
        Ok(Box::pin(stream))
    }
    
    /// Parse OpenAI SSE format (used by both Gemini mode streaming)
    fn parse_openai_sse(chunk_str: &str) -> Option<Result<OpenAIStreamResponse>> {
        for line in chunk_str.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    return None;
                }
                
                match serde_json::from_str::<OpenAIStreamResponse>(data) {
                    Ok(stream_response) => {
                        return Some(Ok(stream_response));
                    }
                    Err(e) => {
                        warn!("Failed to parse OpenAI streaming response: {}", e);
                    }
                }
            }
        }
        None
    }
    
    /// Convert OpenAI request to Gemini format
    fn convert_to_gemini_request(&self, openai_req: &OpenAIRequest, model_config: &ModelConfig) -> Result<GeminiRequest> {
        let mut contents = Vec::new();
        let mut system_instruction = None;
        
        for msg in &openai_req.messages {
            if msg.role == "system" {
                // Gemini uses system_instruction for system messages
                if let Some(content) = &msg.content {
                    system_instruction = Some(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart::Text { text: content.extract_text() }],
                    });
                }
            } else {
                let role = if msg.role == "assistant" { "model" } else { "user" };
                let mut parts = Vec::new();
                
                if let Some(content) = &msg.content {
                    match content {
                        OpenAIContent::Text(text) => {
                            parts.push(GeminiPart::Text { text: text.clone() });
                        }
                        OpenAIContent::Array(arr) => {
                            for part in arr {
                                match part {
                                    OpenAIContentPart::Text { text } => {
                                        parts.push(GeminiPart::Text { text: text.clone() });
                                    }
                                    OpenAIContentPart::ImageUrl { image_url } => {
                                        // Parse data URL
                                        if let Some((mime, data)) = parse_data_url(&image_url.url) {
                                            parts.push(GeminiPart::InlineData {
                                                inline_data: GeminiInlineData {
                                                    mime_type: mime,
                                                    data,
                                                },
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Handle tool calls from assistant
                if let Some(tool_calls) = &msg.tool_calls {
                    for tool_call in tool_calls {
                        if let (Some(name), Some(args)) = (&tool_call.function.name, &tool_call.function.arguments) {
                            let args_value: serde_json::Value = serde_json::from_str(args).unwrap_or(serde_json::json!({}));
                            parts.push(GeminiPart::FunctionCall {
                                function_call: GeminiFunctionCall {
                                    name: name.clone(),
                                    args: args_value,
                                },
                            });
                        }
                    }
                }
                
                // Handle tool results (role=tool in OpenAI)
                if msg.role == "tool" {
                    if let (Some(tool_call_id), Some(content)) = (&msg.tool_call_id, &msg.content) {
                        parts.push(GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: tool_call_id.clone(),
                                response: serde_json::json!({ "result": content.extract_text() }),
                            },
                        });
                    }
                }
                
                if !parts.is_empty() {
                    contents.push(GeminiContent {
                        role: role.to_string(),
                        parts,
                    });
                }
            }
        }
        
        // Convert tools to Gemini format (with sanitization)
        let tools = openai_req.tools.as_ref().map(|openai_tools| {
            vec![GeminiTool {
                function_declarations: openai_tools
                    .iter()
                    .map(|t| {
                        GeminiFunctionDeclaration {
                            name: t.function.name.clone(),
                            description: t.function.description.clone().unwrap_or_default(),
                            parameters: sanitize_tool_schema(t.function.parameters.clone()),
                        }
                    })
                    .collect(),
            }]
        });
        
        // Build generation config
        let generation_config = GeminiGenerationConfig {
            temperature: openai_req.temperature,
            top_p: openai_req.top_p,
            max_output_tokens: openai_req.max_tokens.or(model_config.max_tokens),
            stop_sequences: openai_req.stop.clone(),
        };
        
        Ok(GeminiRequest {
            model: model_config.name.clone(),
            contents,
            system_instruction,
            tools,
            generation_config: Some(generation_config),
            stream: openai_req.stream,
        })
    }
    
    /// Convert Gemini response to OpenAI format
    fn convert_from_gemini_response(&self, gemini_resp: GeminiResponse, model: &str) -> Result<OpenAIResponse> {
        let mut content_text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = "stop".to_string();
        
        if let Some(candidates) = gemini_resp.candidates {
            if let Some(candidate) = candidates.first() {
                if let Some(content) = &candidate.content {
                    for part in &content.parts {
                        match part {
                            GeminiPart::Text { text } => {
                                content_text.push_str(text);
                            }
                            GeminiPart::FunctionCall { function_call } => {
                                tool_calls.push(OpenAIToolCall {
                                    id: Some(format!("call_{}", uuid::Uuid::new_v4().simple())),
                                    tool_type: Some("function".to_string()),
                                    function: OpenAIFunctionCall {
                                        name: Some(function_call.name.clone()),
                                        arguments: Some(function_call.args.to_string()),
                                    },
                                    signature: None, // TODO: extract from Gemini response if present
                                    extra_content: None,
                                });
                                finish_reason = "tool_calls".to_string();
                            }
                            _ => {}
                        }
                    }
                }
                
                // Handle thought_signature for multi-turn tool use
                if let Some(thought_sig) = &candidate.thought_signature {
                    // Inject thought_signature into tool call arguments for preservation
                    for tc in &mut tool_calls {
                        if let Some(args) = &tc.function.arguments {
                            if let Ok(mut args_value) = serde_json::from_str::<serde_json::Value>(args) {
                                args_value["_thought_signature"] = serde_json::Value::String(thought_sig.clone());
                                tc.function.arguments = Some(args_value.to_string());
                            }
                        }
                    }
                }
                
                if let Some(fr) = &candidate.finish_reason {
                    finish_reason = match fr.as_str() {
                        "STOP" => "stop".to_string(),
                        "MAX_TOKENS" => "length".to_string(),
                        "SAFETY" => "content_filter".to_string(),
                        "RECITATION" => "content_filter".to_string(),
                        _ => "stop".to_string(),
                    };
                }
            }
        }
        
        // Build usage from metadata
        let (prompt_tokens, completion_tokens) = gemini_resp.usage_metadata
            .map(|u| (u.prompt_token_count.unwrap_or(0), u.candidates_token_count.unwrap_or(0)))
            .unwrap_or((0, 0));
        
        Ok(OpenAIResponse {
            id: format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: model.to_string(),
            choices: vec![OpenAIChoice {
                index: 0,
                message: OpenAIMessage {
                    role: "assistant".to_string(),
                    content: if content_text.is_empty() { None } else { Some(OpenAIContent::Text(content_text)) },
                    name: None,
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    tool_call_id: None,
                },
                logprobs: None,
                finish_reason: Some(finish_reason),
            }],
            usage: Some(OpenAIUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }),
            system_fingerprint: None,
        })
    }
    
    /// Convert Gemini streaming chunk to OpenAI streaming format
    fn convert_gemini_stream_chunk(gemini_chunk: GeminiStreamResponse, model: &str) -> Option<OpenAIStreamResponse> {
        let mut content = None;
        let mut tool_calls = None;
        let mut finish_reason = None;
        
        if let Some(candidates) = gemini_chunk.candidates {
            if let Some(candidate) = candidates.first() {
                if let Some(c) = &candidate.content {
                    for part in &c.parts {
                        match part {
                            GeminiPart::Text { text } => {
                                content = Some(text.clone());
                            }
                            GeminiPart::FunctionCall { function_call } => {
                                tool_calls = Some(vec![OpenAIToolCall {
                                    id: Some(format!("call_{}", uuid::Uuid::new_v4().simple())),
                                    tool_type: Some("function".to_string()),
                                    function: OpenAIFunctionCall {
                                        name: Some(function_call.name.clone()),
                                        arguments: Some(function_call.args.to_string()),
                                    },
                                    signature: None, // TODO: extract from Gemini response if present
                                    extra_content: None,
                                }]);
                                finish_reason = Some("tool_calls".to_string());
                            }
                            _ => {}
                        }
                    }
                }
                
                if let Some(fr) = &candidate.finish_reason {
                    finish_reason = Some(match fr.as_str() {
                        "STOP" => "stop".to_string(),
                        "MAX_TOKENS" => "length".to_string(),
                        "SAFETY" => "content_filter".to_string(),
                        _ => "stop".to_string(),
                    });
                }
            }
        }
        
        // Skip empty chunks
        if content.is_none() && tool_calls.is_none() && finish_reason.is_none() {
            return None;
        }
        
        Some(OpenAIStreamResponse {
            id: format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: model.to_string(),
            system_fingerprint: None,
            choices: vec![OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: None,
                    content,
                    tool_calls,
                },
                logprobs: None,
                finish_reason,
            }],
        })
    }
}

#[async_trait]
impl Provider for ModelHubProvider {
    fn name(&self) -> &str {
        "modelhub"
    }
    
    async fn chat_complete(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse> {
        match self.get_mode(provider_config) {
            "gemini" => self.chat_complete_gemini_mode(request, provider_config, model_config).await,
            _ => self.openai_responses_mode(request, provider_config, model_config).await,
        }
    }
    
    async fn chat_stream(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        match self.get_mode(provider_config) {
            "gemini" => self.chat_stream_gemini_mode(request, provider_config, model_config).await,
            _ => self.openai_responses_mode_stream(request, provider_config, model_config).await,
        }
    }
}

impl Default for ModelHubProvider {
    fn default() -> Self {
        Self::new().expect("Failed to create default ModelHub provider")
    }
}

// ====================
// Gemini Data Types
// ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiRequest {
    pub model: String,
    pub contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    pub generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "topP")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxOutputTokens")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stopSequences")]
    pub stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiResponse {
    pub candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiStreamResponse {
    pub candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiCandidate {
    pub content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
    #[serde(rename = "thoughtSignature")]
    pub thought_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    pub prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    pub candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    pub total_token_count: Option<u32>,
}

// ====================
// Helper Functions
// ====================

/// Parse a data URL into mime type and base64 data
fn parse_data_url(url: &str) -> Option<(String, String)> {
    if !url.starts_with("data:") {
        return None;
    }
    
    let rest = url.strip_prefix("data:")?;
    let parts: Vec<&str> = rest.splitn(2, ',').collect();
    if parts.len() != 2 {
        return None;
    }
    
    let mime_and_encoding = parts[0];
    let data = parts[1];
    
    // Parse mime type (before ;base64)
    let mime_type = mime_and_encoding
        .split(';')
        .next()
        .unwrap_or("application/octet-stream")
        .to_string();
    
    Some((mime_type, data.to_string()))
}

/// Sanitize tool schema for Gemini compatibility
/// Removes unsupported JSON Schema features like anyOf, allOf, oneOf
pub fn sanitize_tool_schema(schema: Option<serde_json::Value>) -> Option<serde_json::Value> {
    schema.map(|s| sanitize_schema_value(s))
}

fn sanitize_schema_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(mut map) => {
            // Remove unsupported schema keywords that Gemini rejects
            // Reference: https://ai.google.dev/gemini-api/docs/function-calling
            
            // JSON Schema meta keywords
            map.remove("$schema");
            map.remove("$id");
            map.remove("$ref");
            map.remove("$defs");
            map.remove("definitions");
            map.remove("$comment");
            
            // Composition keywords (Gemini doesn't support these)
            map.remove("anyOf");
            map.remove("allOf");
            map.remove("oneOf");
            map.remove("not");
            map.remove("if");
            map.remove("then");
            map.remove("else");
            
            // Numeric validation keywords not supported by Gemini
            map.remove("exclusiveMinimum");
            map.remove("exclusiveMaximum");
            map.remove("multipleOf");
            
            // Object validation keywords not supported by Gemini
            map.remove("propertyNames");
            map.remove("patternProperties");
            map.remove("unevaluatedProperties");
            map.remove("dependentSchemas");
            map.remove("dependentRequired");
            map.remove("minProperties");
            map.remove("maxProperties");
            
            // Array validation keywords not supported by Gemini
            map.remove("contains");
            map.remove("minContains");
            map.remove("maxContains");
            map.remove("unevaluatedItems");
            map.remove("prefixItems");
            map.remove("uniqueItems");
            
            // String validation keywords that may not be supported
            map.remove("contentEncoding");
            map.remove("contentMediaType");
            map.remove("contentSchema");
            
            // Other keywords
            map.remove("const");
            map.remove("deprecated");
            map.remove("readOnly");
            map.remove("writeOnly");
            map.remove("examples");
            map.remove("default");
            
            // Recursively sanitize nested objects
            let sanitized: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| (k, sanitize_schema_value(v)))
                .collect();
            
            serde_json::Value::Object(sanitized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(sanitize_schema_value).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderOptions;
    
    #[test]
    fn test_provider_creation() {
        let provider = ModelHubProvider::new();
        assert!(provider.is_ok());
    }
    
    #[test]
    fn test_provider_name() {
        let provider = ModelHubProvider::new().unwrap();
        assert_eq!(provider.name(), "modelhub");
    }
    
    #[test]
    fn test_build_url_with_ak_param() {
        let provider = ModelHubProvider::new().unwrap();
        
        let mut config = ProviderConfig {
            provider_type: "modelhub".to_string(),
            base_url: "https://modelhub.example.com".to_string(),
            api_key: "test-api-key".to_string(),
            options: ProviderOptions {
                api_key_param: Some("ak".to_string()),
                mode: Some("responses".to_string()),
                headers: Default::default(),
            },
            models: Default::default(),
        };
        
        let url = provider.build_url(&config, "/chat/completions");
        assert_eq!(url, "https://modelhub.example.com/chat/completions?ak=test-api-key");
        
        // Test without api key param
        config.options.api_key_param = None;
        let url2 = provider.build_url(&config, "/chat/completions");
        assert_eq!(url2, "https://modelhub.example.com/chat/completions");
    }
    
    #[test]
    fn test_sanitize_tool_schema() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "propertyNames": {"pattern": "^[a-z]+$"}
                },
                "value": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "number"}
                    ]
                },
                "count": {
                    "type": "integer",
                    "exclusiveMinimum": 0,
                    "exclusiveMaximum": 100
                }
            },
            "allOf": [
                {"required": ["name"]}
            ]
        });
        
        let sanitized = sanitize_tool_schema(Some(schema)).unwrap();
        
        // Check top-level unsupported fields are removed
        assert!(sanitized.get("$schema").is_none());
        assert!(sanitized.get("anyOf").is_none());
        assert!(sanitized.get("allOf").is_none());
        assert!(sanitized.get("properties").is_some());
        
        // Check nested fields are also removed
        let props = sanitized.get("properties").unwrap();
        let value_prop = props.get("value").unwrap();
        assert!(value_prop.get("anyOf").is_none());
        
        let name_prop = props.get("name").unwrap();
        assert!(name_prop.get("propertyNames").is_none());
        
        let count_prop = props.get("count").unwrap();
        assert!(count_prop.get("exclusiveMinimum").is_none());
        assert!(count_prop.get("exclusiveMaximum").is_none());
        // But type should still be there
        assert_eq!(count_prop.get("type").unwrap(), "integer");
    }
    
    #[test]
    fn test_parse_data_url() {
        let url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let result = parse_data_url(url);
        assert!(result.is_some());
        let (mime, data) = result.unwrap();
        assert_eq!(mime, "image/png");
        assert!(data.starts_with("iVBORw"));
        
        // Test invalid URL
        let invalid = "https://example.com/image.png";
        assert!(parse_data_url(invalid).is_none());
    }
    
    #[test]
    fn test_get_mode() {
        let provider = ModelHubProvider::new().unwrap();
        
        let mut config = ProviderConfig {
            provider_type: "modelhub".to_string(),
            base_url: "https://example.com".to_string(),
            api_key: "".to_string(),
            options: ProviderOptions {
                api_key_param: None,
                mode: Some("gemini".to_string()),
                headers: Default::default(),
            },
            models: Default::default(),
        };
        
        assert_eq!(provider.get_mode(&config), "gemini");
        
        config.options.mode = Some("responses".to_string());
        assert_eq!(provider.get_mode(&config), "responses");
        
        config.options.mode = None;
        assert_eq!(provider.get_mode(&config), "responses"); // Default
    }
}
