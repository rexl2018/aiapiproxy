//! Claude API data models
//! 
//! Defines Claude API request and response structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Claude API request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeRequest {
    /// Model name
    pub model: String,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Message list
    pub messages: Vec<ClaudeMessage>,
    /// System prompt (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Temperature parameter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p parameter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k parameter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Stop sequences (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Whether to stream response (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Metadata (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Claude message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMessage {
    /// Role (user/assistant/system)
    pub role: String,
    /// Message content
    pub content: ClaudeContent,
}

/// Claude content type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaudeContent {
    /// Plain text content
    Text(String),
    /// Structured content blocks
    Blocks(Vec<ClaudeContentBlock>),
}

/// Claude content block
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeContentBlock {
    /// Text block
    #[serde(rename = "text")]
    Text { text: String },
    /// Image block
    #[serde(rename = "image")]
    Image {
        source: ClaudeImageSource,
    },
}

/// Claude image source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeImageSource {
    /// Source type (base64)
    #[serde(rename = "type")]
    pub source_type: String,
    /// Media type
    pub media_type: String,
    /// Image data
    pub data: String,
}

/// Claude API response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeResponse {
    /// Response ID
    pub id: String,
    /// Response type
    #[serde(rename = "type")]
    pub response_type: String,
    /// Role
    pub role: String,
    /// Response content
    pub content: Vec<ClaudeContentBlock>,
    /// Model used
    pub model: String,
    /// Stop reason
    pub stop_reason: Option<String>,
    /// Stop sequence
    pub stop_sequence: Option<String>,
    /// Usage statistics
    pub usage: ClaudeUsage,
}

/// Claude usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeUsage {
    /// Input token count
    pub input_tokens: u32,
    /// Output token count
    pub output_tokens: u32,
}

/// Claude streaming response event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeStreamEvent {
    /// Message start
    #[serde(rename = "message_start")]
    MessageStart {
        message: ClaudeStreamMessage,
    },
    /// Content block start
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ClaudeContentBlock,
    },
    /// Content block delta
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u32,
        delta: ClaudeContentDelta,
    },
    /// Content block stop
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        index: u32,
    },
    /// Message delta
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: ClaudeMessageDelta,
        usage: ClaudeUsage,
    },
    /// Message stop
    #[serde(rename = "message_stop")]
    MessageStop,
    /// Ping event
    #[serde(rename = "ping")]
    Ping,
    /// Error event
    #[serde(rename = "error")]
    Error {
        error: ClaudeError,
    },
}

/// Claude streaming message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeStreamMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub role: String,
    pub content: Vec<serde_json::Value>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: ClaudeUsage,
}

/// Claude content delta
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeContentDelta {
    /// Text delta
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
}

/// Claude message delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMessageDelta {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

/// Claude error structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

/// Claude error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeErrorResponse {
    #[serde(rename = "type")]
    pub error_type: String,
    pub error: ClaudeError,
}

impl ClaudeContent {
    /// Extract text content
    pub fn extract_text(&self) -> String {
        match self {
            ClaudeContent::Text(text) => text.clone(),
            ClaudeContent::Blocks(blocks) => {
                blocks
                    .iter()
                    .filter_map(|block| match block {
                        ClaudeContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            }
        }
    }
    
    /// Check if contains images
    pub fn has_images(&self) -> bool {
        match self {
            ClaudeContent::Text(_) => false,
            ClaudeContent::Blocks(blocks) => {
                blocks.iter().any(|block| matches!(block, ClaudeContentBlock::Image { .. }))
            }
        }
    }
}

impl Default for ClaudeRequest {
    fn default() -> Self {
        Self {
            model: "claude-3-5-sonnet-20241022".to_string(),
            max_tokens: 1024,
            messages: Vec::new(),
            system: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            stream: None,
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_claude_request_serialization() {
        let request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Hello".to_string()),
            }],
            ..Default::default()
        };
        
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ClaudeRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(request.model, deserialized.model);
        assert_eq!(request.max_tokens, deserialized.max_tokens);
    }
    
    #[test]
    fn test_content_text_extraction() {
        let text_content = ClaudeContent::Text("Hello world".to_string());
        assert_eq!(text_content.extract_text(), "Hello world");
        
        let blocks_content = ClaudeContent::Blocks(vec![
            ClaudeContentBlock::Text { text: "Hello ".to_string() },
            ClaudeContentBlock::Text { text: "world".to_string() },
        ]);
        assert_eq!(blocks_content.extract_text(), "Hello world");
    }
}