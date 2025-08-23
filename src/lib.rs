//! AI API Proxy Library
//! 
//! Provides Claude API to OpenAI API conversion functionality

pub mod config;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;
pub mod utils;

// Re-export common types
pub use config::Settings;
pub use handlers::{create_router, AppState};
pub use models::{claude, openai};
pub use services::{ApiConverter, OpenAIClient, RetryableOpenAIClient};
pub use utils::error::{AppError, AppResult};

/// Library version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Library description
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// Get version information
pub fn version_info() -> String {
    format!("{} v{} - {}", NAME, VERSION, DESCRIPTION)
}