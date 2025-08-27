//! HTTP client service
//! 
//! Encapsulates HTTP communication with OpenAI API

use crate::config::Settings;
use crate::models::openai::*;
use anyhow::{Context, Result};
use reqwest::{Client, Response};
use std::time::Duration;
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error, info, warn};

/// OpenAI API client
#[derive(Debug, Clone)]
pub struct OpenAIClient {
    client: Client,
    stream_client: Client,
    settings: Settings,
}

impl OpenAIClient {
    /// Create a new client instance
    pub fn new(settings: Settings) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(settings.openai.timeout))
            .user_agent("aiapiproxy/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;
        
        let stream_client = Client::builder()
            .timeout(Duration::from_secs(settings.openai.stream_timeout))
            .user_agent("aiapiproxy/0.1.0")
            .build()
            .context("Failed to create streaming HTTP client")?;
        
        Ok(Self { client, stream_client, settings })
    }
    
    /// Send chat completion request
    pub async fn chat_completions(&self, request: OpenAIRequest) -> Result<OpenAIResponse> {
        debug!("Sending OpenAI chat completion request");
        
        let url = format!("{}/chat/completions", self.settings.openai.base_url);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.settings.openai.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;
        
        self.handle_response(response).await
    }
    
    /// Send streaming chat completion request
    pub async fn chat_completions_stream(
        &self,
        request: OpenAIRequest,
    ) -> Result<impl Stream<Item = Result<OpenAIStreamResponse>> + '_> {
        debug!("Sending OpenAI streaming chat completion request");
        
        let url = format!("{}/chat/completions", self.settings.openai.base_url);
        
        let response = self.stream_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.settings.openai.api_key))
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
            .map(|chunk_result| {
                chunk_result
                    .context("Failed to read streaming response chunk")
                    .and_then(|chunk| self.parse_sse_chunk(&chunk))
            })
            .filter_map(|result| {
                match result {
                    Ok(Some(event)) => Some(Ok(event)),
                    Ok(None) => None, // Skip empty events or non-data events
                    Err(e) => Some(Err(e)),
                }
            });
        
        Ok(stream)
    }
    
    /// Handle HTTP response
    async fn handle_response(&self, response: Response) -> Result<OpenAIResponse> {
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
            
            // Try to parse as OpenAI error format
            if let Ok(error_response) = serde_json::from_str::<OpenAIErrorResponse>(&error_text) {
                error!("OpenAI API error: {:?}", error_response.error);
                anyhow::bail!("OpenAI API error: {}", error_response.error.message);
            } else {
                error!("OpenAI API request failed: {} - {}", status, error_text);
                anyhow::bail!("OpenAI API request failed: {} - {}", status, error_text);
            }
        }
    }
    
    /// Parse Server-Sent Events (SSE) data chunk
    fn parse_sse_chunk(&self, chunk: &[u8]) -> Result<Option<OpenAIStreamResponse>> {
        let chunk_str = std::str::from_utf8(chunk)
            .context("Invalid UTF-8 data")?;
        
        // SSE format: "data: {json}\n\n"
        for line in chunk_str.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                // Check if it's the end marker
                if data.trim() == "[DONE]" {
                    debug!("Received streaming response end marker");
                    return Ok(None);
                }
                
                // Try to parse JSON data
                match serde_json::from_str::<OpenAIStreamResponse>(data) {
                    Ok(stream_response) => {
                        debug!("Successfully parsed streaming response chunk");
                        return Ok(Some(stream_response));
                    }
                    Err(e) => {
                        warn!("Failed to parse streaming response chunk: {} - data: {}", e, data);
                        // Continue processing next line, don't return error
                    }
                }
            }
        }
        
        // If no valid data found, return None
        Ok(None)
    }
    
    /// Check API connection
    pub async fn health_check(&self) -> Result<bool> {
        debug!("Performing OpenAI API health check");
        
        // Send a simple request to check connection
        let test_request = OpenAIRequest {
            model: "gpt-3.5-turbo".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: Some(OpenAIContent::Text("test".to_string())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: Some(1),
            ..Default::default()
        };
        
        match self.chat_completions(test_request).await {
            Ok(_) => {
                info!("OpenAI API health check passed");
                Ok(true)
            }
            Err(e) => {
                warn!("OpenAI API health check failed: {}", e);
                Ok(false)
            }
        }
    }
    
    /// Get available models list (optional feature)
    pub async fn list_models(&self) -> Result<Vec<String>> {
        debug!("Getting OpenAI available models list");
        
        let url = format!("{}/models", self.settings.openai.base_url);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.settings.openai.api_key))
            .send()
            .await
            .context("Failed to get models list")?;
        
        if response.status().is_success() {
            let models_response: serde_json::Value = response
                .json()
                .await
                .context("Failed to parse models list response")?;
            
            let models: Vec<String> = models_response["data"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|model| model["id"].as_str().map(|s| s.to_string()))
                .collect();
            
            debug!("Successfully retrieved {} models", models.len());
            Ok(models)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get models list: {}", error_text);
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Base delay time (milliseconds)
    pub base_delay_ms: u64,
    /// Maximum delay time (milliseconds)
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10000,
        }
    }
}

/// Client wrapper with retry functionality
#[derive(Debug, Clone)]
pub struct RetryableOpenAIClient {
    client: OpenAIClient,
    retry_config: RetryConfig,
}

impl RetryableOpenAIClient {
    /// Create client with retry functionality
    pub fn new(settings: Settings, retry_config: Option<RetryConfig>) -> Result<Self> {
        let client = OpenAIClient::new(settings)?;
        let retry_config = retry_config.unwrap_or_default();
        
        Ok(Self {
            client,
            retry_config,
        })
    }
    
    /// Chat completion request with retry
    pub async fn chat_completions_with_retry(&self, request: OpenAIRequest) -> Result<OpenAIResponse> {
        let mut last_error = None;
        
        for attempt in 0..=self.retry_config.max_retries {
            match self.client.chat_completions(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);
                    
                    if attempt < self.retry_config.max_retries {
                        let delay = std::cmp::min(
                            self.retry_config.base_delay_ms * (2_u64.pow(attempt)),
                            self.retry_config.max_delay_ms,
                        );
                        
                        warn!("Request failed, retrying after {}ms (attempt {}/{})", delay, attempt + 1, self.retry_config.max_retries);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap())
    }
    
    /// Get inner client reference
    pub fn inner(&self) -> &OpenAIClient {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::*;
    
    fn create_test_settings() -> Settings {
        Settings {
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
        }
    }
    
    #[test]
    fn test_client_creation() {
        let settings = create_test_settings();
        let client = OpenAIClient::new(settings);
        assert!(client.is_ok());
    }
    
    #[test]
    fn test_sse_parsing() {
        let settings = create_test_settings();
        let client = OpenAIClient::new(settings).unwrap();
        
        // Test normal SSE data
        let sse_data = b"data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":1234567890,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n";
        let result = client.parse_sse_chunk(sse_data).unwrap();
        assert!(result.is_some());
        
        // Test end marker
        let done_data = b"data: [DONE]\n\n";
        let result = client.parse_sse_chunk(done_data).unwrap();
        assert!(result.is_none());
    }
    
    #[test]
    fn test_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 10000);
    }
}