//! OpenAI Provider implementation
//!
//! Standard OpenAI-compatible API provider

use super::{BoxStream, Provider};
use crate::config::{ModelConfig, ProviderConfig};
use crate::models::openai::*;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{debug, error, warn};

/// OpenAI Provider
pub struct OpenAIProvider {
    client: Client,
    stream_client: Client,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with default timeouts
    pub fn new() -> Result<Self> {
        Self::with_timeouts(30, 300)
    }
    
    /// Create a new OpenAI provider with custom timeouts
    pub fn with_timeouts(timeout_secs: u64, stream_timeout_secs: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("aiapiproxy/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;
        
        let stream_client = Client::builder()
            .timeout(Duration::from_secs(stream_timeout_secs))
            .user_agent("aiapiproxy/0.1.0")
            .build()
            .context("Failed to create streaming HTTP client")?;
        
        Ok(Self { client, stream_client })
    }
    
    /// Build the request URL
    fn build_url(&self, provider_config: &ProviderConfig) -> String {
        let base_url = provider_config.base_url.trim_end_matches('/');
        format!("{}/chat/completions", base_url)
    }
    
    /// Build authorization header value
    fn get_auth_header(&self, provider_config: &ProviderConfig) -> String {
        let api_key = if provider_config.api_key.is_empty() {
            std::env::var("OPENAI_API_KEY").unwrap_or_default()
        } else {
            provider_config.api_key.clone()
        };
        format!("Bearer {}", api_key)
    }
    
    /// Parse SSE chunk from bytes
    fn parse_sse_chunk(&self, chunk: &[u8]) -> Result<Option<OpenAIStreamResponse>> {
        let chunk_str = std::str::from_utf8(chunk)
            .context("Invalid UTF-8 data")?;
        
        for line in chunk_str.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    debug!("Received streaming response end marker");
                    return Ok(None);
                }
                
                match serde_json::from_str::<OpenAIStreamResponse>(data) {
                    Ok(stream_response) => {
                        debug!("Successfully parsed streaming response chunk");
                        return Ok(Some(stream_response));
                    }
                    Err(e) => {
                        warn!("Failed to parse streaming response chunk: {} - data: {}", e, data);
                    }
                }
            }
        }
        
        Ok(None)
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }
    
    async fn chat_complete(
        &self,
        mut request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse> {
        debug!("Sending OpenAI chat completion request");
        
        // Override model name with provider's model name
        request.model = model_config.name.clone();
        
        // Apply model-specific settings if not already set
        if request.max_tokens.is_none() {
            request.max_tokens = model_config.max_tokens;
        }
        if request.temperature.is_none() {
            request.temperature = model_config.temperature;
        }
        
        let url = self.build_url(provider_config);
        let auth = self.get_auth_header(provider_config);
        
        let response = self.client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;
        
        let status = response.status();
        
        if status.is_success() {
            let openai_response: OpenAIResponse = response
                .json()
                .await
                .context("Failed to parse OpenAI response")?;
            
            debug!("OpenAI request completed successfully");
            Ok(openai_response)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            
            if let Ok(error_response) = serde_json::from_str::<OpenAIErrorResponse>(&error_text) {
                error!("OpenAI API error: {:?}", error_response.error);
                anyhow::bail!("OpenAI API error: {}", error_response.error.message);
            } else {
                error!("OpenAI API request failed: {} - {}", status, error_text);
                anyhow::bail!("OpenAI API request failed: {} - {}", status, error_text);
            }
        }
    }
    
    async fn chat_stream(
        &self,
        mut request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>> {
        debug!("Sending OpenAI streaming chat completion request");
        
        // Override model name with provider's model name
        request.model = model_config.name.clone();
        request.stream = Some(true);
        
        // Apply model-specific settings if not already set
        if request.max_tokens.is_none() {
            request.max_tokens = model_config.max_tokens;
        }
        if request.temperature.is_none() {
            request.temperature = model_config.temperature;
        }
        
        let url = self.build_url(provider_config);
        let auth = self.get_auth_header(provider_config);
        
        let response = self.stream_client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API request failed: {} - {}", status, error_text);
        }
        
        let stream = response
            .bytes_stream()
            .filter_map(move |chunk_result| {
                match chunk_result {
                    Ok(chunk) => {
                        match std::str::from_utf8(&chunk) {
                            Ok(chunk_str) => {
                                for line in chunk_str.lines() {
                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if data.trim() == "[DONE]" {
                                            return None;
                                        }
                                        
                                        match serde_json::from_str::<OpenAIStreamResponse>(data) {
                                            Ok(stream_response) => {
                                                return Some(Ok(stream_response));
                                            }
                                            Err(e) => {
                                                warn!("Failed to parse streaming response chunk: {}", e);
                                            }
                                        }
                                    }
                                }
                                None
                            }
                            Err(e) => Some(Err(anyhow::anyhow!("Invalid UTF-8: {}", e))),
                        }
                    }
                    Err(e) => Some(Err(anyhow::anyhow!("Stream error: {}", e))),
                }
            });
        
        Ok(Box::pin(stream))
    }
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new().expect("Failed to create default OpenAI provider")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_provider_creation() {
        let provider = OpenAIProvider::new();
        assert!(provider.is_ok());
    }
    
    #[test]
    fn test_provider_name() {
        let provider = OpenAIProvider::new().unwrap();
        assert_eq!(provider.name(), "openai");
    }
    
    #[test]
    fn test_build_url() {
        let provider = OpenAIProvider::new().unwrap();
        
        let config = ProviderConfig {
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "".to_string(),
            options: Default::default(),
            models: Default::default(),
        };
        
        let url = provider.build_url(&config);
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
        
        // Test with trailing slash
        let config2 = ProviderConfig {
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com/v1/".to_string(),
            api_key: "".to_string(),
            options: Default::default(),
            models: Default::default(),
        };
        
        let url2 = provider.build_url(&config2);
        assert_eq!(url2, "https://api.openai.com/v1/chat/completions");
    }
    
    #[test]
    fn test_parse_sse_chunk() {
        let provider = OpenAIProvider::new().unwrap();
        
        // Test normal SSE data
        let sse_data = b"data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n";
        let result = provider.parse_sse_chunk(sse_data).unwrap();
        assert!(result.is_some());
        
        // Test end marker
        let done_data = b"data: [DONE]\n\n";
        let result = provider.parse_sse_chunk(done_data).unwrap();
        assert!(result.is_none());
    }
}
