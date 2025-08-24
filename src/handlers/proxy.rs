//! Claude API proxy handlers
//! 
//! Handles Claude API requests and converts them to OpenAI API calls

use crate::handlers::AppState;
use crate::models::claude::*;
use crate::models::openai::*;
use tracing::warn;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Response, Sse, IntoResponse},
    Json,
};
use axum::response::sse::{Event, KeepAlive};
// use futures::StreamExt; // 暂时注释掉未使用的导入
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Handle Claude message requests
/// 
/// POST /v1/messages
pub async fn handle_messages(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(claude_request): Json<ClaudeRequest>,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Received Claude API request");
    
    // 🔍 DEBUG: 记录完整的客户端请求内容
    if let Ok(request_json) = serde_json::to_string_pretty(&claude_request) {
        info!("📥 Client Request Body:\n{}", request_json);
    }
    
    // Validate request
    if let Err(error_msg) = validate_claude_request(&claude_request) {
        warn!("Request validation failed: {}", error_msg);
        return Ok(create_error_response("invalid_request_error", &error_msg).into_response());
    }
    
    // Convert Claude request to OpenAI request
    let openai_request = match state.converter.convert_request(claude_request.clone()) {
        Ok(req) => {
            // 🔍 DEBUG: 记录转换后的OpenAI请求内容
            if let Ok(openai_json) = serde_json::to_string_pretty(&req) {
                info!("🔄 Converted OpenAI Request:\n{}", openai_json);
            }
            req
        },
        Err(e) => {
            error!("Request conversion failed: {}", e);
            return Ok(create_error_response("conversion_error", "Failed to convert request").into_response());
        }
    };
    
    // Save original model name for response conversion
    let original_model = claude_request.model.clone();
    
    // Choose handling method based on whether it's a streaming request
    if claude_request.stream.unwrap_or(false) {
        handle_stream_request(state, openai_request, original_model).await
    } else {
        handle_normal_request(state, openai_request, original_model).await
    }
}

/// Handle normal requests
async fn handle_normal_request(
    state: Arc<AppState>,
    openai_request: OpenAIRequest,
    original_model: String,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Handling normal request");
    
    // Call OpenAI API
    let openai_response = match state.openai_client.chat_completions_with_retry(openai_request).await {
        Ok(response) => {
            // 🔍 DEBUG: 记录OpenAI API响应内容
            if let Ok(response_json) = serde_json::to_string_pretty(&response) {
                info!("📤 OpenAI API Response:\n{}", response_json);
            }
            response
        },
        Err(e) => {
            error!("OpenAI API request failed: {}", e);
            return Err(StatusCode::BAD_GATEWAY);
        }
    };
    
    // Convert response format
    let claude_response = match state.converter.convert_response(openai_response, &original_model) {
        Ok(response) => {
            // 🔍 DEBUG: 记录最终返回给客户端的Claude响应
            if let Ok(claude_json) = serde_json::to_string_pretty(&response) {
                info!("📋 Final Claude Response:\n{}", claude_json);
            }
            response
        },
        Err(e) => {
            error!("Response conversion failed: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    
    info!("Request processing completed");
    
    Ok(Json(claude_response).into_response())
}

/// Handle streaming requests
async fn handle_stream_request(
    state: Arc<AppState>,
    mut openai_request: OpenAIRequest,
    original_model: String,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Handling streaming request");
    
    // Ensure it's a streaming request
    openai_request.stream = Some(true);
    
    // Clone necessary components to avoid lifetime issues
    let openai_client = state.openai_client.clone();
    let converter = state.converter.clone();
    
    // Create converted stream
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, axum::Error>>(100);
    
    // Handle streaming data conversion in background task
    tokio::spawn(async move {
        // Get streaming response
        let openai_stream = match openai_client.inner().chat_completions_stream(openai_request).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("OpenAI streaming API request failed: {}", e);
                let error_event = Event::default()
                    .event("error")
                    .data(format!("{{\"error\": \"{}\"}}", e));
                let _ = tx.send(Ok(error_event)).await;
                return;
            }
        };
        let mut stream = Box::pin(openai_stream);
        
        while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
            match chunk_result {
                Ok(openai_chunk) => {
                    // Convert OpenAI streaming response to Claude format
                    match converter.convert_stream_chunk(openai_chunk, &original_model) {
                        Ok(claude_events) => {
                            // Send each converted event
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
                                        let error_event = Event::default()
                                            .event("error")
                                            .data(format!("{{\"error\": \"{}\"}}", e));
                                        let _ = tx.send(Ok(error_event)).await;
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Streaming response conversion failed: {}", e);
                            let error_event = Event::default()
                                .event("error")
                                .data(format!("{{\"error\": \"{}\"}}", e));
                            let _ = tx.send(Ok(error_event)).await;
                            return;
                        }
                    }
                }
                Err(e) => {
                    error!("OpenAI streaming response error: {}", e);
                    let error_event = Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e));
                    let _ = tx.send(Ok(error_event)).await;
                    return;
                }
            }
        }
        
        // Send end event
        let end_event = Event::default().event("done").data("{}");
        let _ = tx.send(Ok(end_event)).await;
    });
    
    // Create SSE response
    let stream = ReceiverStream::new(rx);
    let sse = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive")
        );
    
    info!("Starting streaming response transmission");
    
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
        
        // 🔍 DEBUG: 详细记录消息验证信息
        debug!("Validating message {}: role={}, content_text_len={}, has_images={}, has_tool_calls={}", 
               i, message.role, content_text.len(), has_images, has_tool_calls);
        debug!("Message {} content type: {:?}", i, message.content);
        
        // Only reject if content is completely empty (no text, images, or tool calls)
        // 🔧 修复工具调用验证：允许只包含工具调用的消息
        if content_text.is_empty() && !has_images && !has_tool_calls {
            warn!("Message {} validation failed - completely empty content: text_empty={}, no_images={}, no_tool_calls={}", 
                  i, content_text.is_empty(), !has_images, !has_tool_calls);
            warn!("Complete message {} details: role='{}', content={:#?}", i, message.role, message.content);
            warn!("Full request context: model='{}', messages_count={}, max_tokens={}", 
                  request.model, request.messages.len(), request.max_tokens);
            return Err(format!("Message {} content cannot be empty", i));
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
fn create_error_response(error_type: &str, message: &str) -> Json<serde_json::Value> {
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
    
    Json(error_response)
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