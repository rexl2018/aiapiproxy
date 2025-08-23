//! Integration test module
//!
//! Contains end-to-end tests and integration tests

#[cfg(test)]
mod integration_tests {
    use crate::config::Settings;
    use crate::handlers::create_router;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    
    /// Create test settings
    pub fn create_test_settings() -> Settings {
        // Set test environment variables
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-for-testing");
        std::env::set_var("SERVER_PORT", "8083");
        std::env::set_var("RUST_LOG", "debug");
        
        Settings::new().expect("Failed to create test settings")
    }
    
    #[tokio::test]
    async fn test_health_check() {
        let settings = create_test_settings();
        let app = create_router(settings).await.expect("Failed to create router");
        
        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_root_endpoint() {
        let settings = create_test_settings();
        let app = create_router(settings).await.expect("Failed to create router");
        
        let request = Request::builder()
            .uri("/")
            .body(Body::empty())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_messages_endpoint_without_auth() {
        let settings = create_test_settings();
        let app = create_router(settings).await.expect("Failed to create router");
        
        let request = Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("content-type", "application/json")
            .body(Body::from(r#"{
                "model": "claude-3-sonnet",
                "max_tokens": 100,
                "messages": [
                    {
                        "role": "user",
                        "content": "Hello"
                    }
                ]
            }"#))
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        
        // Should return 401 Unauthorized because no authentication header is provided
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}