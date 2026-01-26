//! Provider module
//!
//! Defines the Provider trait and provider implementations

pub mod ark;
pub mod modelhub;
pub mod openai;

use crate::config::{ModelConfig, ProviderConfig};
use crate::models::openai::{OpenAIRequest, OpenAIResponse, OpenAIStreamResponse};
use anyhow::Result;
use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;

/// A boxed stream of streaming responses
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = Result<T>> + Send + 'a>>;

/// Provider trait for upstream API providers
///
/// All providers must implement this trait to support both
/// streaming and non-streaming chat completion requests.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;
    
    /// Send a chat completion request (non-streaming)
    async fn chat_complete(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<OpenAIResponse>;
    
    /// Send a chat completion request (streaming)
    async fn chat_stream(
        &self,
        request: OpenAIRequest,
        provider_config: &ProviderConfig,
        model_config: &ModelConfig,
    ) -> Result<BoxStream<'static, OpenAIStreamResponse>>;
}

pub use ark::ArkProvider;
pub use modelhub::ModelHubProvider;
pub use openai::OpenAIProvider;
