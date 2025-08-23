//! Error handling module
//! 
//! Defines error types and handling logic used in the project

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Application error types
#[derive(Error, Debug)]
pub enum AppError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(#[from] anyhow::Error),
    
    /// HTTP client error
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),
    
    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    /// Authentication error
    #[error("Authentication failed: {0}")]
    Authentication(String),
    
    /// Authorization error
    #[error("Authorization failed: {0}")]
    Authorization(String),
    
    /// Request validation failed
    #[error("Request validation failed: {0}")]
    Validation(String),
    
    /// API conversion failed
    #[error("API conversion failed: {0}")]
    Conversion(String),
    
    /// External API error
    #[error("External API error: {0}")]
    ExternalApi(String),
    
    /// Rate limit exceeded
    #[error("Rate limit exceeded, please try again later")]
    RateLimit,
    
    /// Service temporarily unavailable
    #[error("Service temporarily unavailable: {0}")]
    ServiceUnavailable(String),
    
    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),
    
    /// Request timeout
    #[error("Request timeout")]
    Timeout,
    
    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    /// Payload too large
    #[error("Payload too large")]
    PayloadTooLarge,
}

/// Error response structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error type
    #[serde(rename = "type")]
    pub error_type: String,
    /// Error message
    pub message: String,
    /// Error code (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Request ID (for tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Claude API error response format
#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeErrorResponse {
    #[serde(rename = "type")]
    pub error_type: String,
    pub error: ClaudeError,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

impl AppError {
    /// Get HTTP status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Authentication(_) => StatusCode::UNAUTHORIZED,
            AppError::Authorization(_) => StatusCode::FORBIDDEN,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::RateLimit => StatusCode::TOO_MANY_REQUESTS,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::Timeout => StatusCode::REQUEST_TIMEOUT,
            AppError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::ExternalApi(_) => StatusCode::BAD_GATEWAY,
            AppError::Config(_) 
            | AppError::HttpClient(_) 
            | AppError::Serialization(_) 
            | AppError::Conversion(_) 
            | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    
    /// Get error type string
    pub fn error_type(&self) -> &'static str {
        match self {
            AppError::Authentication(_) => "authentication_error",
            AppError::Authorization(_) => "permission_error",
            AppError::Validation(_) => "invalid_request_error",
            AppError::NotFound(_) => "not_found_error",
            AppError::RateLimit => "rate_limit_error",
            AppError::PayloadTooLarge => "invalid_request_error",
            AppError::Timeout => "timeout_error",
            AppError::ServiceUnavailable(_) => "overloaded_error",
            AppError::ExternalApi(_) => "api_error",
            AppError::Config(_) 
            | AppError::HttpClient(_) 
            | AppError::Serialization(_) 
            | AppError::Conversion(_) 
            | AppError::Internal(_) => "api_error",
        }
    }
    
    /// Whether detailed error information should be logged
    pub fn should_log_details(&self) -> bool {
        match self {
            AppError::Authentication(_) | AppError::Authorization(_) => false,
            _ => true,
        }
    }
    
    /// Convert to Claude API error format
    pub fn to_claude_error(&self) -> ClaudeErrorResponse {
        ClaudeErrorResponse {
            error_type: "error".to_string(),
            error: ClaudeError {
                error_type: self.error_type().to_string(),
                message: self.to_string(),
            },
        }
    }
}

/// Implement IntoResponse trait to allow errors to be returned directly as HTTP responses
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        
        // Log error
        if self.should_log_details() {
            tracing::error!("Application error: {} - Status code: {}", self, status);
        } else {
            tracing::warn!("Client error: {} - Status code: {}", self.error_type(), status);
        }
        
        // Create error response
        let error_response = self.to_claude_error();
        
        (status, Json(error_response)).into_response()
    }
}

/// Result type alias
pub type AppResult<T> = Result<T, AppError>;

/// Error handling helper functions
#[allow(dead_code)]
pub mod helpers {
    use super::*;
    
