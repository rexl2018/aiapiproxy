//! API converter service
//! 
//! Responsible for converting between Claude API and OpenAI API formats

use crate::config::Settings;
use crate::models::{
    claude::*, openai::*,
};
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
    pub fn convert_request(&self, claude_req: ClaudeRequest) -> Result<OpenAIRequest> {
        debug!("Starting conversion from Claude request to OpenAI format");
        
        // Map model name
        let openai_model = self.settings
            .get_openai_model(&claude_req.model)
            .context("Unable to map Claude model to OpenAI model")?;
        
        // Convert messages
        let mut openai_messages = Vec::new();
        
        // If there's a system prompt, add it as a system message
        if let Some(system) = claude_req.system {
            openai_messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(OpenAIContent::Text(system)),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        
        // Convert Claude messages to OpenAI messages
        for claude_msg in claude_req.messages {
            let openai_msg = self.convert_claude_message_to_openai(claude_msg)?;
            openai_messages.push(openai_msg);
        }
        
        // Build OpenAI request
        let openai_req = OpenAIRequest {
            model: openai_model,
            messages: openai_messages,
            max_tokens: Some(claude_req.max_tokens),
            temperature: claude_req.temperature,
            top_p: claude_req.top_p,
            stop: claude_req.stop_sequences,
            stream: claude_req.stream,
            n: Some(1), // Claude always returns a single response
            presence_penalty: None,
            frequency_penalty: None,
            logit_bias: None,
            user: None,
            response_format: None,
            seed: None,
            tools: None,
            tool_choice: None,
        };
        
        debug!("Claude request conversion completed");
        Ok(openai_req)
    }
    
    /// Convert OpenAI response to Claude response
    pub fn convert_response(&self, openai_resp: OpenAIResponse, original_model: &str) -> Result<ClaudeResponse> {
        debug!("Starting conversion from OpenAI response to Claude format");
        
        if openai_resp.choices.is_empty() {
            anyhow::bail!("No choices in OpenAI response");
        }
        
        let choice = &openai_resp.choices[0];
        let message = &choice.message;
        
        // Extract content
        let content_text = message.content
            .as_ref()
            .map(|c| c.extract_text())
            .unwrap_or_default();
        
        // Build Claude content blocks
        let content_blocks = if content_text.is_empty() {
            vec![]
        } else {
            vec![ClaudeContentBlock::Text { text: content_text }]
        };
        
        // Map stop reason
        let stop_reason = self.map_finish_reason_to_stop_reason(choice.finish_reason.as_deref());
        
        // Build Claude response
        let claude_resp = ClaudeResponse {
            id: format!("msg_{}", Uuid::new_v4().simple()),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            content: content_blocks,
            model: original_model.to_string(),
            stop_reason: Some(stop_reason),
            stop_sequence: None,
            usage: ClaudeUsage {
                input_tokens: openai_resp.usage.prompt_tokens,
                output_tokens: openai_resp.usage.completion_tokens,
            },
        };
        
        debug!("OpenAI response conversion completed");
        Ok(claude_resp)
    }
    
    /// Convert OpenAI stream response to Claude stream events
    pub fn convert_stream_chunk(&self, openai_chunk: OpenAIStreamResponse, original_model: &str) -> Result<Vec<ClaudeStreamEvent>> {
        debug!("Converting OpenAI stream response chunk");
        
        let mut events = Vec::new();
        
        if openai_chunk.choices.is_empty() {
            return Ok(events);
        }
        
        let choice = &openai_chunk.choices[0];
        let delta = &choice.delta;
        
        // If this is the first chunk (contains role info), generate message start event
        if delta.role.is_some() {
            let message_start = ClaudeStreamEvent::MessageStart {
                message: ClaudeStreamMessage {
                    id: format!("msg_{}", Uuid::new_v4().simple()),
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
            };
            events.push(message_start);
            
            // Content block start event
            let content_block_start = ClaudeStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ClaudeContentBlock::Text { text: String::new() },
            };
            events.push(content_block_start);
        }
        
        // If there's content delta, generate content block delta event
        if let Some(content) = &delta.content {
            if !content.is_empty() {
                let content_delta = ClaudeStreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ClaudeContentDelta::TextDelta {
                        text: content.clone(),
                    },
                };
                events.push(content_delta);
            }
        }
        
        // If this is the last chunk (has finish reason), generate end events
        if let Some(finish_reason) = &choice.finish_reason {
            // Content block stop event
            let content_block_stop = ClaudeStreamEvent::ContentBlockStop { index: 0 };
            events.push(content_block_stop);
            
            // Message delta event (contains stop reason)
            let stop_reason = self.map_finish_reason_to_stop_reason(Some(finish_reason));
            let message_delta = ClaudeStreamEvent::MessageDelta {
                delta: ClaudeMessageDelta {
                    stop_reason: Some(stop_reason),
                    stop_sequence: None,
                },
                usage: ClaudeUsage {
                    input_tokens: 0, // Stream responses usually don't include usage stats
                    output_tokens: 0,
                },
            };
            events.push(message_delta);
            
            // Message stop event
            let message_stop = ClaudeStreamEvent::MessageStop;
            events.push(message_stop);
        }
        
        Ok(events)
    }
    
    /// Convert OpenAI error to Claude error
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
    
    /// Convert Claude message to OpenAI message
    fn convert_claude_message_to_openai(&self, claude_msg: ClaudeMessage) -> Result<OpenAIMessage> {
        let content = match claude_msg.content {
            ClaudeContent::Text(text) => Some(OpenAIContent::Text(text)),
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
                    }
                }
                
                if openai_parts.is_empty() {
                    None
                } else {
                    Some(OpenAIContent::Array(openai_parts))
                }
            }
        };
        
        Ok(OpenAIMessage {
            role: claude_msg.role,
            content,
            name: None,
            tool_calls: None,
            tool_call_id: None,
        })
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
                api_key: "sk-test".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                timeout: 30,
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
            usage: OpenAIUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
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