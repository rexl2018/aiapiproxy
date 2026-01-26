//! Request Router
//!
//! Routes requests to appropriate providers based on model path

use crate::config::{AppConfig, ModelConfig, ProviderConfig};
use crate::models::openai::{OpenAIRequest, OpenAIResponse, OpenAIStreamResponse};
use crate::providers::{ArkProvider, BoxStream, ModelHubProvider, OpenAIProvider, Provider};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Request Router
///
/// Holds provider instances and routes requests based on model path
pub struct Router {
    /// Application configuration
    config: AppConfig,
    /// Provider instances by type
    providers: HashMap<String, Arc<dyn Provider>>,
}

impl Router {
    /// Create a new router from configuration
    pub fn new(config: AppConfig) -> Result<Self> {
        let mut providers: HashMap<String, Arc<dyn Provider>> = HashMap::new();
        
        // Initialize required provider types based on config
        for provider_config in config.providers.values() {
            let provider_type = &provider_config.provider_type;
            
            if !providers.contains_key(provider_type) {
                let provider: Arc<dyn Provider> = match provider_type.as_str() {
                    "openai" => Arc::new(OpenAIProvider::new()?),
                    "modelhub" => Arc::new(ModelHubProvider::new()?),
                    "ark" => Arc::new(ArkProvider::new()?),
                    "anthropic" => {
                        // For anthropic type, we can use OpenAI provider with custom URL
                        // as the API format is handled by the converter
                        Arc::new(OpenAIProvider::new()?)
                    }
                    _ => {
                        warn!("Unknown provider type: {}, using OpenAI provider", provider_type);
                        Arc::new(OpenAIProvider::new()?)
                    }
                };
                
                providers.insert(provider_type.clone(), provider);
            }
        }
        
        info!("Router initialized with {} provider types", providers.len());
        
        Ok(Self { config, providers })
    }
    
    /// Route a model path to provider and model config
    ///
    /// Model path format: "{provider}/{model}" (e.g., "openai/gpt-4o", "modelhub-sg1/gpt-5")
    pub fn route(&self, model_path: &str) -> Option<(Arc<dyn Provider>, &ProviderConfig, &ModelConfig)> {
        // Split model path into provider and model
        let (provider_config, model_config) = self.config.get_provider_model(model_path)?;
        
        // Get provider instance by type
        let provider = self.providers.get(&provider_config.provider_type)?;
        
        debug!("Routed {} to provider type: {}", model_path, provider_config.provider_type);
        
        Some((provider.clone(), provider_config, model_config))
    }
    
    /// Parse model field from request and extract provider/model path
    ///
    /// Resolution order:
    /// 1. If model contains '/', treat as provider/model path directly
    /// 2. Check Claude model mapping (e.g., "claude-3-sonnet" -> "modelhub-sg1/gpt-5")
    /// 3. Search for model name in all providers
    /// 4. Search for model alias in all providers
    pub fn resolve_model(&self, model: &str) -> Option<String> {
        // 1. If already in provider/model format
        if model.contains('/') {
            if self.config.get_provider_model(model).is_some() {
                return Some(model.to_string());
            }
        }
        
        // 2. Check Claude model mapping
        if let Some(mapped_path) = self.config.resolve_claude_model(model) {
            if self.config.get_provider_model(mapped_path).is_some() {
                debug!("Mapped Claude model '{}' to '{}'", model, mapped_path);
                return Some(mapped_path.to_string());
            }
        }
        
        // 3. Search for model in all providers by exact name
        for (provider_name, provider_config) in &self.config.providers {
            if provider_config.models.contains_key(model) {
                return Some(format!("{}/{}", provider_name, model));
            }
        }
        
        // 4. Search for model by alias
        for (provider_name, provider_config) in &self.config.providers {
            for (model_key, model_config) in &provider_config.models {
                if model_config.alias.as_deref() == Some(model) {
                    return Some(format!("{}/{}", provider_name, model_key));
                }
            }
        }
        
        None
    }
    
    /// Chat completion (non-streaming)
    pub async fn chat_complete(&self, mut request: OpenAIRequest) -> Result<OpenAIResponse> {
        let model_path = self.resolve_model(&request.model)
            .with_context(|| format!("Model not found: {}", request.model))?;
        
        let (provider, provider_config, model_config) = self.route(&model_path)
            .with_context(|| format!("Failed to route model: {}", model_path))?;
        
        debug!("Processing chat completion for model: {}", model_path);
        
        // Update request model to the resolved path for tracking
        request.model = model_path;
        
        provider.chat_complete(request, provider_config, model_config).await
    }
    
