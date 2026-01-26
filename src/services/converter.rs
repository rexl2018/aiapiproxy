//! API converter service
//! 
//! Responsible for converting between Claude API and OpenAI API formats

use crate::config::Settings;
use crate::models::{
    claude::*, openai::*,
};
use crate::utils::thought_cache::cache_thought_signature;
use anyhow::{Context, Result};
use tracing::{debug, warn};
use uuid::Uuid;

/// API converter
#[derive(Debug, Clone)]
pub struct ApiConverter {
    settings: Settings,
}

impl ApiConverter {
    /// Create a new converter instance
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }
    
    /// Convert Claude request to OpenAI request
    /// Implements the conversion logic as specified in the conversion guide
    pub fn convert_request(&self, claude_req: ClaudeRequest) -> Result<OpenAIRequest> {
        debug!("Starting conversion from Claude request to OpenAI format");
        
        // Map model name according to conversion guide
        let openai_model = self.settings
            .get_openai_model(&claude_req.model)
            .context("Unable to map Claude model to OpenAI model")?;
        
        // Convert messages
        let mut openai_messages = Vec::new();
        
        // Handle system prompt conversion as per guide
        if let Some(system) = claude_req.system {
            let system_text = match system {
                SystemPrompt::String(text) => text,
                SystemPrompt::Array(blocks) => {
                    // Merge array format into single string as per guide
                    blocks.iter()
                        .map(|block| match block {
                            ClaudeContentBlock::Text { text } => text.clone(),
                            _ => String::new(), // Skip non-text blocks in system prompt
                        })
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            };
            
            openai_messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(OpenAIContent::Text(system_text)),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        
        // Convert Claude messages to OpenAI messages
        // This may expand one Claude message into multiple OpenAI messages (e.g., for tool results)
        for claude_msg in claude_req.messages {
            let converted_msgs = self.convert_claude_message_to_openai_messages(claude_msg)?;
            openai_messages.extend(converted_msgs);
        }
        
        // Extract user ID from metadata if available (参考claude-code-proxy项目的做法)
        let user_id = claude_req.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("user_id"))
            .and_then(|user_id| user_id.as_str())
            .map(|s| s.to_string());
        
        // Extract session_id from user_id
        // Format: user_{hash}_account__session_{session-uuid}
        let session_id = user_id.as_ref().and_then(|uid| {
            uid.split("_session_")
                .nth(1)
                .map(|s| s.to_string())
        });
        
        // 🔍 DEBUG: 记录metadata处理信息
        if let Some(metadata) = &claude_req.metadata {
            debug!("Processing metadata: {:?}", metadata);
            if let Some(ref uid) = user_id {
                debug!("Mapped user_id from metadata to OpenAI user field: {}", uid);
            }
            if let Some(ref sid) = session_id {
                debug!("Extracted session_id for ModelHub: {}", sid);
            }
        }
        
        // Convert tools if present - Claude to OpenAI format conversion
        let openai_tools: Option<Vec<OpenAITool>> = claude_req.tools.as_ref().map(|claude_tools| {
            claude_tools.iter().map(|claude_tool| {
                OpenAITool {
                    tool_type: "function".to_string(),
                    function: OpenAIFunction {
                        name: claude_tool.name.clone(),
                        description: claude_tool.description.clone(),
                        parameters: Some(claude_tool.input_schema.clone()),
                    },
                }
            }).collect()
        });
        
        debug!("Converted {} Claude tools to OpenAI format", 
               openai_tools.as_ref().map(|t| t.len()).unwrap_or(0));
        
        // Ensure max_tokens is set (required by Anthropic API as per guide)
        let max_tokens = if claude_req.max_tokens == 0 {
            4096 // Default value as per conversion guide
        } else {
            claude_req.max_tokens
        };
        
        // Build OpenAI request according to conversion guide
        let openai_req = OpenAIRequest {
            model: openai_model,
            messages: openai_messages,
            max_tokens: Some(max_tokens),
            temperature: claude_req.temperature,
            top_p: claude_req.top_p,
            stop: claude_req.stop_sequences,
            stream: claude_req.stream,
            n: Some(1), // Claude always returns a single response
            presence_penalty: None,
            frequency_penalty: None,
            logit_bias: None,
            user: user_id, // Map metadata user_id to OpenAI user field
            response_format: None,
            seed: None,
            tools: openai_tools,
            tool_choice: claude_req.tool_choice.clone(),
            session_id, // For ModelHub server-side caching
        };
        
        debug!("Claude request conversion completed");
        Ok(openai_req)
    }
    
    /// Convert OpenAI response to Claude response
    /// Implements the conversion logic as specified in the conversion guide
    pub fn convert_response(&self, openai_resp: OpenAIResponse, original_model: &str) -> Result<ClaudeResponse> {
        debug!("Starting conversion from OpenAI response to Claude format");
        
        if openai_resp.choices.is_empty() {
            anyhow::bail!("No choices in OpenAI response");
        }
        
        let choice = &openai_resp.choices[0];
        let message = &choice.message;
        
        // Build Claude content blocks according to conversion guide
        let mut content_blocks = Vec::new();
        
        // Add text content if present
        if let Some(content) = &message.content {
            let content_text = content.extract_text();
            if !content_text.is_empty() {
                content_blocks.push(ClaudeContentBlock::Text { text: content_text });
            }
        }
        
        // Convert OpenAI tool_calls to Claude ToolUse blocks
        if let Some(tool_calls) = &message.tool_calls {
            for tool_call in tool_calls {
                if tool_call.tool_type.as_deref() == Some("function") {
                    // Safe parsing of tool arguments as per conversion guide
                    let _id = tool_call.id.as_deref().unwrap_or("unknown_id");
                    let name = tool_call.function.name.as_deref().unwrap_or("unknown_function");
                    let arguments = tool_call.function.arguments.as_deref().unwrap_or("{}");
                    
                    // Parse tool arguments safely (handles empty strings)
                    let input = self.safe_parse_tool_arguments(arguments);
                    
                    // Extract thought_signature from tool_call if present (for Gemini thinking models)
                    let thought_signature = tool_call.signature.clone()
                        .or_else(|| {
                            tool_call.extra_content.as_ref()
                                .and_then(|ec| ec.get("google"))
                                .and_then(|g| g.get("thought_signature"))
                                .and_then(|ts| ts.as_str())
                                .map(|s| s.to_string())
                        });
                    
                    // Use provided ID if non-empty, otherwise generate one
                    let tool_id = tool_call.id.as_ref()
                        .filter(|id| !id.is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("toolu_{}", self.generate_id()));
                    
                    // Cache thought_signature if present for use in subsequent requests
                    if let Some(ref sig) = thought_signature {
                        cache_thought_signature(&tool_id, sig);
                    }
                    
                    content_blocks.push(ClaudeContentBlock::ToolUse {
                        id: tool_id,
                        name: name.to_string(),
                        input,
                        thought_signature,
                    });
                }
            }
            
            debug!("Converted {} OpenAI tool_calls to Claude ToolUse blocks", tool_calls.len());
        }
        
        // Map finish reason to stop reason as per conversion guide
        let stop_reason = self.map_finish_reason_to_stop_reason(choice.finish_reason.as_deref());
        
        // Extract usage info with defaults if not provided
        let (input_tokens, output_tokens) = match &openai_resp.usage {
            Some(usage) => (usage.prompt_tokens, usage.completion_tokens),
            None => (0, 0), // Default to 0 if usage not provided
        };
        
        debug!("Converted OpenAI response: model={}, tokens={}+{}, stop_reason={}", 
               original_model, input_tokens, output_tokens, &stop_reason);
        
        // Build Claude response according to conversion guide format
        let claude_resp = ClaudeResponse {
            id: format!("msg_{}", self.generate_id()),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            content: content_blocks,
            model: original_model.to_string(),
            stop_reason: Some(stop_reason),
            stop_sequence: None,
            usage: ClaudeUsage {
                input_tokens,
                output_tokens,
            },
        };
        
        debug!("OpenAI response conversion completed");
        Ok(claude_resp)
    }
    
    /// Convert OpenAI stream response to Claude stream events
    /// Implements complete streaming conversion as per conversion guide
    pub fn convert_stream_chunk(
        &self, 
        openai_chunk: OpenAIStreamResponse, 
        original_model: &str
    ) -> Result<Vec<ClaudeStreamEvent>> {
        debug!("Converting OpenAI stream response chunk");
        
        let mut events = Vec::new();
        
        if openai_chunk.choices.is_empty() {
            return Ok(events);
        }
        
        let choice = &openai_chunk.choices[0];
        let delta = &choice.delta;
        
        // Generate message_start event for first chunk (contains role)
        if delta.role.is_some() {
            events.push(ClaudeStreamEvent::MessageStart {
                message: ClaudeStreamMessage {
                    id: format!("msg_{}", self.generate_id()),
                    message_type: "message".to_string(),
                    role: "assistant".to_string(),
                    content: vec![],
                    model: original_model.to_string(),
                    stop_reason: None,
                    stop_sequence: None,
                    usage: ClaudeUsage {
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                },
            });
            
            // Content block start event for text
            events.push(ClaudeStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ClaudeContentBlock::Text { text: String::new() },
            });
        }
        
        // Handle content delta events
        if let Some(content) = &delta.content {
            if !content.is_empty() {
                events.push(ClaudeStreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ClaudeContentDelta::TextDelta {
                        text: content.clone(),
                    },
                });
            }
        }
        
        // Handle tool calls in streaming (as per conversion guide)
        if let Some(tool_calls) = &delta.tool_calls {
            for (i, tool_call) in tool_calls.iter().enumerate() {
                let function = &tool_call.function;
                
                if let Some(name) = &function.name {
                    // Extract thought_signature if present
                    let thought_signature = tool_call.signature.clone()
                        .or_else(|| {
                            tool_call.extra_content.as_ref()
                                .and_then(|ec| ec.get("google"))
                                .and_then(|g| g.get("thought_signature"))
                                .and_then(|ts| ts.as_str())
                                .map(|s| s.to_string())
                        });
                    
                    // Use provided ID if non-empty, otherwise generate one
                    let tool_id = tool_call.id.as_ref()
                        .filter(|id| !id.is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("toolu_{}", self.generate_id()));
                    
                    // Cache thought_signature if present for use in subsequent requests
                    if let Some(ref sig) = thought_signature {
                        cache_thought_signature(&tool_id, sig);
                    }
                    
                    // Tool use content block start
                    events.push(ClaudeStreamEvent::ContentBlockStart {
                        index: (i + 1) as u32,
                        content_block: ClaudeContentBlock::ToolUse {
                            id: tool_id,
                            name: name.clone(),
                            input: serde_json::json!({}),
                            thought_signature,
                        },
                    });
                }
                
                if let Some(arguments) = &function.arguments {
                    // Tool input delta (partial JSON)
                    events.push(ClaudeStreamEvent::ContentBlockDelta {
                        index: (i + 1) as u32,
                        delta: ClaudeContentDelta::TextDelta {
                            text: arguments.clone(),
                        },
                    });
                }
            }
        }
        
        // Handle completion events
        if let Some(finish_reason) = &choice.finish_reason {
            // Content block stop events
            events.push(ClaudeStreamEvent::ContentBlockStop { index: 0 });
            
            // Stop tool use blocks if any
            if let Some(tool_calls) = &delta.tool_calls {
                for i in 0..tool_calls.len() {
                    events.push(ClaudeStreamEvent::ContentBlockStop { 
                        index: (i + 1) as u32 
                    });
                }
            }
            
            // Message delta with stop reason
            let stop_reason = self.map_finish_reason_to_stop_reason(Some(finish_reason));
            events.push(ClaudeStreamEvent::MessageDelta {
                delta: ClaudeMessageDelta {
                    stop_reason: Some(stop_reason),
                    stop_sequence: None,
                },
                usage: ClaudeUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            });
            
            // Final message stop event
            events.push(ClaudeStreamEvent::MessageStop);
        }
        
        Ok(events)
    }
    
    /// Convert OpenAI error to Claude error
    /// Maps all provider errors to OpenAI-compatible format as per guide
    pub fn convert_error(&self, openai_error: OpenAIError) -> ClaudeErrorResponse {
        debug!("Converting OpenAI error to Claude format");
        
        let claude_error_type = self.map_openai_error_type(&openai_error.error_type);
        
        ClaudeErrorResponse {
            error_type: "error".to_string(),
            error: ClaudeError {
                error_type: claude_error_type,
                message: openai_error.message,
            },
        }
    }
    
    /// Convert Anthropic error to OpenAI-compatible error
    /// Implements error mapping as specified in conversion guide
    pub fn convert_anthropic_error(&self, anthropic_error: &str, error_type: &str) -> OpenAIError {
        let (openai_type, openai_message) = match error_type {
            "invalid_request_error" => ("invalid_request_error", anthropic_error),
            "authentication_error" => ("authentication_error", anthropic_error),
            "permission_error" => ("permission_error", anthropic_error),
            "not_found_error" => ("not_found_error", anthropic_error),
            "rate_limit_error" => ("rate_limit_error", "Rate limit exceeded. Please try again later."),
            "api_error" => ("api_error", anthropic_error),
            "overloaded_error" => ("service_unavailable", "Service temporarily overloaded. Please try again later."),
            _ => ("api_error", anthropic_error),
        };
        
        OpenAIError {
             error_type: openai_type.to_string(),
             message: openai_message.to_string(),
             param: None,
             code: None,
         }
    }
    
    /// Convert Claude message to OpenAI messages
    /// May return multiple messages (e.g., tool results become separate "tool" role messages)
    fn convert_claude_message_to_openai_messages(&self, claude_msg: ClaudeMessage) -> Result<Vec<OpenAIMessage>> {
        let mut messages = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_results = Vec::new();
        
        let content = match claude_msg.content {
            ClaudeContent::Text(text) => Some(OpenAIContent::Text(text)),
            ClaudeContent::Other(v) => {
                // Handle null or unexpected content types
                if v.is_null() {
                    None
                } else {
                    warn!("Unexpected content type in Claude message: {:?}", v);
                    None
                }
            }
            ClaudeContent::Blocks(blocks) => {
                let mut openai_parts = Vec::new();
                
                for block in blocks {
                    match block {
                        ClaudeContentBlock::Text { text } => {
                            openai_parts.push(OpenAIContentPart::Text { text });
                        }
                        ClaudeContentBlock::Image { source } => {
                            // Convert Claude image format to OpenAI format
                            let image_url = if source.source_type == "base64" {
                                format!("data:{};base64,{}", source.media_type, source.data)
                            } else {
                                warn!("Unsupported image source type: {}", source.source_type);
                                continue;
                            };
                            
                            openai_parts.push(OpenAIContentPart::ImageUrl {
                                image_url: OpenAIImageUrl {
                                    url: image_url,
                                    detail: Some("auto".to_string()),
                                },
                            });
                        }
                        ClaudeContentBlock::ToolUse { id, name, input, thought_signature } => {
                            // Convert Claude ToolUse to OpenAI tool call format
                            // Use the original Claude tool_use id for proper matching
                            // Include thought_signature for Gemini thinking models
                            let extra_content = thought_signature.as_ref().map(|sig| {
                                serde_json::json!({
                                    "google": {
                                        "thought_signature": sig
                                    }
                                })
                            });
                            
                            tool_calls.push(OpenAIToolCall {
                                id: Some(id),
                                tool_type: Some("function".to_string()),
                                function: OpenAIFunctionCall {
                                    name: Some(name),
                                    arguments: Some(input.to_string()),
                                },
                                signature: thought_signature,
                                extra_content,
                            });
                        }
                        ClaudeContentBlock::ToolResult { tool_use_id, content, is_error } => {
                            // Collect tool results to be sent as separate "tool" role messages
                            tool_results.push((tool_use_id, content, is_error));
                        }
                        ClaudeContentBlock::Unknown => {
                            // Skip unknown block types
                            warn!("Skipping unknown content block type in message conversion");
                        }
                    }
                }
                
                if openai_parts.is_empty() {
                    None
                } else {
                    Some(OpenAIContent::Array(openai_parts))
                }
            }
        };
        
        // If this message has tool results, create separate "tool" role messages for each
        if !tool_results.is_empty() {
            for (tool_call_id, result_content, _is_error) in tool_results {
                messages.push(OpenAIMessage {
                    role: "tool".to_string(),
                    content: Some(OpenAIContent::Text(result_content)),
                    name: None,
                    tool_calls: None,
                    tool_call_id: Some(tool_call_id),
                });
            }
            return Ok(messages);
        }
        
        // Regular message (possibly with tool calls)
        let openai_tool_calls = if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        };
        
        messages.push(OpenAIMessage {
            role: claude_msg.role,
            content,
            name: None,
            tool_calls: openai_tool_calls,
            tool_call_id: None,
        });
        
        Ok(messages)
    }
    
    /// Map OpenAI finish_reason to Claude stop_reason
    fn map_finish_reason_to_stop_reason(&self, finish_reason: Option<&str>) -> String {
        match finish_reason {
            Some("stop") => "end_turn".to_string(),
            Some("length") => "max_tokens".to_string(),
            Some("content_filter") => "stop_sequence".to_string(),
            Some("tool_calls") => "tool_use".to_string(),
            Some(other) => {
                warn!("Unknown finish_reason: {}", other);
                "end_turn".to_string()
            }
            None => "end_turn".to_string(),
        }
    }
    
    /// Map OpenAI error type to Claude error type
    fn map_openai_error_type(&self, openai_type: &str) -> String {
        match openai_type {
            "invalid_request_error" => "invalid_request_error".to_string(),
            "authentication_error" => "authentication_error".to_string(),
            "permission_error" => "permission_error".to_string(),
            "not_found_error" => "not_found_error".to_string(),
            "rate_limit_error" => "rate_limit_error".to_string(),
            "api_error" => "api_error".to_string(),
            "overloaded_error" => "overloaded_error".to_string(),
            _ => "api_error".to_string(),
        }
    }
    
    /// Safe parsing of tool arguments (handles empty strings as per conversion guide)
    fn safe_parse_tool_arguments(&self, arguments: &str) -> serde_json::Value {
        if arguments.is_empty() || arguments == "\"\"" {
            return serde_json::json!({});
        }
        
        serde_json::from_str(arguments)
            .unwrap_or_else(|e| {
                warn!("Failed to parse tool arguments: {}, using empty object", e);
                serde_json::json!({})
            })
    }
    
    /// Generate a simple ID for Claude responses
    fn generate_id(&self) -> String {
        Uuid::new_v4().simple().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::*;
    use chrono::Utc;
    
    fn create_test_settings() -> Settings {
        Settings {
            server: ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            openai: OpenAIConfig {
                api_key: "test_key".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                timeout: 30,
                stream_timeout: 300,
            },
            model_mapping: ModelMapping {
                haiku: "gpt-4o-mini".to_string(),
                sonnet: "gpt-4o".to_string(),
                opus: "gpt-4".to_string(),
                custom: std::collections::HashMap::new(),
            },
            request: RequestConfig {
                max_request_size: 1024,
                max_concurrent_requests: 10,
                timeout: 30,
            },
            security: SecurityConfig {
                allowed_origins: vec!["*".to_string()],
                api_key_header: "Authorization".to_string(),
                cors_enabled: true,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "text".to_string(),
            },
        }
    }
    
    #[test]
    fn test_convert_simple_request() {
        let settings = create_test_settings();
        let converter = ApiConverter::new(settings);
        
        let claude_req = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Hello".to_string()),
            }],
            ..Default::default()
        };
        
        let openai_req = converter.convert_request(claude_req).unwrap();
        
        assert_eq!(openai_req.model, "gpt-4o");
        assert_eq!(openai_req.max_tokens, Some(100));
        assert_eq!(openai_req.messages.len(), 1);
        assert_eq!(openai_req.messages[0].role, "user");
    }
    
    #[test]
    fn test_convert_response() {
        let settings = create_test_settings();
        let converter = ApiConverter::new(settings);
        
        let openai_resp = OpenAIResponse {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion".to_string(),
            created: Utc::now().timestamp() as u64,
            model: "gpt-4o".to_string(),
            choices: vec![OpenAIChoice {
                index: 0,
                message: OpenAIMessage {
                    role: "assistant".to_string(),
                    content: Some(OpenAIContent::Text("Hello!".to_string())),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                logprobs: None,
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(OpenAIUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
            system_fingerprint: None,
        };
        
        let claude_resp = converter.convert_response(openai_resp, "claude-3-sonnet").unwrap();
        
        assert_eq!(claude_resp.model, "claude-3-sonnet");
        assert_eq!(claude_resp.role, "assistant");
        assert_eq!(claude_resp.stop_reason, Some("end_turn".to_string()));
        assert_eq!(claude_resp.usage.input_tokens, 10);
        assert_eq!(claude_resp.usage.output_tokens, 5);
    }
    
    #[test]
    fn test_finish_reason_mapping() {
        let settings = create_test_settings();
        let converter = ApiConverter::new(settings);
        
        assert_eq!(converter.map_finish_reason_to_stop_reason(Some("stop")), "end_turn");
        assert_eq!(converter.map_finish_reason_to_stop_reason(Some("length")), "max_tokens");
        assert_eq!(converter.map_finish_reason_to_stop_reason(Some("content_filter")), "stop_sequence");
        assert_eq!(converter.map_finish_reason_to_stop_reason(None), "end_turn");
    }
}