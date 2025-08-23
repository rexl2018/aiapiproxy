//! Configuration module unit tests

use aiapiproxy::config::settings::{Settings, ServerConfig, OpenAIConfig, ModelMapping, RequestConfig, SecurityConfig, LoggingConfig};
use std::collections::HashMap;
use std::env;

/// Setup test environment variables
fn setup_test_env() {
    env::set_var("OPENAI_API_KEY", "sk-test-key-12345678901234567890");
    env::set_var("SERVER_HOST", "127.0.0.1");
    env::set_var("SERVER_PORT", "8080");
    env::set_var("OPENAI_BASE_URL", "https://api.openai.com/v1");
    env::set_var("CLAUDE_HAIKU_MODEL", "gpt-4o-mini");
    env::set_var("CLAUDE_SONNET_MODEL", "gpt-4o");
    env::set_var("CLAUDE_OPUS_MODEL", "gpt-4");
    env::set_var("REQUEST_TIMEOUT", "30");
    env::set_var("MAX_REQUEST_SIZE", "10485760");
    env::set_var("MAX_CONCURRENT_REQUESTS", "100");
    env::set_var("RUST_LOG", "info");
    env::set_var("LOG_FORMAT", "text");
    env::set_var("ALLOWED_ORIGINS", "*");
    env::set_var("CORS_ENABLED", "true");
}

/// Clean up test environment variables
fn cleanup_test_env() {
    let vars = [
        "OPENAI_API_KEY", "SERVER_HOST", "SERVER_PORT", "OPENAI_BASE_URL",
        "CLAUDE_HAIKU_MODEL", "CLAUDE_SONNET_MODEL", "CLAUDE_OPUS_MODEL",
        "REQUEST_TIMEOUT", "MAX_REQUEST_SIZE", "MAX_CONCURRENT_REQUESTS",
        "RUST_LOG", "LOG_FORMAT", "ALLOWED_ORIGINS", "CORS_ENABLED"
    ];
    
    for var in &vars {
        env::remove_var(var);
    }
}

#[test]
fn test_settings_creation_with_valid_env() {
    setup_test_env();
    
    let settings = Settings::new();
    assert!(settings.is_ok());
    
    let settings = settings.unwrap();
    assert_eq!(settings.server.host, "127.0.0.1");
    assert_eq!(settings.server.port, 8080);
    assert_eq!(settings.openai.api_key, "sk-test-key-12345678901234567890");
    assert_eq!(settings.openai.base_url, "https://api.openai.com/v1");
    assert_eq!(settings.model_mapping.haiku, "gpt-4o-mini");
    assert_eq!(settings.model_mapping.sonnet, "gpt-4o");
    assert_eq!(settings.model_mapping.opus, "gpt-4");
    
    cleanup_test_env();
}

#[test]
fn test_settings_creation_missing_api_key() {
    cleanup_test_env();
    env::set_var("SERVER_PORT", "8080");
    
    let settings = Settings::new();
    assert!(settings.is_err());
    
    let error = settings.unwrap_err();
    assert!(error.to_string().contains("OPENAI_API_KEY"));
    
    cleanup_test_env();
}

#[test]
fn test_settings_validation_invalid_port() {
    setup_test_env();
    env::set_var("SERVER_PORT", "0");
    
    let settings = Settings::new();
    assert!(settings.is_err());
    
    let error = settings.unwrap_err();
    assert!(error.to_string().contains("Port cannot be 0"));
    
    cleanup_test_env();
}

#[test]
fn test_settings_validation_invalid_api_key() {
    setup_test_env();
    env::set_var("OPENAI_API_KEY", "invalid-key");
    
    let settings = Settings::new();
    assert!(settings.is_err());
    
    let error = settings.unwrap_err();
    assert!(error.to_string().contains("Invalid API key format"));
    
    cleanup_test_env();
}

#[test]
fn test_settings_validation_invalid_url() {
    setup_test_env();
    env::set_var("OPENAI_BASE_URL", "invalid-url");
    
    let settings = Settings::new();
    assert!(settings.is_err());
    
    let error = settings.unwrap_err();
    assert!(error.to_string().contains("Invalid URL format"));
    
    cleanup_test_env();
}

