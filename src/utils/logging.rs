//! Logging utilities
//!
//! Shared logging configuration and helper functions

use crate::models::claude::{ClaudeContent, ClaudeContentBlock, ClaudeRequest};
use crate::models::openai::{OpenAIContent, OpenAIMessage, OpenAIRequest};

/// Set to true to include full request details (tools, system prompts) in debug logs
/// Default is false to reduce log verbosity
pub const VERBOSE_REQUEST_LOGGING: bool = false;

/// Truncate a string with a note about original length
fn truncate_content(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}... ({} chars truncated)", &s[..max_len], s.len() - max_len)
    } else {
        s.to_string()
    }
}

/// Create a filtered version of OpenAI message for logging
fn filter_openai_message(msg: &OpenAIMessage) -> serde_json::Value {
    let content = match &msg.content {
        Some(OpenAIContent::Text(t)) => {
            // For system messages, truncate more aggressively
            let max_len = if msg.role == "system" { 100 } else { 200 };
            serde_json::Value::String(truncate_content(t, max_len))
        },
        Some(OpenAIContent::Array(arr)) => {
            serde_json::json!(format!("[...{} content blocks]", arr.len()))
        },
        None => serde_json::Value::Null,
    };
    
    let mut obj = serde_json::json!({
        "role": msg.role,
        "content": content,
    });
    
    if let Some(tool_calls) = &msg.tool_calls {
        obj["tool_calls"] = serde_json::json!(format!("[...{} tool calls]", tool_calls.len()));
    }
    if let Some(tool_call_id) = &msg.tool_call_id {
        obj["tool_call_id"] = serde_json::json!(tool_call_id);
    }
    
    obj
}

/// Create a filtered summary of OpenAI request for logging
/// Keeps original structure but truncates verbose content
pub fn create_request_log_summary(request: &OpenAIRequest) -> serde_json::Value {
    if VERBOSE_REQUEST_LOGGING {
        serde_json::to_value(request).unwrap_or(serde_json::json!({"error": "serialize failed"}))
    } else {
        let filtered_messages: Vec<serde_json::Value> = request.messages.iter()
            .map(filter_openai_message)
            .collect();
        
        let tools = match &request.tools {
            Some(t) if !t.is_empty() => serde_json::json!([format!("...{} tools (details truncated)", t.len())]),
            _ => serde_json::Value::Null,
        };
        
        serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": request.stream,
            "messages": filtered_messages,
            "tools": tools,
        })
    }
}

/// Create a filtered version of Claude message for logging  
fn filter_claude_message(msg: &crate::models::claude::ClaudeMessage) -> serde_json::Value {
    let content = match &msg.content {
        ClaudeContent::Text(t) => {
            serde_json::Value::String(truncate_content(t, 200))
        },
        ClaudeContent::Blocks(blocks) => {
            let previews: Vec<serde_json::Value> = blocks.iter()
                .take(3)
                .map(|b| {
                    match b {
                        ClaudeContentBlock::Text { text } => {
                            serde_json::json!({"type": "text", "text": truncate_content(text, 100)})
                        },
                        ClaudeContentBlock::Image { .. } => {
                            serde_json::json!({"type": "image", "source": "[truncated]"})
                        },
                        ClaudeContentBlock::ToolUse { id, name, thought_signature, .. } => {
                            let mut obj = serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": "[truncated]"});
                            if thought_signature.is_some() {
                                obj["thought_signature"] = serde_json::json!("[present]");
                            }
                            obj
                        },
                        ClaudeContentBlock::ToolResult { tool_use_id, content, .. } => {
                            serde_json::json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": truncate_content(content, 50)})
                        },
                    }
                })
                .collect();
            
            if blocks.len() > 3 {
                let mut result = previews;
                result.push(serde_json::json!(format!("...and {} more blocks", blocks.len() - 3)));
                serde_json::Value::Array(result)
            } else {
                serde_json::Value::Array(previews)
            }
        },
    };
    
    serde_json::json!({
        "role": msg.role,
        "content": content,
    })
}

/// Create a filtered summary of Claude request for logging
/// Keeps original structure but truncates verbose content
pub fn create_claude_request_log_summary(request: &ClaudeRequest) -> serde_json::Value {
    if VERBOSE_REQUEST_LOGGING {
        serde_json::to_value(request).unwrap_or(serde_json::json!({"error": "serialize failed"}))
    } else {
        let filtered_messages: Vec<serde_json::Value> = request.messages.iter()
            .map(filter_claude_message)
            .collect();
        
        let tools = match &request.tools {
            Some(t) if !t.is_empty() => serde_json::json!([format!("...{} tools (details truncated)", t.len())]),
            _ => serde_json::Value::Null,
        };
        
        let system = match &request.system {
            Some(s) => {
                let text = s.extract_text();
                serde_json::Value::String(truncate_content(&text, 100))
            },
            None => serde_json::Value::Null,
        };
        
        serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": request.stream,
            "system": system,
            "messages": filtered_messages,
            "tools": tools,
        })
    }
}