    /// Create authentication error
    pub fn auth_error(message: impl Into<String>) -> AppError {
        AppError::Authentication(message.into())
    }
    
    /// Create validation error
    pub fn validation_error(message: impl Into<String>) -> AppError {
        AppError::Validation(message.into())
    }
    
    /// Create conversion error
    pub fn conversion_error(message: impl Into<String>) -> AppError {
        AppError::Conversion(message.into())
    }
    
    /// Create external API error
    pub fn external_api_error(message: impl Into<String>) -> AppError {
        AppError::ExternalApi(message.into())
    }
    
    /// Create internal error
    pub fn internal_error(message: impl Into<String>) -> AppError {
        AppError::Internal(message.into())
    }
    
    /// Create service unavailable error
    pub fn service_unavailable_error(message: impl Into<String>) -> AppError {
        AppError::ServiceUnavailable(message.into())
    }
}

/// Error context extension trait
#[allow(dead_code)]
pub trait ErrorContext<T> {
    /// Add validation error context
    fn validation_context(self, message: &str) -> AppResult<T>;
    
    /// Add conversion error context
    fn conversion_context(self, message: &str) -> AppResult<T>;
    
    /// Add external API error context
    fn external_api_context(self, message: &str) -> AppResult<T>;
    
    /// Add internal error context
    fn internal_context(self, message: &str) -> AppResult<T>;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn validation_context(self, message: &str) -> AppResult<T> {
        self.map_err(|e| AppError::Validation(format!("{}: {}", message, e)))
    }
    
    fn conversion_context(self, message: &str) -> AppResult<T> {
        self.map_err(|e| AppError::Conversion(format!("{}: {}", message, e)))
    }
    
    fn external_api_context(self, message: &str) -> AppResult<T> {
        self.map_err(|e| AppError::ExternalApi(format!("{}: {}", message, e)))
    }
    
    fn internal_context(self, message: &str) -> AppResult<T> {
        self.map_err(|e| AppError::Internal(format!("{}: {}", message, e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_status_codes() {
        assert_eq!(AppError::Authentication("test".to_string()).status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(AppError::Authorization("test".to_string()).status_code(), StatusCode::FORBIDDEN);
        assert_eq!(AppError::Validation("test".to_string()).status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(AppError::NotFound("test".to_string()).status_code(), StatusCode::NOT_FOUND);
        assert_eq!(AppError::RateLimit.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(AppError::Internal("test".to_string()).status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    #[test]
    fn test_error_types() {
        assert_eq!(AppError::Authentication("test".to_string()).error_type(), "authentication_error");
        assert_eq!(AppError::Validation("test".to_string()).error_type(), "invalid_request_error");
        assert_eq!(AppError::RateLimit.error_type(), "rate_limit_error");
        assert_eq!(AppError::Internal("test".to_string()).error_type(), "api_error");
    }
    
    #[test]
    fn test_claude_error_conversion() {
        let app_error = AppError::Validation("Invalid input".to_string());
        let claude_error = app_error.to_claude_error();
        
        assert_eq!(claude_error.error_type, "error");
        assert_eq!(claude_error.error.error_type, "invalid_request_error");
        assert_eq!(claude_error.error.message, "Request validation failed: Invalid input");
    }
    
    #[test]
    fn test_error_context() {
        let result: Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found"
        ));
        
        let app_result = result.validation_context("Failed to read config");
        assert!(app_result.is_err());
        
        if let Err(AppError::Validation(msg)) = app_result {
            assert!(msg.contains("Failed to read config"));
            assert!(msg.contains("file not found"));
        } else {
            panic!("Expected validation error");
        }
    }
    
    #[test]
    fn test_helpers() {
        let auth_err = helpers::auth_error("Invalid token");
        assert!(matches!(auth_err, AppError::Authentication(_)));
        
        let validation_err = helpers::validation_error("Missing field");
        assert!(matches!(validation_err, AppError::Validation(_)));
        
        let conversion_err = helpers::conversion_error("Format mismatch");
        assert!(matches!(conversion_err, AppError::Conversion(_)));
    }
}