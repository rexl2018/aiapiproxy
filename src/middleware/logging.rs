//! Logging middleware
//! 
//! Records HTTP request and response information

use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

/// Request logging middleware
/// 
/// Records detailed information for each HTTP request
pub async fn request_logging_middleware(
    State(_state): State<Arc<crate::handlers::AppState>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let start_time = Instant::now();
    let request_id = Uuid::new_v4().to_string();
    
    // Create request span
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %uri.path(),
        query = %uri.query().unwrap_or(""),
    );
    
    let _enter = span.enter();
    
    // Log request start
    info!(
        "Request started: {} {} - User-Agent: {}",
        method,
        uri,
        headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
    );
    
    // Log request body size (if any)
    if let Some(content_length) = headers.get("content-length") {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<u64>() {
                info!("Request body size: {} bytes", length);
            }
        }
    }
    
    // Execute request
    let response = next.run(request).await;
    
    // Calculate processing time
    let duration = start_time.elapsed();
    let status = response.status();
    
    // Log response
    if status.is_success() {
        info!(
            "Request completed: {} - Duration: {:.2}ms",
            status,
            duration.as_secs_f64() * 1000.0
        );
    } else if status.is_client_error() {
        warn!(
            "Client error: {} - Duration: {:.2}ms",
            status,
            duration.as_secs_f64() * 1000.0
        );
    } else if status.is_server_error() {
        warn!(
            "Server error: {} - Duration: {:.2}ms",
            status,
            duration.as_secs_f64() * 1000.0
        );
    } else {
        info!(
            "Request response: {} - Duration: {:.2}ms",
            status,
            duration.as_secs_f64() * 1000.0
        );
    }
    
    // Log slow requests
    if duration.as_secs() > 5 {
        warn!(
            "Slow request detected: {} {} - Duration: {:.2}s",
            method,
            uri,
            duration.as_secs_f64()
        );
    }
    
    response
}

/// Error logging middleware
/// 
/// Capture and log unhandled errors
pub async fn error_logging_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    
    let response = next.run(request).await;
    
    // Check response status code
    if response.status().is_server_error() {
        tracing::error!(
            "Server internal error: {} {} - Status code: {}",
            method,
            uri,
            response.status()
        );
    }
    
    Ok(response)
}

/// Security logging middleware
/// 
/// Log security-related events
pub async fn security_logging_middleware(
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    
    // Check suspicious request headers
    check_suspicious_headers(&headers, &method, &uri);
    
    // Check suspicious paths
    check_suspicious_paths(&uri);
    
    // Execute request
    let response = next.run(request).await;
    
    // Log authentication failures
    if response.status() == StatusCode::UNAUTHORIZED {
        warn!(
            "Authentication failed: {} {} - IP: {}",
            method,
            uri,
            get_client_ip(&headers).unwrap_or("unknown".to_string())
        );
    }
    
    response
}

/// Check suspicious request headers
fn check_suspicious_headers(headers: &HeaderMap, method: &Method, uri: &Uri) {
    // Check SQL injection attempts
    for (name, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            let value_lower = value_str.to_lowercase();
            if value_lower.contains("union select") 
                || value_lower.contains("drop table")
                || value_lower.contains("<script")
                || value_lower.contains("javascript:") {
                warn!(
                    "Suspicious header detected: {} {} - Header: {} = {}",
                    method, uri, name, value_str
                );
            }
        }
    }
    
    // Check abnormal User-Agent
    if let Some(user_agent) = headers.get("user-agent").and_then(|v| v.to_str().ok()) {
        if user_agent.len() > 1000 || user_agent.contains("sqlmap") || user_agent.contains("nmap") {
            warn!(
                "Suspicious User-Agent detected: {} {} - User-Agent: {}",
                method, uri, user_agent
            );
        }
    }
}

/// Check suspicious paths
fn check_suspicious_paths(uri: &Uri) {
    let path = uri.path();
    
    // Check common attack paths
    let suspicious_patterns = [
        "../",
        "..%2f",
        ".env",
        "wp-admin",
        "admin",
        "phpmyadmin",
        ".git",
        "config",
        "backup",
        "sql",
        "dump",
    ];
    
    for pattern in &suspicious_patterns {
        if path.to_lowercase().contains(pattern) {
            warn!(
                "Suspicious path access detected: {} - Pattern: {}",
                path,
                pattern
            );
            break;
        }
    }
}

/// Get client IP address
fn get_client_ip(headers: &HeaderMap) -> Option<String> {
    // Check different IP headers by priority
    let ip_headers = [
        "x-forwarded-for",
        "x-real-ip",
        "x-client-ip",
        "cf-connecting-ip", // Cloudflare
        "x-cluster-client-ip",
    ];
    
    for header_name in &ip_headers {
        if let Some(header_value) = headers.get(*header_name) {
            if let Ok(ip_str) = header_value.to_str() {
                // X-Forwarded-For may contain multiple IPs, take the first one
                if let Some(first_ip) = ip_str.split(',').next() {
                    let ip = first_ip.trim();
                    if !ip.is_empty() && ip != "unknown" {
                        return Some(ip.to_string());
                    }
                }
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    
    #[test]
    fn test_get_client_ip() {
        let mut headers = HeaderMap::new();
        
        // Test X-Forwarded-For
        headers.insert("x-forwarded-for", "192.168.1.1, 10.0.0.1".parse().unwrap());
        assert_eq!(get_client_ip(&headers), Some("192.168.1.1".to_string()));
        
        // Test X-Real-IP
        headers.clear();
        headers.insert("x-real-ip", "192.168.1.2".parse().unwrap());
        assert_eq!(get_client_ip(&headers), Some("192.168.1.2".to_string()));
        
        // Test no IP headers
        headers.clear();
        assert_eq!(get_client_ip(&headers), None);
    }
    
    #[test]
    fn test_check_suspicious_paths() {
        use axum::http::Uri;
        
        // Normal path
        let uri: Uri = "/v1/messages".parse().unwrap();
        check_suspicious_paths(&uri); // Should not produce warning
        
        // Suspicious path
        let uri: Uri = "/../etc/passwd".parse().unwrap();
        check_suspicious_paths(&uri); // Should produce warning
    }
}