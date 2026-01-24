//! AI API Proxy Server
//! 
//! HTTP proxy service that converts Claude API requests to OpenAI API format
//! with multi-provider routing via JSON configuration

use anyhow::{Context, Result};
use tracing::info;

mod config;
mod handlers;
mod middleware;
mod models;
mod providers;
mod services;
mod utils;

use config::{AppConfig, Settings};
use handlers::create_router;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    init_logging();
    
    // Load provider configuration from JSON file (required)
    let app_config = AppConfig::load_default()
        .context("Failed to load provider configuration")?;
    
    info!("📁 Provider configuration loaded");
    
    // Load additional settings from environment (for logging, security, etc.)
    let settings = Settings::new().context("Failed to load server settings")?;
    info!("Server settings loaded");
    
    // Create router
    let app = create_router(settings.clone(), app_config.clone()).await?;
    
    // Build server address from JSON config
    let addr = format!("{}:{}", app_config.server.host, app_config.server.port);
    
    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    info!("🚀 AI API Proxy server started!");
    info!("📝 Health check: http://{}/health", addr);
    info!("🔄 Proxy endpoint: http://{}/v1/messages", addr);
    
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start server: {}", e))?;
    
    Ok(())
}

/// Initialize logging system
fn init_logging() {
    // Get log level from environment variable, default to info
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    
    // Check if JSON format should be used
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    
    let subscriber: Box<dyn tracing::Subscriber + Send + Sync> = if log_format == "json" {
        // JSON format logs (production environment)
        Box::new(tracing_subscriber::fmt()
            .with_env_filter(log_level)
            .json()
            .with_current_span(false)
            .with_span_list(false)
            .finish())
    } else {
        // Human readable format (development environment)
        Box::new(tracing_subscriber::fmt()
            .with_env_filter(log_level)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .finish())
    };
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
    
    info!("Logging system initialized");
}