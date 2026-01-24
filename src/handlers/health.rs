//! Health check handlers
//! 
//! Provides application health status check endpoints

use crate::handlers::AppState;
use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Service name
    pub service: String,
    /// Version information
    pub version: String,
    /// Timestamp
    pub timestamp: String,
    /// Details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HealthDetails>,
}

/// Check result
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthDetails {
    /// OpenAI API connection status
    pub openai_api: String,
    /// Configuration status
    pub config: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Memory usage (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<MemoryUsage>,
}

/// Memory usage information
#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryUsage {
    /// Used memory in bytes
    pub used_bytes: u64,
    /// Total memory in bytes
    pub total_bytes: u64,
    /// Usage percentage
    pub usage_percent: f64,
}

/// Basic health check
/// 
/// Returns basic service status information
pub async fn health_check(State(_state): State<Arc<AppState>>) -> Json<HealthResponse> {
    debug!("Executing health check");
    
    let response = HealthResponse {
        status: "healthy".to_string(),
        service: "AI API Proxy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        details: Some(HealthDetails {
            openai_api: "checking".to_string(),
            config: "valid".to_string(),
            uptime_seconds: get_uptime_seconds(),
            memory_usage: get_memory_usage(),
        }),
    };
    
    Json(response)
}

/// Readiness check
/// 
/// GET /health/ready
/// Check if the service is ready to receive requests
pub async fn readiness_check(State(state): State<Arc<AppState>>) -> Result<Json<HealthResponse>, StatusCode> {
    debug!("Executing readiness check");
    
    // Check router status (providers configured)
    let provider_count = state.router.list_models().len();
    let provider_status = if provider_count > 0 {
        format!("{} models available", provider_count)
    } else {
        "no models configured".to_string()
    };
    
    // Check configuration
    let config_status = "valid".to_string(); // Configuration validated at startup
    
    // Calculate uptime
    let uptime_seconds = get_uptime_seconds();
    
    // Get memory usage
    let memory_usage = get_memory_usage();
    
    let details = HealthDetails {
        openai_api: provider_status.clone(),
        config: config_status,
        uptime_seconds,
        memory_usage,
    };
    
    // Determine overall status
    let overall_status = if provider_count > 0 {
        "ready".to_string()
    } else {
        "not_ready".to_string()
    };
    
    let response = HealthResponse {
        status: overall_status.clone(),
        service: "aiapiproxy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        details: Some(details),
    };
    
    // Return 503 status code if service is not ready
    if overall_status == "not_ready" {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    
    Ok(Json(response))
}

/// Liveness check
/// 
/// GET /health/live
/// Check if the service is still running
pub async fn liveness_check(State(_state): State<Arc<AppState>>) -> Result<Json<HealthResponse>, StatusCode> {
    debug!("Executing liveness check");
    
    // Liveness check only needs to confirm the service is running
    // Does not check external dependencies
    
    let uptime_seconds = get_uptime_seconds();
    let memory_usage = get_memory_usage();
    
    let details = HealthDetails {
        openai_api: "not_checked".to_string(),
        config: "valid".to_string(),
        uptime_seconds,
        memory_usage,
    };
    
    let response = HealthResponse {
        status: "alive".to_string(),
        service: "aiapiproxy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        details: Some(details),
    };
    
    Ok(Json(response))
}

/// Get service uptime in seconds
fn get_uptime_seconds() -> u64 {
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};
    
    static START_TIME: OnceLock<u64> = OnceLock::new();
    
    let start_time = *START_TIME.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    });
    
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    current_time.saturating_sub(start_time)
}

/// Get memory usage information
fn get_memory_usage() -> Option<MemoryUsage> {
    // In production environment, system calls can be used to get real memory usage
    // This provides a simplified implementation
    
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        
        // Read /proc/self/status to get memory information
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            let mut vm_rss = None;
            let mut vm_size = None;
            
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            vm_rss = Some(kb * 1024); // Convert to bytes
                        }
                    }
                } else if line.starts_with("VmSize:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            vm_size = Some(kb * 1024); // Convert to bytes
                        }
                    }
                }
            }
            
            if let (Some(used), Some(total)) = (vm_rss, vm_size) {
                let usage_percent = if total > 0 {
                    (used as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                
                return Some(MemoryUsage {
                    used_bytes: used,
                    total_bytes: total,
                    usage_percent,
                });
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // Memory information retrieval on macOS is complex, returning None here
        // In actual projects, system calls or third-party libraries can be used
    }
    
    #[cfg(target_os = "windows")]
    {
        // Memory information retrieval on Windows, returning None here
        // In actual projects, Windows API can be used
    }
    
    // Return None if unable to get real memory information
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{settings::*, AppConfig, ModelConfig, ProviderConfig};
    use crate::services::{ApiConverter, Router};
    use std::collections::HashMap;
    use std::sync::Arc;
    
    fn create_test_config() -> AppConfig {
        let mut models = HashMap::new();
        models.insert("gpt-4o".to_string(), ModelConfig {
            name: "gpt-4o".to_string(),
            alias: None,
            max_tokens: Some(8192),
            temperature: None,
            options: Default::default(),
        });
        
        let mut providers = HashMap::new();
        providers.insert("openai".to_string(), ProviderConfig {
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test_key".to_string(),
            options: Default::default(),
            models,
        });
        
        AppConfig { 
            providers,
            model_mapping: HashMap::new(),
        }
    }
    
    fn create_test_state() -> Arc<AppState> {
        let settings = Settings {
            server: ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            openai: OpenAIConfig {
                api_key: "test_key".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                timeout: 30,
                stream_timeout: 300,
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
        };
        
        let converter = ApiConverter::new(settings.clone());
        let router = Arc::new(Router::new(create_test_config()).unwrap());
        
        Arc::new(AppState {
            settings,
            converter,
            router,
        })
    }
    
    #[tokio::test]
    async fn test_health_check() {
        let state = create_test_state();
        let result = health_check(State(state)).await;
        
        let response = result.0;
        assert_eq!(response.status, "healthy");
        assert_eq!(response.service, "AI API Proxy");
    }
    
    #[tokio::test]
    async fn test_liveness_check() {
        let state = create_test_state();
        let result = liveness_check(State(state)).await;
        
        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert_eq!(response.status, "alive");
        assert!(response.details.is_some());
    }
    
    #[test]
    fn test_uptime_calculation() {
        let uptime1 = get_uptime_seconds();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let uptime2 = get_uptime_seconds();
        
        // The second call's uptime should be greater than or equal to the first
        assert!(uptime2 >= uptime1);
    }
}