    /// Chat completion (streaming)
    pub async fn chat_stream(&self, mut request: OpenAIRequest) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        let model_path = self.resolve_model(&request.model)
            .with_context(|| format!("Model not found: {}", request.model))?;
        
        let (provider, provider_config, model_config) = self.route(&model_path)
            .with_context(|| format!("Failed to route model: {}", model_path))?;
        
        debug!("Processing streaming chat completion for model: {}", model_path);
        
        // Update request model to the resolved path for tracking
        request.model = model_path;
        
        provider.chat_stream(request, provider_config, model_config).await
    }
    
    /// List all available model paths
    pub fn list_models(&self) -> Vec<String> {
        self.config.list_model_paths()
    }
    
    /// Get the underlying configuration
    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::config::{ModelConfig, ProviderConfig, ProviderOptions};
    
    fn create_test_config() -> AppConfig {
        let mut providers = HashMap::new();
        
        // OpenAI provider
        let mut openai_models = HashMap::new();
        openai_models.insert("gpt-4o".to_string(), ModelConfig {
            name: "gpt-4o".to_string(),
            alias: Some("gpt4".to_string()),
            max_tokens: Some(8192),
            temperature: None,
            options: Default::default(),
        });
        
        providers.insert("openai".to_string(), ProviderConfig {
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "".to_string(),
            options: Default::default(),
            models: openai_models,
        });
        
        // ModelHub provider
        let mut modelhub_models = HashMap::new();
        modelhub_models.insert("gpt-5".to_string(), ModelConfig {
            name: "gpt-5".to_string(),
            alias: None,
            max_tokens: Some(32768),
            temperature: None,
            options: Default::default(),
        });
        
        providers.insert("modelhub-sg1".to_string(), ProviderConfig {
            provider_type: "modelhub".to_string(),
            base_url: "https://modelhub-sg1.example.com".to_string(),
            api_key: "".to_string(),
            options: ProviderOptions {
                api_key_param: Some("ak".to_string()),
                mode: Some("responses".to_string()),
                headers: Default::default(),
            },
            models: modelhub_models,
        });
        
        AppConfig { 
            server: crate::config::ServerConfig::default(),
            providers,
            model_mapping: HashMap::new(),
        }
    }
    
    #[test]
    fn test_router_creation() {
        let config = create_test_config();
        let router = Router::new(config);
        assert!(router.is_ok());
    }
    
    #[test]
    fn test_resolve_model_with_path() {
        let config = create_test_config();
        let router = Router::new(config).unwrap();
        
        // Full path
        let result = router.resolve_model("openai/gpt-4o");
        assert_eq!(result, Some("openai/gpt-4o".to_string()));
        
        // ModelHub path
        let result = router.resolve_model("modelhub-sg1/gpt-5");
        assert_eq!(result, Some("modelhub-sg1/gpt-5".to_string()));
    }
    
    #[test]
    fn test_resolve_model_without_path() {
        let config = create_test_config();
        let router = Router::new(config).unwrap();
        
        // Just model name
        let result = router.resolve_model("gpt-4o");
        assert_eq!(result, Some("openai/gpt-4o".to_string()));
        
        // Alias
        let result = router.resolve_model("gpt4");
        assert_eq!(result, Some("openai/gpt-4o".to_string()));
    }
    
    #[test]
    fn test_resolve_model_not_found() {
        let config = create_test_config();
        let router = Router::new(config).unwrap();
        
        let result = router.resolve_model("nonexistent-model");
        assert!(result.is_none());
    }
    
    #[test]
    fn test_route() {
        let config = create_test_config();
        let router = Router::new(config).unwrap();
        
        // Test routing
        let result = router.route("openai/gpt-4o");
        assert!(result.is_some());
        let (_, provider_config, model_config) = result.unwrap();
        assert_eq!(provider_config.provider_type, "openai");
        assert_eq!(model_config.name, "gpt-4o");
        
        // Test modelhub routing
        let result = router.route("modelhub-sg1/gpt-5");
        assert!(result.is_some());
        let (_, provider_config, model_config) = result.unwrap();
        assert_eq!(provider_config.provider_type, "modelhub");
        assert_eq!(model_config.name, "gpt-5");
    }
    
    #[test]
    fn test_list_models() {
        let config = create_test_config();
        let router = Router::new(config).unwrap();
        
        let models = router.list_models();
        assert!(models.contains(&"openai/gpt-4o".to_string()));
        assert!(models.contains(&"modelhub-sg1/gpt-5".to_string()));
    }
}
