//! Authentication middleware
//! 
//! Handles API key validation and access control

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    body::Body,
};
use std::sync::Arc;
use tracing::{debug, warn};

/// Authentication middleware
/// 
/// Validates API keys in requests
pub async fn auth_middleware(
    State(state): State<Arc<crate::handlers::AppState>>,
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Executing authentication middleware");
    
    // Get request path
    let path = request.uri().path();
    
    // Skip authentication for health check endpoints
    if path.starts_with("/health") || path == "/" {
        return Ok(next.run(request).await);
    }
    
    // Get authentication header
    let auth_header = headers
        .get(&state.settings.security.api_key_header)
        .and_then(|h| h.to_str().ok());
    
    // Validate authentication
    match auth_header {
        Some(token) => {
            if validate_api_key(token) {
                debug!("Authentication successful");
                Ok(next.run(request).await)
            } else {
                warn!("Invalid API key");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => {
            warn!("Missing authentication header: {}", state.settings.security.api_key_header);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Validate API key
/// 
/// More complex validation logic can be implemented here
pub fn validate_api_key(api_key: &str) -> bool {
    // Basic format validation
    if api_key.is_empty() {
        return false;
    }
    
    // Remove Bearer prefix if present
    let token = if api_key.starts_with("Bearer ") {
        &api_key[7..]
    } else {
        api_key
    };
    
    // Validate token format
    validate_token_format(token)
}

/// Validate token format
pub fn validate_token_format(token: &str) -> bool {
    // Claude API keys usually start with "sk-ant-"
    // OpenAI API keys usually start with "sk-"
    // This implements generic validation logic
    
    // Check minimum length
    if token.len() < 8 {
        return false;
    }
    
    // Check for invalid characters (spaces, newlines, etc.)
    if token.contains(char::is_whitespace) {
        return false;
    }
    
    // More validation rules can be added here
    // For example: check specific prefixes, length ranges, etc.
    
    true
}

/// Rate limiting middleware (optional)
/// 
/// Rate limiting based on IP address or API key
pub async fn rate_limit_middleware(
    State(_state): State<Arc<crate::handlers::AppState>>,
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Result<Response<axum::body::Body>, StatusCode> {
    debug!("Executing rate limit check");
    
    // Get client identifier
    let client_id = get_client_identifier(&headers, &request);
    
    // Check if rate limit is exceeded
    if is_rate_limited(&client_id).await {
        warn!("Client {} exceeded rate limit", client_id);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    
    // Record request
    record_request(&client_id).await;
    
    Ok(next.run(request).await)
}

/// Get client identifier
pub fn get_client_identifier(headers: &HeaderMap, _request: &Request<Body>) -> String {
    // Prioritize using API key as identifier
    if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(key_part) = auth_header.split_whitespace().last() {
            // Only use first few characters of key as identifier (privacy protection)
            if key_part.len() > 10 {
                return format!("key_{}", &key_part[..10]);
            }
        }
    }
    
    // Fallback to IP address
    if let Some(forwarded_for) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = forwarded_for.split(',').next() {
            return format!("ip_{}", ip.trim());
        }
    }
    
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        return format!("ip_{}", real_ip);
    }
    
    // Default identifier
    "unknown".to_string()
}

/// Check if rate limit is triggered
async fn is_rate_limited(client_id: &str) -> bool {
    // Real rate limiting logic should be implemented here
    // Can use Redis, memory cache or other storage solutions
    // Currently returns false (no limiting)
    
    // Example implementation: using simple memory counter
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    
    static RATE_LIMITER: std::sync::LazyLock<Mutex<HashMap<String, (Instant, u32)>>> = 
        std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
    
    let mut limiter = RATE_LIMITER.lock().unwrap();
    let now = Instant::now();
    let window = Duration::from_secs(60); // 1 minute window
    let max_requests = 100; // Maximum 100 requests per minute
    
    match limiter.get_mut(client_id) {
        Some((last_reset, count)) => {
            if now.duration_since(*last_reset) > window {
                // Reset counter
                *last_reset = now;
                *count = 1;
                false
            } else if *count >= max_requests {
                // Exceeded limit
                true
            } else {
                // Increment count
                *count += 1;
                false
            }
        }
        None => {
            // New client
            limiter.insert(client_id.to_string(), (now, 1));
            false
        }
    }
}

/// Record request
async fn record_request(client_id: &str) {
    debug!("Recording client request: {}", client_id);
    // Request statistics logic can be implemented here
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_api_key() {
        // Valid Bearer token
        assert!(validate_api_key("Bearer sk-ant-api03-1234567890abcdef"));
        assert!(validate_api_key("Bearer sk-1234567890abcdef"));
        
        // Valid direct API key
        assert!(validate_api_key("sk-ant-api03-1234567890abcdef"));
        assert!(validate_api_key("sk-1234567890abcdef"));
        
        // Invalid API key
        assert!(!validate_api_key(""));
        assert!(!validate_api_key("invalid"));
        assert!(!validate_api_key("sk-"));
        assert!(!validate_api_key("Bearer "));
        assert!(!validate_api_key("Bearer invalid key with spaces"));
    }
    
    #[test]
    fn test_validate_token_format() {
        // Claude API key
        assert!(validate_token_format("sk-ant-api03-1234567890abcdef"));
        
        // OpenAI API key
        assert!(validate_token_format("sk-1234567890abcdef"));
        
        // Other formats
        assert!(validate_token_format("custom_api_key_123"));
        
        // Invalid formats
        assert!(!validate_token_format("sk-"));
        assert!(!validate_token_format("short"));
        assert!(!validate_token_format("key with spaces"));
    }
    
    #[test]
    fn test_get_client_identifier() {
        use axum::http::HeaderMap;
        
        let mut headers = HeaderMap::new();
        let request = Request::builder().body(Body::empty()).unwrap();
        
        // Test API key identifier
        headers.insert("authorization", "Bearer sk-1234567890abcdef".parse().unwrap());
        let id = get_client_identifier(&headers, &request);
        assert!(id.starts_with("key_"));
        
        // Test IP address identifier
        headers.clear();
        headers.insert("x-forwarded-for", "192.168.1.1, 10.0.0.1".parse().unwrap());
        let id = get_client_identifier(&headers, &request);
        assert_eq!(id, "ip_192.168.1.1");
        
        // Test default identifier
        headers.clear();
        let id = get_client_identifier(&headers, &request);
        assert_eq!(id, "unknown");
    }
}