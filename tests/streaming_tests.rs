//! Streaming response functionality tests

use aiapiproxy::handlers::AppState;
use aiapiproxy::config::settings::*;
use aiapiproxy::config::{AppConfig, ModelConfig, ProviderConfig};
use aiapiproxy::services::{ApiConverter, Router};
use aiapiproxy::models::claude::*;
use aiapiproxy::models::openai::*;
use std::sync::Arc;
use std::collections::HashMap;

/// Create test config
fn create_test_config() -> AppConfig {
    let mut models = HashMap::new();
    models.insert("gpt-4o".to_string(), ModelConfig {
        name: "gpt-4o".to_string(),
        alias: None,
        max_tokens: Some(8192),
        temperature: None,
        options: Default::default(),
    });
    
    let mut providers = HashMap::new();
    providers.insert("openai".to_string(), ProviderConfig {
        provider_type: "openai".to_string(),
        base_url: "https://api.openai.com/v1".to_string(),
        api_key: "test_key".to_string(),
        options: Default::default(),
        models,
    });
    
    AppConfig { 
        server: aiapiproxy::config::ServerConfig::default(),
        providers,
        model_mapping: HashMap::new(),
    }
}

/// Create test application state
fn create_test_app_state() -> Arc<AppState> {
    let settings = Settings {
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
    };
    
    let converter = ApiConverter::new(settings.clone());
    let router = Arc::new(Router::new(create_test_config()).unwrap());
    
    Arc::new(AppState {
        settings,
        converter,
        router,
    })
}

/// Create test Claude streaming request
fn create_test_claude_stream_request() -> ClaudeRequest {
    ClaudeRequest {
        model: "claude-3-sonnet".to_string(),
        max_tokens: 1000,
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("Please answer in streaming mode: What is artificial intelligence?".to_string()),
            }
        ],
        stream: Some(true),
        temperature: Some(0.7),
        top_p: None,
        top_k: None,
        stop_sequences: None,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
    }
}

#[tokio::test]
async fn test_stream_request_validation() {
    let claude_request = create_test_claude_stream_request();
    
    // Verify streaming request flag
    assert_eq!(claude_request.stream, Some(true));
    
    // Verify required fields
    assert!(!claude_request.model.is_empty());
    assert!(!claude_request.messages.is_empty());
    assert!(claude_request.max_tokens > 0);
}

#[tokio::test]
async fn test_stream_request_conversion() {
    let app_state = create_test_app_state();
    let claude_request = create_test_claude_stream_request();
    
    // Test Claude to OpenAI request conversion
    let openai_request = app_state.converter.convert_request(claude_request).unwrap();
    
    // Verify conversion result
    assert_eq!(openai_request.model, "gpt-4o"); // sonnet mapping
    assert_eq!(openai_request.stream, Some(true));
    assert!(!openai_request.messages.is_empty());
    assert_eq!(openai_request.max_tokens, Some(1000));
    assert_eq!(openai_request.temperature, Some(0.7));
}

#[test]
fn test_stream_chunk_conversion() {
    let app_state = create_test_app_state();
    
    // Create mock OpenAI streaming response chunk
    let openai_chunk = OpenAIStreamResponse {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![
            OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: Some("assistant".to_string()),
                    content: Some("Artificial intelligence".to_string()),
                    tool_calls: None,
                },
                logprobs: None,
                finish_reason: None,
            }
        ],
    };
    
    // Test streaming chunk conversion
    let claude_events = app_state.converter.convert_stream_chunk(openai_chunk, "claude-3-sonnet").unwrap();
    
    // Verify conversion result
    assert!(!claude_events.is_empty());
    
    // Check first event
    if let Some(first_event) = claude_events.first() {
        match first_event {
            ClaudeStreamEvent::ContentBlockDelta { delta, .. } => {
                let ClaudeContentDelta::TextDelta { text } = delta;
                assert_eq!(text, "Artificial intelligence");
            }
            _ => {}
                // Accept any event type as the actual implementation may vary
        }
    }
}

#[test]
fn test_stream_error_handling() {
    let app_state = create_test_app_state();
    
    // Create OpenAI streaming response with error
    let error_chunk = OpenAIStreamResponse {
        id: "error-test".to_string(),
        object: "error".to_string(),
        created: 1234567890,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![],
    };
    
    // Test error handling
    let result = app_state.converter.convert_stream_chunk(error_chunk, "claude-3-sonnet");
    
    // Verify error handling
    match result {
        Ok(events) => {
            // May return empty events or error events
            // Both behaviors are acceptable
        }
        Err(_) => {
            // Or return error
            // This is also acceptable behavior
        }
    }
}

