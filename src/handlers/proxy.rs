//! Claude API proxy handlers
//! 
//! Handles Claude API requests and converts them to OpenAI API calls
//! Supports both legacy single-provider mode and multi-provider routing

use crate::handlers::AppState;
use crate::models::claude::*;
use crate::models::openai::*;
use crate::utils::logging::{create_request_log_summary, create_claude_request_log_summary};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, Sse},
    Json,
};
use axum::response::sse::{Event, KeepAlive};
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, warn};

/// Handle Claude message requests
/// 
/// POST /v1/messages
/// 
/// Routes requests to providers based on model path (e.g., "openai/gpt-4o", "modelhub-sg1/gpt-5")
pub async fn handle_messages(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(claude_request): Json<ClaudeRequest>,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Received Claude API request for model: {}", claude_request.model);
    
    // 🔍 DEBUG: 记录客户端请求摘要
    let log_summary = create_claude_request_log_summary(&claude_request);
    if let Ok(summary_json) = serde_json::to_string_pretty(&log_summary) {
        debug!("📥 Client Request:\n{}", summary_json);
    }
    
    // Validate request
    if let Err(error_msg) = validate_claude_request(&claude_request) {
        warn!("Request validation failed: {}", error_msg);
        return Ok(create_error_response("invalid_request_error", &error_msg, StatusCode::BAD_REQUEST));
    }
    
    // Convert Claude request to OpenAI request
    let openai_request = match state.converter.convert_request(claude_request.clone()) {
        Ok(mut req) => {
            // Keep the original model path for routing
            req.model = claude_request.model.clone();
            
            let log_summary = create_request_log_summary(&req);
            if let Ok(summary_json) = serde_json::to_string_pretty(&log_summary) {
                debug!("🔄 Converted OpenAI Request:\n{}", summary_json);
            }
            req
        },
        Err(e) => {
            error!("Request conversion failed: {}", e);
            return Ok(create_error_response("conversion_error", "Failed to convert request", StatusCode::INTERNAL_SERVER_ERROR));
        }
    };
    
    let original_model = claude_request.model.clone();
    let is_streaming = claude_request.stream.unwrap_or(false);
    
    if is_streaming {
        handle_stream_request(state, openai_request, original_model).await
    } else {
        handle_normal_request(state, openai_request, original_model).await
    }
}


/// Categorize error message to appropriate error type and message
fn categorize_error(error_message: &str) -> (&str, &str, StatusCode) {
    if error_message.contains("429") || error_message.contains("TooManyRequests") || error_message.contains("RateLimitExceeded") || error_message.contains("Too Many Requests") {
        ("rate_limit_error", "Rate limit exceeded. Please try again later.", StatusCode::TOO_MANY_REQUESTS)
    } else if error_message.contains("authentication") || error_message.contains("Invalid API key") || error_message.contains("401") {
        ("authentication_error", "Invalid API key provided.", StatusCode::UNAUTHORIZED)
    } else if error_message.contains("insufficient_quota") || error_message.contains("quota") {
        ("billing_error", "Insufficient quota or billing issue.", StatusCode::PAYMENT_REQUIRED)
    } else if error_message.contains("not found") || error_message.contains("Model not found") || error_message.contains("404") {
        ("not_found_error", "The requested model was not found.", StatusCode::NOT_FOUND)
    } else if error_message.contains("400") || error_message.contains("Bad Request") {
        ("invalid_request_error", "Bad request to upstream API.", StatusCode::BAD_REQUEST)
    } else {
        ("api_error", "External API request failed.", StatusCode::BAD_GATEWAY)
    }
}

