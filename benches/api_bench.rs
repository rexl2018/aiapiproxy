//! API endpoint performance benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use aiapiproxy::config::settings::Settings;
use aiapiproxy::handlers::create_router;
use aiapiproxy::models::claude::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json;
use std::env;
use tower::ServiceExt;

/// Setup test environment
fn setup_test_env() {
    env::set_var("OPENAI_API_KEY", "sk-test-key-for-benchmarking-1234567890abcdef");
    env::set_var("SERVER_HOST", "127.0.0.1");
    env::set_var("SERVER_PORT", "8084");
    env::set_var("RUST_LOG", "warn"); // Reduce log output to improve performance
    env::set_var("LOG_FORMAT", "text");
    env::set_var("CORS_ENABLED", "true");
    env::set_var("MAX_REQUEST_SIZE", "1048576");
}

/// Create test settings
fn create_test_settings() -> Settings {
    setup_test_env();
    Settings::new().expect("Failed to create test settings")
}

/// Create simple Claude request
fn create_simple_claude_request() -> ClaudeRequest {
    ClaudeRequest {
        model: "claude-3-sonnet".to_string(),
        max_tokens: 100,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Text("Hello, world!".to_string()),
        }],
        ..Default::default()
    }
}

/// Create complex Claude request
fn create_complex_claude_request() -> ClaudeRequest {
    ClaudeRequest {
        model: "claude-3-opus".to_string(),
        max_tokens: 1000,
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("What is artificial intelligence?".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ClaudeContent::Text("Artificial intelligence (AI) is a branch of computer science...".to_string()),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Can you explain machine learning?".to_string()),
            },
        ],
        system: Some("You are an expert in artificial intelligence and machine learning.".to_string()),
        temperature: Some(0.7),
        top_p: Some(0.9),
        ..Default::default()
    }
}

/// Benchmark: Health check endpoint
fn bench_health_endpoint(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("health_endpoint", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request = Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert_eq!(response.status(), StatusCode::OK);
            })
        })
    });
}

/// Benchmark: Ready check endpoint
fn bench_readiness_endpoint(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("readiness_endpoint", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request = Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                // Expect 503 in test environment because OpenAI connection will fail
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            })
        })
    });
}

/// Benchmark: Live check endpoint
fn bench_liveness_endpoint(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("liveness_endpoint", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request = Request::builder()
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert_eq!(response.status(), StatusCode::OK);
            })
        })
    });
}

/// Benchmark: Message endpoint request validation
fn bench_messages_validation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("messages_validation", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let claude_request = create_simple_claude_request();
                let request_body = serde_json::to_string(&claude_request).unwrap();
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890")
                    .body(Body::from(request_body))
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                // Expect 502 because OpenAI request will fail, but validation should pass
                assert!(response.status() == StatusCode::BAD_GATEWAY || response.status() == StatusCode::INTERNAL_SERVER_ERROR);
            })
        })
    });
}

/// Benchmark: Authentication failure handling
fn bench_auth_failure(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("auth_failure", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let claude_request = create_simple_claude_request();
                let request_body = serde_json::to_string(&claude_request).unwrap();
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    // Intentionally not providing authentication header
                    .body(Body::from(request_body))
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            })
        })
    });
}

/// Benchmark: Different request size handling
fn bench_request_sizes(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("request_sizes");
    
    for size in [100, 1000, 10000].iter() {
        let content = "x".repeat(*size);
        
        group.bench_with_input(BenchmarkId::new("process_request", size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let settings = create_test_settings();
                    let app = create_router(settings).await.expect("Failed to create router");
                    
                    let claude_request = ClaudeRequest {
                        model: "claude-3-sonnet".to_string(),
                        max_tokens: 100,
                        messages: vec![ClaudeMessage {
                            role: "user".to_string(),
                            content: ClaudeContent::Text(content.clone()),
                        }],
                        ..Default::default()
                    };
                    
                    let request_body = serde_json::to_string(&claude_request).unwrap();
                    
                    let request = Request::builder()
                        .method("POST")
                        .uri("/v1/messages")
                        .header("content-type", "application/json")
                        .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890")
                        .body(Body::from(request_body))
                        .unwrap();
                    
                    let response = black_box(app.oneshot(request).await.unwrap());
                    // Validation should pass, but OpenAI request will fail
                    assert!(response.status() == StatusCode::BAD_GATEWAY || response.status() == StatusCode::INTERNAL_SERVER_ERROR);
                })
            })
        });
    }
    
    group.finish();
}