#[test]
fn test_settings_validation_invalid_timeout() {
    setup_test_env();
    env::set_var("REQUEST_TIMEOUT", "0");
    
    let settings = Settings::new();
    assert!(settings.is_err());
    
    let error = settings.unwrap_err();
    assert!(error.to_string().contains("Timeout cannot be 0"));
    
    cleanup_test_env();
}

#[test]
fn test_settings_validation_invalid_log_level() {
    setup_test_env();
    env::set_var("RUST_LOG", "invalid");
    
    let settings = Settings::new();
    assert!(settings.is_err());
    
    let error = settings.unwrap_err();
    assert!(error.to_string().contains("Invalid log level"));
    
    cleanup_test_env();
}

#[test]
fn test_get_openai_model_mapping() {
    let mut custom_mapping = HashMap::new();
    custom_mapping.insert("claude-custom".to_string(), "gpt-custom".to_string());
    
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
            custom: custom_mapping,
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
    
    // Test predefined mappings
    assert_eq!(settings.get_openai_model("claude-3-haiku"), Some("gpt-4o-mini".to_string()));
    assert_eq!(settings.get_openai_model("claude-3-sonnet"), Some("gpt-4o".to_string()));
    assert_eq!(settings.get_openai_model("claude-3-opus"), Some("gpt-4".to_string()));
    
    // Test custom mappings
    assert_eq!(settings.get_openai_model("claude-custom"), Some("gpt-custom".to_string()));
    
    // Test unknown model (should return default sonnet model)
    assert_eq!(settings.get_openai_model("unknown-model"), Some("gpt-4o".to_string()));
}

#[test]
fn test_is_dev_mode() {
    // Test development mode off
    env::remove_var("DEV_MODE");
    let settings = create_test_settings();
    assert!(!settings.is_dev_mode());
    
    // Test development mode on
    env::set_var("DEV_MODE", "true");
    let settings = create_test_settings();
    assert!(settings.is_dev_mode());
    
    // Test development mode value as false
    env::set_var("DEV_MODE", "false");
    let settings = create_test_settings();
    assert!(!settings.is_dev_mode());
    
    env::remove_var("DEV_MODE");
}

#[test]
fn test_default_values() {
    cleanup_test_env();
    env::set_var("OPENAI_API_KEY", "sk-test-key-12345678901234567890");
    
    let settings = Settings::new().unwrap();
    
    // Check default values
    assert_eq!(settings.server.host, "0.0.0.0");
    assert_eq!(settings.server.port, 8082);
    assert_eq!(settings.openai.base_url, "https://api.openai.com/v1");
    assert_eq!(settings.openai.timeout, 30);
    assert_eq!(settings.model_mapping.haiku, "gpt-4o-mini");
    assert_eq!(settings.model_mapping.sonnet, "gpt-4o");
    assert_eq!(settings.model_mapping.opus, "gpt-4");
    assert_eq!(settings.request.max_request_size, 10485760);
    assert_eq!(settings.request.max_concurrent_requests, 100);
    assert_eq!(settings.request.timeout, 30);
    assert_eq!(settings.security.allowed_origins, vec!["*".to_string()]);
    assert_eq!(settings.security.api_key_header, "Authorization");
    assert!(settings.security.cors_enabled);
    assert_eq!(settings.logging.level, "info");
    assert_eq!(settings.logging.format, "text");
    
    cleanup_test_env();
}

fn create_test_settings() -> Settings {
    setup_test_env();
    Settings::new().unwrap()
}

#[test]
fn test_parse_errors() {
    setup_test_env();
    
    // Test invalid port number
    env::set_var("SERVER_PORT", "invalid");
    let settings = Settings::new();
    assert!(settings.is_err());
    assert!(settings.unwrap_err().to_string().contains("Invalid port number"));
    
    // Test invalid timeout
    env::set_var("SERVER_PORT", "8080");
    env::set_var("REQUEST_TIMEOUT", "invalid");
    let settings = Settings::new();
    assert!(settings.is_err());
    assert!(settings.unwrap_err().to_string().contains("Invalid timeout"));
    
    // Test invalid request size
    env::set_var("REQUEST_TIMEOUT", "30");
    env::set_var("MAX_REQUEST_SIZE", "invalid");
    let settings = Settings::new();
    assert!(settings.is_err());
    assert!(settings.unwrap_err().to_string().contains("Invalid max request size"));
    
    cleanup_test_env();
}