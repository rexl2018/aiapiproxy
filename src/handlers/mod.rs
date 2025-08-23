//! HTTP handlers module
//! 
//! Contains all HTTP endpoint handling logic

pub mod health;
pub mod proxy;

use crate::config::Settings;
use crate::services::{ApiConverter, RetryableOpenAIClient};
use anyhow::Result;
use axum::{routing::get, routing::post, Router};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

/// Application state
#[derive(Debug, Clone)]
pub struct AppState {
    pub settings: Settings,
    pub openai_client: RetryableOpenAIClient,
    pub converter: ApiConverter,
}

/// Create application router
pub async fn create_router(settings: Settings) -> Result<Router> {
    // Create OpenAI client
    let openai_client = RetryableOpenAIClient::new(settings.clone(), None)?;
    
    // Create API converter
    let converter = ApiConverter::new(settings.clone());
    
    // Create application state
    let app_state = Arc::new(AppState {
        settings: settings.clone(),
        openai_client,
        converter,
    });
    
    // Create middleware stack
    let middleware_stack = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    
    // Create routes
    let router = Router::new()
        .route("/v1/messages", post(proxy::handle_messages))
        .route("/health", get(health::health_check))
        .route("/health/live", get(health::liveness_check))
        .with_state(app_state)
        .layer(middleware_stack);
    
    Ok(router)
}

