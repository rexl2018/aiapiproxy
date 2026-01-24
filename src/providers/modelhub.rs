//! ModelHub Provider implementation
//!
//! Supports both OpenAI-compatible (responses) mode and Gemini mode

use super::{BoxStream, Provider};
use crate::config::{ModelConfig, ProviderConfig};
use crate::models::openai::*;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{debug, error, warn};

// ====== Responses API Structures ======

/// OpenAI Responses API Request format
#[derive(Debug, Serialize)]
struct ResponsesApiRequest {
    model: String,
    input: Vec<ResponsesInputMessage>,
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
    model: String,
    output: Vec<ResponsesOutput>,
    #[serde(default)]
    usage: Option<ResponsesUsage>,
    status: String,
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
    fn add_modelhub_headers(&self, builder: reqwest::RequestBuilder, provider_config: &ProviderConfig) -> reqwest::RequestBuilder {
        let mut builder = builder
            .header("HTTP-Referer", "https://aiapiproxy.local")
            .header("X-Title", "AIAPIProxy");
        
        // Add custom headers from config
        for (key, value) in &provider_config.options.headers {
            builder = builder.header(key, value);
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
        
        if let Ok(req_json) = serde_json::to_string_pretty(&responses_request) {
            debug!("📤 Responses API Request:\n{}", req_json);
        }
        
        let url = self.build_url(provider_config, "/responses");
        
        let builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&responses_request);
        
        let response = self.add_modelhub_headers(builder, provider_config)
            .send()
            .await
            .context("Failed to send request")?;
        
        let status = response.status();
        
        if status.is_success() {
            let responses_api_response: ResponsesApiResponse = response
                .json()
                .await
                .context("Failed to parse Responses API response")?;
            
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
        let mut input: Vec<ResponsesInputMessage> = Vec::new();
        let mut system_instructions: Option<String> = None;
        
        for msg in &request.messages {
            let role = msg.role.clone();
            
            // Extract system message as instructions
            if role == "system" {
                if let Some(content) = &msg.content {
                    system_instructions = Some(content.extract_text());
                }
                continue;
            }
            
            // Convert content to Value
            let content = if let Some(c) = &msg.content {
                match c {
                    OpenAIContent::Text(text) => Value::String(text.clone()),
                    OpenAIContent::Array(parts) => {
                        let json_parts: Vec<Value> = parts.iter().map(|p| {
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
                        }).collect();
                        Value::Array(json_parts)
                    }
                }
            } else {
                Value::String(String::new())
            };
            
            input.push(ResponsesInputMessage { role, content });
        }
        
        // Convert tools if present
        let tools = request.tools.as_ref().map(|t| {
            t.iter().map(|tool| serde_json::to_value(tool).unwrap_or_default()).collect()
        });
        
        // Ensure max_output_tokens >= 16
        let max_output_tokens = request.max_tokens
            .or(model_config.max_tokens)
            .map(|t| if t < 16 { 16 } else { t });
        
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
                        });
                    }
                },
                _ => {}
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
        }).unwrap_or(OpenAIUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });
        
        OpenAIResponse {
            id: response.id,
            object: "chat.completion".to_string(),
            created: 0,
            model: response.model,
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
        
        let response = self.add_modelhub_headers(builder, provider_config)
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
        
        // Update model name and apply defaults
        request.model = model_config.name.clone();
        if request.max_tokens.is_none() {
            request.max_tokens = model_config.max_tokens;
        }
        if request.temperature.is_none() {
            request.temperature = model_config.temperature;
        }
        
        // Sanitize tools if present (Gemini rejects some JSON Schema features)
        if let Some(ref mut tools) = request.tools {
            for tool in tools.iter_mut() {
                tool.function.parameters = sanitize_tool_schema(tool.function.parameters.take());
            }
        }
        
        if let Ok(req_json) = serde_json::to_string_pretty(&request) {
            debug!("📤 Gemini Mode Request (OpenAI format):\n{}", req_json);
        }
        
        let url = self.build_url(provider_config, "/v2/crawl");
        
        let builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request);
        
        let response = self.add_modelhub_headers(builder, provider_config)
            .send()
            .await
            .context("Failed to send Gemini request")?;
        
        let status = response.status();
        
        if status.is_success() {
            // Response is in OpenAI chat format
            let openai_response: OpenAIResponse = response
                .json()
                .await
                .context("Failed to parse Gemini response (OpenAI format)")?;
            
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
        
        // Update model name and apply defaults
        request.model = model_config.name.clone();
        request.stream = Some(true);
        if request.max_tokens.is_none() {
            request.max_tokens = model_config.max_tokens;
        }
        if request.temperature.is_none() {
            request.temperature = model_config.temperature;
        }
        
        // Sanitize tools if present (Gemini rejects some JSON Schema features)
        if let Some(ref mut tools) = request.tools {
            for tool in tools.iter_mut() {
                tool.function.parameters = sanitize_tool_schema(tool.function.parameters.take());
            }
        }
        
        let url = self.build_url(provider_config, "/v2/crawl");
        
        let builder = self.stream_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request);
        
        let response = self.add_modelhub_headers(builder, provider_config)
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
            usage: OpenAIUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
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
            // Remove unsupported schema keywords
            map.remove("anyOf");
            map.remove("allOf");
            map.remove("oneOf");
            map.remove("$ref");
            map.remove("$defs");
            map.remove("definitions");
            
            // If we have anyOf/allOf/oneOf, try to use the first alternative
            // This is handled by removing them above
            
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
            "type": "object",
            "properties": {
                "name": {
                    "type": "string"
                },
                "value": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "number"}
                    ]
                }
            },
            "allOf": [
                {"required": ["name"]}
            ]
        });
        
        let sanitized = sanitize_tool_schema(Some(schema)).unwrap();
        
        assert!(sanitized.get("anyOf").is_none());
        assert!(sanitized.get("allOf").is_none());
        assert!(sanitized.get("properties").is_some());
        
        // Check nested anyOf is also removed
        let props = sanitized.get("properties").unwrap();
        let value_prop = props.get("value").unwrap();
        assert!(value_prop.get("anyOf").is_none());
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