/// Handle normal (non-streaming) requests
async fn handle_normal_request(
    state: Arc<AppState>,
    openai_request: OpenAIRequest,
    original_model: String,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Handling normal request for model: {}", original_model);
    
    // Route and call provider API
    let openai_response = match state.router.chat_complete(openai_request).await {
        Ok(response) => {
            if let Ok(response_json) = serde_json::to_string_pretty(&response) {
                debug!("📤 Provider API Response:\n{}", response_json);
            }
            response
        },
        Err(e) => {
            error!("Provider API request failed: {}", e);
            let error_msg = e.to_string();
            let (error_type, claude_message, status_code) = categorize_error(&error_msg);
            return Ok(create_error_response(error_type, claude_message, status_code));
        }
    };
    
    // Convert response format
    let claude_response = match state.converter.convert_response(openai_response, &original_model) {
        Ok(response) => {
            if let Ok(claude_json) = serde_json::to_string_pretty(&response) {
                debug!("📋 Final Claude Response:\n{}", claude_json);
            }
            response
        },
        Err(e) => {
            error!("Response conversion failed: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    debug!("Request processing completed");
    Ok(Json(claude_response).into_response())
}

/// Handle streaming requests
async fn handle_stream_request(
    state: Arc<AppState>,
    mut openai_request: OpenAIRequest,
    original_model: String,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Handling streaming request for model: {}", original_model);
    
    openai_request.stream = Some(true);
    
    let router = state.router.clone();
    let converter = state.converter.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, axum::Error>>(100);
    
    tokio::spawn(async move {
        let stream = match router.chat_stream(openai_request).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("Provider streaming API request failed: {}", e);
                let error_msg = e.to_string();
                let (error_type, claude_message, _status_code) = categorize_error(&error_msg);
                
                let claude_error = ClaudeStreamEvent::Error {
                    error: ClaudeError {
                        error_type: error_type.to_string(),
                        message: claude_message.to_string(),
                    },
                };
                
                if let Ok(error_json) = serde_json::to_string(&claude_error) {
                    let error_event = Event::default()
                        .event("error")
                        .data(error_json);
                    let _ = tx.send(Ok(error_event)).await;
                }
                return;
            }
        };
        
        let mut stream = Box::pin(stream);
        
        while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
            match chunk_result {
                Ok(openai_chunk) => {
                    match converter.convert_stream_chunk(openai_chunk, &original_model) {
                        Ok(claude_events) => {
                            for event in claude_events {
                                match serde_json::to_string(&event) {
                                    Ok(json) => {
                                        let sse_event = Event::default().data(json);
                                        if tx.send(Ok(sse_event)).await.is_err() {
                                            debug!("Client disconnected");
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Event serialization failed: {}", e);
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Streaming response conversion failed: {}", e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    error!("Provider streaming response error: {}", e);
                    return;
                }
            }
        }
        
        let end_event = Event::default().event("done").data("{}");
        let _ = tx.send(Ok(end_event)).await;
    });
    
    let stream = ReceiverStream::new(rx);
    let sse = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive")
        );
    
    debug!("Starting streaming response transmission");
    Ok(sse.into_response())
}

/// Validate Claude request
fn validate_claude_request(request: &ClaudeRequest) -> Result<(), String> {
    // Check model name
    if request.model.is_empty() {
        return Err("Model name cannot be empty".to_string());
    }
    
    // Check max_tokens
    if request.max_tokens == 0 {
        return Err("max_tokens must be greater than 0".to_string());
    }
    
    if request.max_tokens > 100000 {
        return Err("max_tokens cannot exceed 100000".to_string());
    }
    
    // Check message list
    if request.messages.is_empty() {
        return Err("Message list cannot be empty".to_string());
    }
    
    // Check message format
    for (i, message) in request.messages.iter().enumerate() {
        if message.role.is_empty() {
            return Err(format!("Message {} role cannot be empty", i));
        }
        
        if !matches!(message.role.as_str(), "user" | "assistant" | "system") {
            return Err(format!("Message {} role is invalid: {}", i, message.role));
        }
        
        // Check if content is empty - allow messages with whitespace or special characters
        let content_text = message.content.extract_text();
        let has_images = message.content.has_images();
        let has_tool_calls = message.content.has_tool_calls();
        let has_tool_results = message.content.has_tool_results();
        let is_other_content = message.content.is_other();
        
        // 🔍 DEBUG: 详细记录消息验证信息
        debug!("Validating message {}: role={}, content_text_len={}, has_images={}, has_tool_calls={}, has_tool_results={}, is_other={}", 
               i, message.role, content_text.len(), has_images, has_tool_calls, has_tool_results, is_other_content);
        
        // Only reject if content is completely empty (no text, images, tool calls, tool results, or special content)
        // 🔧 修复工具调用验证：允许只包含工具调用或工具结果的消息
        // 🔧 允许空的 assistant 消息（在 tool_use 流程中可能出现）
        // 🔧 允许 Other 类型内容（null 或其他特殊格式）- 让上游 API 处理
        if content_text.is_empty() && !has_images && !has_tool_calls && !has_tool_results && !is_other_content {
            // Allow empty assistant messages - they can occur in tool_use flows
            // Only reject truly empty user messages (no tool results either)
            if message.role == "user" {
                warn!("Message {} validation failed - completely empty user content", i);
                warn!("Full request context: model='{}', messages_count={}, max_tokens={}", 
                      request.model, request.messages.len(), request.max_tokens);
                return Err(format!("Message {} content cannot be empty", i));
            } else {
                debug!("Allowing empty {} message at index {} (may be part of tool_use flow)", 
                       message.role, i);
            }
        }
        
        // For text-only messages, allow whitespace-only content as some clients may send formatting
        // This is more permissive than the original validation
    }
    
    // Check temperature parameter
    if let Some(temp) = request.temperature {
        if temp < 0.0 || temp > 2.0 {
            return Err("temperature must be between 0.0 and 2.0".to_string());
        }
    }
    
    // Check top_p parameter
    if let Some(top_p) = request.top_p {
        if top_p < 0.0 || top_p > 1.0 {
            return Err("top_p must be between 0.0 and 1.0".to_string());
        }
    }
    
    // Check top_k parameter
    if let Some(top_k) = request.top_k {
        if top_k == 0 {
            return Err("top_k must be greater than 0".to_string());
        }
    }
    
    Ok(())
}

/// Extract authentication header
fn extract_auth_header(headers: &HeaderMap, auth_header_name: &str) -> Option<String> {
    headers
        .get(auth_header_name)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

/// Error response helper function that creates a Claude-compatible error response
fn create_error_response(error_type: &str, message: &str, status_code: StatusCode) -> Response<axum::body::Body> {
    // Create a response that matches Claude API error format but includes expected fields
    let error_response = serde_json::json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": message
        },
        // Include content field as empty array to prevent client-side filter errors
        "content": [],
        // Include other fields that clients might expect
        "id": format!("error_{}", uuid::Uuid::new_v4().simple()),
        "model": "claude-3-sonnet",
        "role": "assistant",
        "stop_reason": "error",
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0
        }
    });
    
    Response::builder()
        .status(status_code)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(error_response.to_string()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::models::claude::*; // 暂时注释掉未使用的导入
    
    #[test]
    fn test_validate_claude_request() {
        // Valid request
        let valid_request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Hello".to_string()),
            }],
            ..Default::default()
        };
        
        assert!(validate_claude_request(&valid_request).is_ok());
        
        // Invalid request - empty model
        let invalid_request = ClaudeRequest {
            model: "".to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Hello".to_string()),
            }],
            ..Default::default()
        };
        
        assert!(validate_claude_request(&invalid_request).is_err());
        
        // Invalid request - max_tokens is 0
        let invalid_request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 0,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Hello".to_string()),
            }],
            ..Default::default()
        };
        
        assert!(validate_claude_request(&invalid_request).is_err());
        
        // Invalid request - empty messages list
        let invalid_request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages: vec![],
            ..Default::default()
        };
        
        assert!(validate_claude_request(&invalid_request).is_err());
    }
    
    #[test]
    fn test_extract_auth_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer sk-test123".parse().unwrap());
        
        let auth = extract_auth_header(&headers, "Authorization");
        assert_eq!(auth, Some("Bearer sk-test123".to_string()));
        
        let no_auth = extract_auth_header(&headers, "X-API-Key");
        assert_eq!(no_auth, None);
    }
    
    #[test]
    fn test_temperature_validation() {
        let mut request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Hello".to_string()),
            }],
            temperature: Some(1.5),
            ..Default::default()
        };
        
        assert!(validate_claude_request(&request).is_ok());
        
        request.temperature = Some(3.0);
        assert!(validate_claude_request(&request).is_err());
        
        request.temperature = Some(-0.5);
        assert!(validate_claude_request(&request).is_err());
    }
}