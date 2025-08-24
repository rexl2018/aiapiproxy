//! Application configuration settings
//! 
//! Defines all configuration structures and loading logic

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use tracing::warn;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Server configuration
    pub server: ServerConfig,
    /// OpenAI API configuration
    pub openai: OpenAIConfig,
    /// Model mapping configuration
    pub model_mapping: ModelMapping,
    /// Request configuration
    pub request: RequestConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Listen host
    pub host: String,
    /// Listen port
    pub port: u16,
}

/// OpenAI API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key
    pub api_key: String,
    /// API base URL
    pub base_url: String,
    /// Request timeout in seconds
    pub timeout: u64,
}

/// Model mapping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMapping {
    /// OpenAI model corresponding to Claude Haiku
    pub haiku: String,
    /// OpenAI model corresponding to Claude Sonnet
    pub sonnet: String,
    /// OpenAI model corresponding to Claude Opus
    pub opus: String,
    /// Custom model mappings
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

/// Request configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    /// Maximum request size in bytes
    pub max_request_size: usize,
    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,
    /// Request timeout in seconds
    pub timeout: u64,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Allowed origins for CORS
    pub allowed_origins: Vec<String>,
    /// API key header name
    pub api_key_header: String,
    /// Whether CORS is enabled
    pub cors_enabled: bool,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format (text/json)
    pub format: String,
}

impl Settings {
    /// Create a new configuration instance
    pub fn new() -> Result<Self> {
        // Load .env file if it exists
        dotenv::dotenv().ok();
        
        let settings = Self {
            server: ServerConfig {
                host: get_env_or_default("SERVER_HOST", "0.0.0.0"),
                port: get_env_or_default("SERVER_PORT", "8082")
                    .parse()
                    .context("Invalid port number")?,
            },
            openai: OpenAIConfig {
                api_key: std::env::var("OPENAI_API_KEY")
                    .context("OPENAI_API_KEY environment variable not set")?,
                base_url: get_env_or_default("OPENAI_BASE_URL", "https://api.openai.com/v1"),
                timeout: get_env_or_default("REQUEST_TIMEOUT", "30")
                    .parse()
                    .context("Invalid timeout value")?,
            },
            model_mapping: ModelMapping {
                haiku: get_env_or_default("CLAUDE_HAIKU_MODEL", "gpt-4o-mini"),
                sonnet: get_env_or_default("CLAUDE_SONNET_MODEL", "gpt-4o"),
                opus: get_env_or_default("CLAUDE_OPUS_MODEL", "gpt-4"),
                custom: HashMap::new(),
            },
            request: RequestConfig {
                max_request_size: get_env_or_default("MAX_REQUEST_SIZE", "10485760")
                    .parse()
                    .context("Invalid maximum request size")?,
                max_concurrent_requests: get_env_or_default("MAX_CONCURRENT_REQUESTS", "100")
                    .parse()
                    .context("Invalid maximum concurrent requests")?,
                timeout: get_env_or_default("REQUEST_TIMEOUT", "30")
                    .parse()
                    .context("Invalid request timeout")?,
            },
            security: SecurityConfig {
                allowed_origins: get_env_or_default("ALLOWED_ORIGINS", "*")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect(),
                api_key_header: get_env_or_default("API_KEY_HEADER", "Authorization"),
                cors_enabled: get_env_or_default("CORS_ENABLED", "true")
                    .parse()
                    .context("Invalid CORS enabled flag")?,
            },
            logging: LoggingConfig {
                level: get_env_or_default("RUST_LOG", "info"),
                format: get_env_or_default("LOG_FORMAT", "text"),
            },
        };
        
        // Validate configuration
        settings.validate()?;
        
        Ok(settings)
    }
    
    /// Validate configuration validity
    fn validate(&self) -> Result<()> {
        // Validate port range
        if self.server.port == 0 {
            anyhow::bail!("Port number cannot be 0");
        }
        
        // Validate API key format - accept various formats for different providers
        if self.openai.api_key.is_empty() {
            anyhow::bail!("OpenAI API key cannot be empty");
        }
        
        // Basic format validation - ensure no whitespace and minimum length
        if self.openai.api_key.contains(char::is_whitespace) {
            anyhow::bail!("OpenAI API key cannot contain whitespace characters");
        }
        
        if self.openai.api_key.len() < 8 {
            anyhow::bail!("OpenAI API key must be at least 8 characters long");
        }
        
        // Validate URL format
        if !self.openai.base_url.starts_with("http") {
            anyhow::bail!("Invalid OpenAI base URL format, should start with 'http'");
        }
        
        // Validate timeout values
        if self.openai.timeout == 0 || self.request.timeout == 0 {
            anyhow::bail!("Timeout values cannot be 0");
        }
        
        // Validate request size limit
        if self.request.max_request_size == 0 {
            anyhow::bail!("Maximum request size cannot be 0");
        }
        
        // Validate concurrent request count
        if self.request.max_concurrent_requests == 0 {
            anyhow::bail!("Maximum concurrent requests cannot be 0");
        }
        
        // Validate log level
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            anyhow::bail!("Invalid log level: {}", self.logging.level);
        }
        
        // Validate log format
        let valid_formats = ["text", "json"];
        if !valid_formats.contains(&self.logging.format.as_str()) {
            anyhow::bail!("Invalid log format: {}", self.logging.format);
        }
        
        Ok(())
    }
    
    /// Get corresponding OpenAI model name
    pub fn get_openai_model(&self, claude_model: &str) -> Option<String> {
        match claude_model {
            // 🔧 更通用的模型匹配：基于模型名称包含的关键词
            model if model.contains("haiku") => Some(self.model_mapping.haiku.clone()),
            model if model.contains("sonnet") => Some(self.model_mapping.sonnet.clone()),
            model if model.contains("opus") => Some(self.model_mapping.opus.clone()),
            _ => {
                // Check custom mappings
                if let Some(mapped_model) = self.model_mapping.custom.get(claude_model) {
                    Some(mapped_model.clone())
                } else {
                    // Default to sonnet model
                    warn!("Unknown Claude model: {}, using default sonnet model", claude_model);
                    Some(self.model_mapping.sonnet.clone())
                }
            }
        }
    }
    
    /// Check if in development mode
    pub fn is_dev_mode(&self) -> bool {
        std::env::var("DEV_MODE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false)
    }
}

/// Get environment variable or default value
fn get_env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_model_mapping() {
        let settings = Settings {
            server: ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            openai: OpenAIConfig {
                api_key: "sk-test".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                timeout: 30,
            },
            model_mapping: ModelMapping {
                haiku: "gpt-4o-mini".to_string(),
                sonnet: "gpt-4o".to_string(),
                opus: "gpt-4".to_string(),
                custom: HashMap::new(),
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
        
        assert_eq!(settings.get_openai_model("claude-3-haiku"), Some("gpt-4o-mini".to_string()));
        assert_eq!(settings.get_openai_model("claude-3-sonnet"), Some("gpt-4o".to_string()));
        assert_eq!(settings.get_openai_model("claude-3-opus"), Some("gpt-4".to_string()));
    }
}