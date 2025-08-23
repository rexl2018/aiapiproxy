//! Data models module
//!
//! Defines request and response data structures for Claude and OpenAI APIs

use serde::{Deserialize, Serialize};

pub mod claude;
pub mod openai;


/// Generic API error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    /// Error type
    pub error_type: String,
    /// Error message
    pub message: String,
    /// Error code (optional)
    pub code: Option<String>,
    /// Details (optional)
    pub details: Option<serde_json::Value>,
}

/// Usage statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    /// Input token count
    pub input_tokens: u32,
    /// Output token count
    pub output_tokens: u32,
    /// Total token count
    pub total_tokens: u32,
}