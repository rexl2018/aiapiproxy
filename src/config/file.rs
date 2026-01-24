//! File-based configuration loading
//!
//! Loads provider and model configuration from JSON file

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Listen host (default: "127.0.0.1" - localhost only)
    #[serde(default = "default_host")]
    pub host: String,
    
    /// Listen port (default: 8082)
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8082
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

/// Application configuration loaded from JSON file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration (optional, defaults to localhost:8082)
    #[serde(default)]
    pub server: ServerConfig,
    
    /// Provider configurations
    pub providers: HashMap<String, ProviderConfig>,
    
    /// Claude model to provider/model mapping
    /// Maps Claude model names (e.g., "claude-3-sonnet-20240620") to provider/model paths
    #[serde(rename = "modelMapping", default)]
    pub model_mapping: HashMap<String, String>,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type (e.g., "openai", "modelhub")
    #[serde(rename = "type")]
    pub provider_type: String,
    
    /// Base URL for the provider API
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    
    /// API key (can be empty if using env var)
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    
    /// Provider-specific options
    #[serde(default)]
    pub options: ProviderOptions,
    
    /// Model configurations for this provider
    pub models: HashMap<String, ModelConfig>,
}

/// Provider-specific options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderOptions {
    /// API key parameter name (for URL query parameter auth)
    #[serde(rename = "apiKeyParam", skip_serializing_if = "Option::is_none")]
    pub api_key_param: Option<String>,
    
    /// Mode for modelhub provider ("responses" or "gemini")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    
    /// Custom headers to add to requests
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model name to use with the upstream provider
    pub name: String,
    
    /// Optional alias for the model (used in request routing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    
    /// Maximum tokens limit for this model
    #[serde(rename = "maxTokens", skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    
    /// Default temperature for this model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    
    /// Model-specific options
    #[serde(default)]
    pub options: ModelOptions,
}

/// Model-specific options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelOptions {
    /// Whether this model supports streaming
    #[serde(rename = "supportsStreaming", default = "default_true")]
    pub supports_streaming: bool,
    
    /// Whether this model supports tool use
    #[serde(rename = "supportsTools", default = "default_true")]
    pub supports_tools: bool,
    
    /// Whether this model supports vision/images
    #[serde(rename = "supportsVision", default)]
    pub supports_vision: bool,
}

fn default_true() -> bool {
    true
}

impl AppConfig {
    /// Load configuration from JSON file
    pub fn load(path: &Path) -> Result<Self> {
        info!("Loading configuration from: {:?}", path);
        
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        
        let config: AppConfig = serde_json::from_str(&content)
            .with_context(|| "Failed to parse config JSON")?;
        
        config.validate()?;
        
        debug!("Loaded {} providers", config.providers.len());
        Ok(config)
    }
    
    /// Load configuration from default locations
    /// Searches in order:
    /// 1. ~/.config/aiapiproxy/aiapiproxy.json
    /// 2. ./aiapiproxy.json
    /// 
    /// Returns error if no configuration file is found.
    pub fn load_default() -> Result<Self> {
        // Try home config directory first
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config").join("aiapiproxy").join("aiapiproxy.json");
            if config_path.exists() {
                return Self::load(&config_path);
            }
        }
        
        // Try current directory
        let local_path = Path::new("aiapiproxy.json");
        if local_path.exists() {
            return Self::load(local_path);
        }
        
