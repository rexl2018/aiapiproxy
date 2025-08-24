//! OpenAI API data models
//! 
//! Defines OpenAI API request and response structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OpenAI API request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIRequest {
    /// Model name
    pub model: String,
    /// Message list
    pub messages: Vec<OpenAIMessage>,
    /// Maximum tokens to generate (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature parameter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p parameter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Number of generations (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Stop sequences (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream response (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Presence penalty (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Logit bias (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<String, f32>>,
    /// User identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Response format (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<OpenAIResponseFormat>,
    /// Seed (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    /// Tools (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAITool>>,
    /// Tool choice (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

/// OpenAI message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    /// Role (system/user/assistant/tool)
    pub role: String,
    /// Message content
    pub content: Option<OpenAIContent>,
    /// Name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    /// Tool call ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// OpenAI message content (can be string or content array)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIContent {
    /// Simple text content
    Text(String),
    /// Content array (supports multimodal)
    Array(Vec<OpenAIContentPart>),
}

/// OpenAI content part
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAIContentPart {
    /// Text part
    #[serde(rename = "text")]
    Text { text: String },
    /// Image URL part
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAIImageUrl },
}

/// OpenAI image URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIImageUrl {
    /// Image URL
    pub url: String,
    /// Detail level (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// OpenAI response format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponseFormat {
    /// Format type
    #[serde(rename = "type")]
    pub format_type: String,
}

/// OpenAI tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAITool {
    /// Tool type
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition
    pub function: OpenAIFunction,
}

/// OpenAI function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunction {
    /// Function name
    pub name: String,
    /// Function description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Parameter schema (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// OpenAI tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    /// Call ID
    pub id: String,
    /// Tool type
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function call
    pub function: OpenAIFunctionCall,
}

/// OpenAI function call structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionCall {
    /// Function name (optional for streaming)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Function arguments as JSON string (optional for streaming)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// OpenAI API response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponse {
    /// Response ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// Choice list
    pub choices: Vec<OpenAIChoice>,
    /// Usage statistics
    pub usage: OpenAIUsage,
    /// System fingerprint (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
}

/// OpenAI choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChoice {
    /// Choice index
    pub index: u32,
    /// Message content
    pub message: OpenAIMessage,
    /// Log probabilities (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// OpenAI usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIUsage {
    /// Prompt token count
    pub prompt_tokens: u32,
    /// Completion token count
    pub completion_tokens: u32,
    /// Total token count
    pub total_tokens: u32,
}

/// OpenAI streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamResponse {
    /// Response ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// System fingerprint (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// Choice list
    pub choices: Vec<OpenAIStreamChoice>,
}

/// OpenAI streaming choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamChoice {
    /// Choice index
    pub index: u32,
    /// Delta content
    pub delta: OpenAIStreamDelta,
    /// Log probabilities (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// OpenAI streaming delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIStreamDelta {
    /// Role (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Content (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
}

/// OpenAI error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIErrorResponse {
    /// Error information
    pub error: OpenAIError,
}

/// OpenAI error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIError {
    /// Error message
    pub message: String,
    /// Error type
    #[serde(rename = "type")]
    pub error_type: String,
    /// Error parameter (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    /// Error code (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl OpenAIContent {
    /// Extract text content
    pub fn extract_text(&self) -> String {
        match self {
            OpenAIContent::Text(text) => text.clone(),
            OpenAIContent::Array(parts) => {
                parts
                    .iter()
                    .filter_map(|part| match part {
                        OpenAIContentPart::Text { text } => Some(text.clone()),
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
            OpenAIContent::Text(_) => false,
            OpenAIContent::Array(parts) => {
                parts.iter().any(|part| matches!(part, OpenAIContentPart::ImageUrl { .. }))
            }
        }
    }
}

impl Default for OpenAIRequest {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            messages: Vec::new(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            n: None,
            stop: None,
            stream: None,
            presence_penalty: None,
            frequency_penalty: None,
            logit_bias: None,
            user: None,
            response_format: None,
            seed: None,
            tools: None,
            tool_choice: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_openai_request_serialization() {
        let request = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: Some(OpenAIContent::Text("Hello".to_string())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: Some(100),
            ..Default::default()
        };
        
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: OpenAIRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(request.model, deserialized.model);
        assert_eq!(request.max_tokens, deserialized.max_tokens);
    }
    
    #[test]
    fn test_content_text_extraction() {
        let text_content = OpenAIContent::Text("Hello world".to_string());
        assert_eq!(text_content.extract_text(), "Hello world");
        
        let array_content = OpenAIContent::Array(vec![
            OpenAIContentPart::Text { text: "Hello ".to_string() },
            OpenAIContentPart::Text { text: "world".to_string() },
        ]);
        assert_eq!(array_content.extract_text(), "Hello world");
    }
}