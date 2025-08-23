//! AI API Proxy Server
//! 
//! HTTP proxy service that converts Claude API requests to OpenAI API format

use anyhow::{Context, Result};
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod handlers;
mod middleware;
mod models;
mod services;
mod utils;

use config::Settings;
use handlers::create_router;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    init_logging();
    
    // Load configuration
    let settings = Settings::new().context("Failed to load configuration")?;
    info!("Configuration loaded successfully: {:?}", settings);
    
    // Create router
    let app = create_router(settings.clone()).await?;
    
    // Build server address
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    info!("Server starting at: http://{}", addr);
    
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