        anyhow::bail!(
            "Configuration file not found. Please create one at:\n\
             - ~/.config/aiapiproxy/aiapiproxy.json (recommended)\n\
             - ./aiapiproxy.json (current directory)\n\
             \n\
             See aiapiproxy.example.json for reference."
        )
    }
    
    /// Validate configuration
    fn validate(&self) -> Result<()> {
        if self.providers.is_empty() {
            anyhow::bail!("At least one provider must be configured");
        }
        
        for (name, provider) in &self.providers {
            // Validate provider type
            let valid_types = ["openai", "modelhub", "anthropic"];
            if !valid_types.contains(&provider.provider_type.as_str()) {
                anyhow::bail!("Invalid provider type '{}' for provider '{}'", provider.provider_type, name);
            }
            
            // Validate base URL
            if !provider.base_url.starts_with("http") {
                anyhow::bail!("Invalid base URL for provider '{}': {}", name, provider.base_url);
            }
            
            // Validate models
            if provider.models.is_empty() {
                anyhow::bail!("Provider '{}' must have at least one model configured", name);
            }
            
            for (model_name, model_config) in &provider.models {
                if model_config.name.is_empty() {
                    anyhow::bail!("Model '{}' in provider '{}' must have a name", model_name, name);
                }
            }
            
            // Validate modelhub-specific options
            if provider.provider_type == "modelhub" {
                if let Some(mode) = &provider.options.mode {
                    let valid_modes = ["responses", "gemini"];
                    if !valid_modes.contains(&mode.as_str()) {
                        anyhow::bail!("Invalid mode '{}' for modelhub provider '{}'. Valid modes: {:?}", mode, name, valid_modes);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get provider and model configuration by path (e.g., "provider/model")
    pub fn get_provider_model(&self, path: &str) -> Option<(&ProviderConfig, &ModelConfig)> {
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() != 2 {
            return None;
        }
        
        let provider_name = parts[0];
        let model_name = parts[1];
        
        let provider = self.providers.get(provider_name)?;
        let model = provider.models.get(model_name)?;
        
        Some((provider, model))
    }
    
    /// Resolve a Claude model name to provider/model path
    /// 
    /// Returns the mapped path if found in modelMapping, otherwise returns None
    pub fn resolve_claude_model(&self, claude_model: &str) -> Option<&str> {
        // First check exact match in modelMapping
        if let Some(path) = self.model_mapping.get(claude_model) {
            return Some(path.as_str());
        }
        
        // Check pattern matching (e.g., "sonnet" matches any model containing "sonnet")
        let model_lower = claude_model.to_lowercase();
        for (pattern, path) in &self.model_mapping {
            let pattern_lower = pattern.to_lowercase();
            if model_lower.contains(&pattern_lower) || pattern_lower.contains(&model_lower) {
                return Some(path.as_str());
            }
        }
        
        None
    }
    
    /// List all available model paths
    pub fn list_model_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for (provider_name, provider) in &self.providers {
            for model_name in provider.models.keys() {
                paths.push(format!("{}/{}", provider_name, model_name));
            }
        }
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    fn create_test_config() -> String {
        r#"{
            "providers": {
                "openai": {
                    "type": "openai",
                    "baseUrl": "https://api.openai.com/v1",
                    "apiKey": "",
                    "models": {
                        "gpt-4o": {
                            "name": "gpt-4o",
                            "maxTokens": 8192
                        },
                        "gpt-4o-mini": {
                            "name": "gpt-4o-mini",
                            "maxTokens": 4096
                        }
                    }
                },
                "modelhub-sg1": {
                    "type": "modelhub",
                    "baseUrl": "https://modelhub-sg1.example.com",
                    "apiKey": "",
                    "options": {
                        "apiKeyParam": "ak",
                        "mode": "responses"
                    },
                    "models": {
                        "gpt-5": {
                            "name": "gpt-5",
                            "maxTokens": 32768
                        }
                    }
                },
                "modelhub-gemini": {
                    "type": "modelhub",
                    "baseUrl": "https://modelhub-gemini.example.com",
                    "apiKey": "",
                    "options": {
                        "apiKeyParam": "ak",
                        "mode": "gemini"
                    },
                    "models": {
                        "gemini-2.5-pro": {
                            "name": "gemini-2.5-pro",
                            "maxTokens": 65536,
                            "options": {
                                "supportsVision": true
                            }
                        }
                    }
                }
            },
            "modelMapping": {
                "claude-3-sonnet": "modelhub-sg1/gpt-5",
                "claude-3-opus": "openai/gpt-4o",
                "sonnet": "modelhub-sg1/gpt-5"
            }
        }"#.to_string()
    }
    
    #[test]
    fn test_load_config() {
        let config_str = create_test_config();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let config = AppConfig::load(file.path()).unwrap();
        
        assert_eq!(config.providers.len(), 3);
        assert!(config.providers.contains_key("openai"));
        assert!(config.providers.contains_key("modelhub-sg1"));
        assert!(config.providers.contains_key("modelhub-gemini"));
    }
    
    #[test]
    fn test_get_provider_model() {
        let config_str = create_test_config();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let config = AppConfig::load(file.path()).unwrap();
        
        // Test valid path
        let result = config.get_provider_model("openai/gpt-4o");
        assert!(result.is_some());
        let (provider, model) = result.unwrap();
        assert_eq!(provider.provider_type, "openai");
        assert_eq!(model.name, "gpt-4o");
        
        // Test modelhub provider
        let result = config.get_provider_model("modelhub-sg1/gpt-5");
        assert!(result.is_some());
        let (provider, model) = result.unwrap();
        assert_eq!(provider.provider_type, "modelhub");
        assert_eq!(provider.options.mode, Some("responses".to_string()));
        assert_eq!(model.name, "gpt-5");
        
        // Test invalid path
        assert!(config.get_provider_model("invalid").is_none());
        assert!(config.get_provider_model("openai/nonexistent").is_none());
    }
    
    #[test]
    fn test_list_model_paths() {
        let config_str = create_test_config();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let config = AppConfig::load(file.path()).unwrap();
        let paths = config.list_model_paths();
        
        assert!(paths.contains(&"openai/gpt-4o".to_string()));
        assert!(paths.contains(&"openai/gpt-4o-mini".to_string()));
        assert!(paths.contains(&"modelhub-sg1/gpt-5".to_string()));
        assert!(paths.contains(&"modelhub-gemini/gemini-2.5-pro".to_string()));
    }
    
    #[test]
    fn test_validation_empty_providers() {
        let config_str = r#"{"providers": {}}"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let result = AppConfig::load(file.path());
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validation_invalid_provider_type() {
        let config_str = r#"{
            "providers": {
                "test": {
                    "type": "invalid_type",
                    "baseUrl": "https://example.com",
                    "models": {
                        "model1": {"name": "model1"}
                    }
                }
            }
        }"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let result = AppConfig::load(file.path());
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validation_invalid_mode() {
        let config_str = r#"{
            "providers": {
                "test": {
                    "type": "modelhub",
                    "baseUrl": "https://example.com",
                    "options": {
                        "mode": "invalid_mode"
                    },
                    "models": {
                        "model1": {"name": "model1"}
                    }
                }
            }
        }"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let result = AppConfig::load(file.path());
        assert!(result.is_err());
    }
    
    #[test]
    fn test_resolve_claude_model() {
        let config_str = create_test_config();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_str.as_bytes()).unwrap();
        
        let config = AppConfig::load(file.path()).unwrap();
        
        // Exact match
        assert_eq!(config.resolve_claude_model("claude-3-sonnet"), Some("modelhub-sg1/gpt-5"));
        assert_eq!(config.resolve_claude_model("claude-3-opus"), Some("openai/gpt-4o"));
        
        // Pattern match (contains)
        assert_eq!(config.resolve_claude_model("claude-3-sonnet-20240620"), Some("modelhub-sg1/gpt-5"));
        
        // Short alias
        assert_eq!(config.resolve_claude_model("sonnet"), Some("modelhub-sg1/gpt-5"));
        
        // Not found
        assert!(config.resolve_claude_model("unknown-model").is_none());
    }
}