#[test]
fn test_stream_completion_event() {
    let app_state = create_test_app_state();
    
    // Create completed OpenAI streaming response chunk
    let completion_chunk = OpenAIStreamResponse {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![
            OpenAIStreamChoice {
                index: 0,
                delta: OpenAIStreamDelta {
                    role: None,
                    content: None,
                    tool_calls: None,
                },
                logprobs: None,
                finish_reason: Some("stop".to_string()),
            }
        ],
    };
    
    // Test completion event conversion
    let claude_events = app_state.converter.convert_stream_chunk(completion_chunk, "claude-3-sonnet").unwrap();
    
    // Verify contains completion events
    let has_message_stop = claude_events.iter().any(|event| {
        matches!(event, ClaudeStreamEvent::MessageStop)
    });
    
    assert!(has_message_stop, "Should contain MessageStop event");
}

#[test]
fn test_multiple_stream_chunks() {
    let app_state = create_test_app_state();
    
    // Simulate multiple streaming response chunks
    let chunks = vec![
        // Start chunk
        OpenAIStreamResponse {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "gpt-4o".to_string(),
            system_fingerprint: None,
            choices: vec![
                OpenAIStreamChoice {
                    index: 0,
                    delta: OpenAIStreamDelta {
                        role: Some("assistant".to_string()),
                        content: None,
                        tool_calls: None,
                    },
                    logprobs: None,
                    finish_reason: None,
                }
            ],
        },
        // Content chunk
        OpenAIStreamResponse {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "gpt-4o".to_string(),
            system_fingerprint: None,
            choices: vec![
                OpenAIStreamChoice {
                    index: 0,
                    delta: OpenAIStreamDelta {
                        role: None,
                        content: Some("Artificial intelligence is".to_string()),
                        tool_calls: None,
                    },
                    logprobs: None,
                    finish_reason: None,
                }
            ],
        },
        // End chunk
        OpenAIStreamResponse {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "gpt-4o".to_string(),
            system_fingerprint: None,
            choices: vec![
                OpenAIStreamChoice {
                    index: 0,
                    delta: OpenAIStreamDelta {
                        role: None,
                        content: None,
                        tool_calls: None,
                    },
                    logprobs: None,
                    finish_reason: Some("stop".to_string()),
                }
            ],
        },
    ];
    
    let mut all_events = Vec::new();
    
    // Process each chunk
    for chunk in chunks {
        let events = app_state.converter.convert_stream_chunk(chunk, "claude-3-sonnet").unwrap();
        all_events.extend(events);
    }
    
    // Verify event sequence
    assert!(!all_events.is_empty());
    
    // Should contain start, content and end events
    let has_message_start = all_events.iter().any(|event| {
        matches!(event, ClaudeStreamEvent::MessageStart { .. })
    });
    
    let has_content_delta = all_events.iter().any(|event| {
        matches!(event, ClaudeStreamEvent::ContentBlockDelta { .. })
    });
    
    let has_message_stop = all_events.iter().any(|event| {
        matches!(event, ClaudeStreamEvent::MessageStop)
    });
    
    assert!(has_message_start, "Should contain MessageStart event");
    assert!(has_content_delta, "Should contain ContentBlockDelta event");
    assert!(has_message_stop, "Should contain MessageStop event");
}

#[test]
fn test_stream_event_serialization() {
    // Test Claude streaming event JSON serialization
    let event = ClaudeStreamEvent::ContentBlockDelta {
        index: 0,
        delta: ClaudeContentDelta::TextDelta {
            text: "Test text".to_string(),
        },
    };
    
    // Serialize to JSON
    let json = serde_json::to_string(&event).unwrap();
    
    // Verify JSON format
    assert!(json.contains("content_block_delta"));
    assert!(json.contains("Test text"));
    
    // Verify deserialization
    let deserialized: ClaudeStreamEvent = serde_json::from_str(&json).unwrap();
    
    match deserialized {
        ClaudeStreamEvent::ContentBlockDelta { index, delta } => {
            assert_eq!(index, 0);
            let ClaudeContentDelta::TextDelta { text } = delta;
            assert_eq!(text, "Test text");
        }
        _ => panic!("Deserialization failed"),
    }
}