/// Benchmark: Concurrent request handling
fn bench_concurrent_requests(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_requests");
    
    for concurrency in [1, 5, 10].iter() {
        group.bench_with_input(BenchmarkId::new("health_check", concurrency), concurrency, |b, &concurrency| {
            b.iter(|| {
                rt.block_on(async move {
                    let settings = create_test_settings();
                    let app = create_router(settings).await.expect("Failed to create router");
                    
                    let mut handles = vec![];
                    
                    for _ in 0..concurrency {
                        let app_clone = app.clone();
                        let handle = tokio::spawn(async move {
                            let request = Request::builder()
                                .uri("/health")
                                .body(Body::empty())
                                .unwrap();
                            
                            app_clone.oneshot(request).await.unwrap()
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        let response = black_box(handle.await.unwrap());
                        assert_eq!(response.status(), StatusCode::OK);
                    }
                })
            })
        });
    }
    
    group.finish();
}

/// Benchmark: JSON serialization/deserialization overhead
fn bench_json_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("json_processing");
    
    let simple_request = create_simple_claude_request();
    let complex_request = create_complex_claude_request();
    
    group.bench_function("simple_request_json", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request_body = black_box(serde_json::to_string(&simple_request).unwrap());
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890")
                    .body(Body::from(request_body))
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert!(response.status().is_client_error() || response.status().is_server_error());
            })
        })
    });
    
    group.bench_function("complex_request_json", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request_body = black_box(serde_json::to_string(&complex_request).unwrap());
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890")
                    .body(Body::from(request_body))
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert!(response.status().is_client_error() || response.status().is_server_error());
            })
        })
    });
    
    group.finish();
}

/// Benchmark: Error handling performance
fn bench_error_handling(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("error_handling");
    
    // Test performance of various error scenario handling
    group.bench_function("missing_auth", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"model":"claude-3-sonnet","max_tokens":100,"messages":[{"role":"user","content":"test"}]}"#))
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            })
        })
    });
    
    group.bench_function("invalid_json", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890")
                    .body(Body::from(r#"{"model":"claude-3-sonnet","max_tokens":}"#)) // Invalid JSON
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            })
        })
    });
    
    group.bench_function("validation_error", |b| {
        b.iter(|| {
            rt.block_on(async {
                let settings = create_test_settings();
                let app = create_router(settings).await.expect("Failed to create router");
                
                let request = Request::builder()
                    .method("POST")
                    .uri("/v1/messages")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890")
                    .body(Body::from(r#"{"model":"claude-3-sonnet","max_tokens":0,"messages":[]}"#)) // Validation error
                    .unwrap();
                
                let response = black_box(app.oneshot(request).await.unwrap());
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            })
        })
    });
    
    group.finish();
}

/// Benchmark: Routing performance
fn bench_routing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("routing");
    
    let endpoints = vec![
        ("/health", "GET"),
        ("/health/ready", "GET"),
        ("/health/live", "GET"),
        ("/", "GET"),
        ("/v1/messages", "POST"),
    ];
    
    for (path, method) in endpoints {
        group.bench_function(&format!("{}_{}", method.to_lowercase(), path.replace('/', "_")), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let settings = create_test_settings();
                    let app = create_router(settings).await.expect("Failed to create router");
                    
                    let mut request_builder = Request::builder().method(method).uri(path);
                    
                    let body = if method == "POST" {
                        request_builder = request_builder
                            .header("content-type", "application/json")
                            .header("authorization", "Bearer sk-ant-api03-test123456789012345678901234567890");
                        Body::from(r#"{"model":"claude-3-sonnet","max_tokens":100,"messages":[{"role":"user","content":"test"}]}"#)
                    } else {
                        Body::empty()
                    };
                    
                    let request = request_builder.body(body).unwrap();
                    let response = black_box(app.oneshot(request).await.unwrap());
                    
                    // Verify response status codes are within reasonable range
                    assert!(response.status().as_u16() >= 200 && response.status().as_u16() < 600);
                })
            })
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_health_endpoint,
    bench_readiness_endpoint,
    bench_liveness_endpoint,
    bench_messages_validation,
    bench_auth_failure,
    bench_request_sizes,
    bench_concurrent_requests,
    bench_json_processing,
    bench_error_handling,
    bench_routing
);

criterion_main!(benches);