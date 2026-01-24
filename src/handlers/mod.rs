//! HTTP handlers module
//! 
//! Contains all HTTP endpoint handling logic

pub mod health;
pub mod proxy;

use crate::config::{AppConfig, Settings};
use crate::services::{ApiConverter, Router as ProviderRouter};
use anyhow::Result;
use axum::{routing::get, routing::post, Router};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;

/// Application state
#[derive(Clone)]
pub struct AppState {
    /// Server settings (from env vars)
    pub settings: Settings,
    /// API converter (Claude <-> OpenAI format conversion)
    pub converter: ApiConverter,
    /// Provider router for multi-provider support
    pub router: Arc<ProviderRouter>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("settings", &self.settings)
            .field("converter", &"ApiConverter")
            .field("router", &"ProviderRouter")
            .finish()
    }
}

/// Create application router with JSON config
pub async fn create_router(settings: Settings, app_config: AppConfig) -> Result<Router> {
    info!("Initializing with {} providers:", app_config.providers.len());
    for (name, provider) in &app_config.providers {
        let model_count = provider.models.len();
        let mode = provider.options.mode.as_deref().unwrap_or("default");
        info!("  - {}: type={}, mode={}, models={}", name, provider.provider_type, mode, model_count);
    }
    
    // Create API converter
    let converter = ApiConverter::new(settings.clone());
    
    // Create provider router
    let router = Arc::new(ProviderRouter::new(app_config)?);
    
    // Create application state
    let app_state = Arc::new(AppState {
        settings: settings.clone(),
        converter,
        router,
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

