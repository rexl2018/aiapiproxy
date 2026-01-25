//! API conversion performance benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use aiapiproxy::services::ApiConverter;
use aiapiproxy::models::claude::*;
use aiapiproxy::models::openai::*;
use aiapiproxy::config::settings::*;
use std::collections::HashMap;
use chrono::Utc;

/// Create test settings
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
            custom: HashMap::new(),
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

/// Create complex Claude request (with multiple messages and system prompt)
fn create_complex_claude_request() -> ClaudeRequest {
    ClaudeRequest {
        model: "claude-3-opus".to_string(),
        max_tokens: 1000,
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("What is the capital of France?".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ClaudeContent::Text("The capital of France is Paris.".to_string()),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Tell me more about Paris.".to_string()),
            },
        ],
        system: Some(SystemPrompt::String("You are a helpful geography assistant.".to_string())),
        temperature: Some(0.7),
        top_p: Some(0.9),
        stop_sequences: Some(vec!["\n\n".to_string()]),
        ..Default::default()
    }
}

/// Create multimodal Claude request
fn create_multimodal_claude_request() -> ClaudeRequest {
    ClaudeRequest {
        model: "claude-3-sonnet".to_string(),
        max_tokens: 500,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Blocks(vec![
                ClaudeContentBlock::Text {
                    text: "What's in this image?".to_string(),
                },
                ClaudeContentBlock::Image {
                    source: ClaudeImageSource {
                        source_type: "base64".to_string(),
                        media_type: "image/jpeg".to_string(),
                        data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(),
                    },
                },
            ]),
        }],
        ..Default::default()
    }
}

/// Create OpenAI response
fn create_openai_response() -> OpenAIResponse {
    OpenAIResponse {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion".to_string(),
        created: Utc::now().timestamp() as u64,
        model: "gpt-4o".to_string(),
        choices: vec![OpenAIChoice {
            index: 0,
            message: OpenAIMessage {
                role: "assistant".to_string(),
                content: Some(OpenAIContent::Text("Hello! How can I help you today?".to_string())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            logprobs: None,
            finish_reason: Some("stop".to_string()),
        }],
        usage: Some(OpenAIUsage {
            prompt_tokens: 15,
            completion_tokens: 10,
            total_tokens: 25,
        }),
        system_fingerprint: None,
    }
}

/// Create OpenAI streaming response
fn create_openai_stream_response() -> OpenAIStreamResponse {
    OpenAIStreamResponse {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: Utc::now().timestamp() as u64,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![OpenAIStreamChoice {
            index: 0,
            delta: OpenAIStreamDelta {
                role: Some("assistant".to_string()),
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
            logprobs: None,
            finish_reason: None,
        }],
    }
}

/// Benchmark: Simple request conversion
fn bench_simple_request_conversion(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    let claude_request = create_simple_claude_request();
    
    c.bench_function("simple_request_conversion", |b| {
        b.iter(|| {
            black_box(converter.convert_request(black_box(claude_request.clone())).unwrap())
        })
    });
}

/// Benchmark: Complex request conversion
fn bench_complex_request_conversion(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    let claude_request = create_complex_claude_request();
    
    c.bench_function("complex_request_conversion", |b| {
        b.iter(|| {
            black_box(converter.convert_request(black_box(claude_request.clone())).unwrap())
        })
    });
}

/// Benchmark: Multimodal request conversion
fn bench_multimodal_request_conversion(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    let claude_request = create_multimodal_claude_request();
    
    c.bench_function("multimodal_request_conversion", |b| {
        b.iter(|| {
            black_box(converter.convert_request(black_box(claude_request.clone())).unwrap())
        })
    });
}

/// Benchmark: Response conversion
fn bench_response_conversion(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    let openai_response = create_openai_response();
    
    c.bench_function("response_conversion", |b| {
        b.iter(|| {
            black_box(converter.convert_response(
                black_box(openai_response.clone()),
                black_box("claude-3-sonnet")
            ).unwrap())
        })
    });
}

/// Benchmark: Streaming response conversion
fn bench_stream_conversion(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    let stream_response = create_openai_stream_response();
    
    c.bench_function("stream_conversion", |b| {
        b.iter(|| {
            black_box(converter.convert_stream_chunk(
                black_box(stream_response.clone()),
                black_box("claude-3-sonnet")
            ).unwrap())
        })
    });
}

/// Benchmark: Different request sizes
fn bench_request_sizes(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let mut group = c.benchmark_group("request_sizes");
    
    for size in [10, 100, 1000, 10000].iter() {
        let content = "x".repeat(*size);
        let claude_request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text(content),
            }],
            ..Default::default()
        };
        
        group.bench_with_input(BenchmarkId::new("convert_request", size), size, |b, _| {
            b.iter(|| {
                black_box(converter.convert_request(black_box(claude_request.clone())).unwrap())
            })
        });
    }
    
    group.finish();
}

/// Benchmark: Different message counts
fn bench_message_counts(c: &mut Criterion) {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let mut group = c.benchmark_group("message_counts");
    
    for count in [1, 5, 10, 20, 50].iter() {
        let mut messages = Vec::new();
        for i in 0..*count {
            messages.push(ClaudeMessage {
                role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: ClaudeContent::Text(format!("Message {}", i)),
            });
        }
        
        let claude_request = ClaudeRequest {
            model: "claude-3-sonnet".to_string(),
            max_tokens: 100,
            messages,
            ..Default::default()
        };
        
        group.bench_with_input(BenchmarkId::new("convert_request", count), count, |b, _| {
            b.iter(|| {
                black_box(converter.convert_request(black_box(claude_request.clone())).unwrap())
            })
        });
    }
    
    group.finish();
}

/// Benchmark: Serialization performance
fn bench_serialization(c: &mut Criterion) {
    let claude_request = create_complex_claude_request();
    let openai_response = create_openai_response();
    
    let mut group = c.benchmark_group("serialization");
    
    group.bench_function("claude_request_serialize", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&black_box(&claude_request)).unwrap())
        })
    });
    
    group.bench_function("openai_response_serialize", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&black_box(&openai_response)).unwrap())
        })
    });
    
    let claude_json = serde_json::to_string(&claude_request).unwrap();
    let openai_json = serde_json::to_string(&openai_response).unwrap();
    
    group.bench_function("claude_request_deserialize", |b| {
        b.iter(|| {
            black_box(serde_json::from_str::<ClaudeRequest>(&black_box(&claude_json)).unwrap())
        })
    });
    
    group.bench_function("openai_response_deserialize", |b| {
        b.iter(|| {
            black_box(serde_json::from_str::<OpenAIResponse>(&black_box(&openai_json)).unwrap())
        })
    });
    
    group.finish();
}

/// Benchmark: Content extraction
fn bench_content_extraction(c: &mut Criterion) {
    let text_content = ClaudeContent::Text("Hello world".repeat(100));
    let blocks_content = ClaudeContent::Blocks(vec![
        ClaudeContentBlock::Text { text: "Hello ".repeat(50) },
        ClaudeContentBlock::Text { text: "world".repeat(50) },
    ]);
    
    let mut group = c.benchmark_group("content_extraction");
    
    group.bench_function("text_extract", |b| {
        b.iter(|| {
            black_box(black_box(&text_content).extract_text())
        })
    });
    
    group.bench_function("blocks_extract", |b| {
        b.iter(|| {
            black_box(black_box(&blocks_content).extract_text())
        })
    });
    
    group.bench_function("has_images_check", |b| {
        b.iter(|| {
            black_box(black_box(&blocks_content).has_images())
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_simple_request_conversion,
    bench_complex_request_conversion,
    bench_multimodal_request_conversion,
    bench_response_conversion,
    bench_stream_conversion,
    bench_request_sizes,
    bench_message_counts,
    bench_serialization,
    bench_content_extraction
);

criterion_main!(benches);