//! API converter unit tests

use aiapiproxy::services::ApiConverter;
use aiapiproxy::models::claude::*;
use aiapiproxy::models::openai::*;
use aiapiproxy::config::settings::*;
use chrono::Utc;
use std::collections::HashMap;

/// Create test settings
fn create_test_settings() -> Settings {
    Settings {
        server: ServerConfig {
            host: "localhost".to_string(),
            port: 8080,
        },
        openai: OpenAIConfig {
            api_key: "sk-test".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            timeout: 30,
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

#[test]
fn test_convert_simple_text_request() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let claude_request = ClaudeRequest {
        model: "claude-3-sonnet".to_string(),
        max_tokens: 100,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Text("Hello, world!".to_string()),
        }],
        system: Some(SystemPrompt::String("You are a helpful assistant.".to_string())),
        temperature: Some(0.7),
        top_p: Some(0.9),
        stop_sequences: Some(vec!["\n\n".to_string()]),
        stream: Some(false),
        ..Default::default()
    };
    
    let openai_request = converter.convert_request(claude_request).unwrap();
    
    assert_eq!(openai_request.model, "gpt-4o");
    assert_eq!(openai_request.max_tokens, Some(100));
    assert_eq!(openai_request.temperature, Some(0.7));
    assert_eq!(openai_request.top_p, Some(0.9));
    assert_eq!(openai_request.stop, Some(vec!["\n\n".to_string()]));
    assert_eq!(openai_request.stream, Some(false));
    assert_eq!(openai_request.n, Some(1));
    
    // Check message conversion
    assert_eq!(openai_request.messages.len(), 2); // system + user
    assert_eq!(openai_request.messages[0].role, "system");
    assert_eq!(openai_request.messages[1].role, "user");
    
    if let Some(OpenAIContent::Text(content)) = &openai_request.messages[1].content {
        assert_eq!(content, "Hello, world!");
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_convert_multimodal_request() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let claude_request = ClaudeRequest {
        model: "claude-3-opus".to_string(),
        max_tokens: 200,
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
    };
    
    let openai_request = converter.convert_request(claude_request).unwrap();
    
    assert_eq!(openai_request.model, "gpt-4");
    assert_eq!(openai_request.messages.len(), 1);
    
    if let Some(OpenAIContent::Array(parts)) = &openai_request.messages[0].content {
        assert_eq!(parts.len(), 2);
        
        // Check text part
        if let OpenAIContentPart::Text { text } = &parts[0] {
            assert_eq!(text, "What's in this image?");
        } else {
            panic!("Expected text part");
        }
        
        // Check image part
        if let OpenAIContentPart::ImageUrl { image_url } = &parts[1] {
            assert!(image_url.url.starts_with("data:image/jpeg;base64,"));
            assert_eq!(image_url.detail, Some("auto".to_string()));
        } else {
            panic!("Expected image URL part");
        }
    } else {
        panic!("Expected array content");
    }
}

#[test]
fn test_convert_response() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let openai_response = OpenAIResponse {
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
        usage: OpenAIUsage {
            prompt_tokens: 15,
            completion_tokens: 10,
            total_tokens: 25,
        },
        system_fingerprint: None,
    };
    
    let claude_response = converter.convert_response(openai_response, "claude-3-sonnet").unwrap();
    
    assert_eq!(claude_response.model, "claude-3-sonnet");
    assert_eq!(claude_response.role, "assistant");
    assert_eq!(claude_response.response_type, "message");
    assert_eq!(claude_response.stop_reason, Some("end_turn".to_string()));
    assert_eq!(claude_response.usage.input_tokens, 15);
    assert_eq!(claude_response.usage.output_tokens, 10);
    
    assert_eq!(claude_response.content.len(), 1);
    if let ClaudeContentBlock::Text { text } = &claude_response.content[0] {
        assert_eq!(text, "Hello! How can I help you today?");
    } else {
        panic!("Expected text content block");
    }
}

#[test]
fn test_convert_stream_chunk_start() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let openai_chunk = OpenAIStreamResponse {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: Utc::now().timestamp() as u64,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![OpenAIStreamChoice {
            index: 0,
            delta: OpenAIStreamDelta {
                role: Some("assistant".to_string()),
                content: None,
                tool_calls: None,
            },
            logprobs: None,
            finish_reason: None,
        }],
    };
    
    let claude_events = converter.convert_stream_chunk(openai_chunk, "claude-3-sonnet").unwrap();
    
    assert_eq!(claude_events.len(), 2); // MessageStart + ContentBlockStart
    
    // Check MessageStart event
    if let ClaudeStreamEvent::MessageStart { message } = &claude_events[0] {
        assert_eq!(message.role, "assistant");
        assert_eq!(message.model, "claude-3-sonnet");
        assert_eq!(message.message_type, "message");
    } else {
        panic!("Expected MessageStart event");
    }
    
    // Check ContentBlockStart event
    if let ClaudeStreamEvent::ContentBlockStart { index, content_block } = &claude_events[1] {
        assert_eq!(*index, 0);
        if let ClaudeContentBlock::Text { text } = content_block {
            assert_eq!(text, "");
        } else {
            panic!("Expected text content block");
        }
    } else {
        panic!("Expected ContentBlockStart event");
    }
}

#[test]
fn test_convert_stream_chunk_delta() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let openai_chunk = OpenAIStreamResponse {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: Utc::now().timestamp() as u64,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![OpenAIStreamChoice {
            index: 0,
            delta: OpenAIStreamDelta {
                role: None,
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
            logprobs: None,
            finish_reason: None,
        }],
    };
    
    let claude_events = converter.convert_stream_chunk(openai_chunk, "claude-3-sonnet").unwrap();
    
    assert_eq!(claude_events.len(), 1);
    
    // Check ContentBlockDelta event
    if let ClaudeStreamEvent::ContentBlockDelta { index, delta } = &claude_events[0] {
        assert_eq!(*index, 0);
        let ClaudeContentDelta::TextDelta { text } = delta;
        assert_eq!(text, "Hello");
    } else {
        panic!("Expected ContentBlockDelta event");
    }
}

#[test]
fn test_convert_stream_chunk_end() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let openai_chunk = OpenAIStreamResponse {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: Utc::now().timestamp() as u64,
        model: "gpt-4o".to_string(),
        system_fingerprint: None,
        choices: vec![OpenAIStreamChoice {
            index: 0,
            delta: OpenAIStreamDelta {
                role: None,
                content: None,
                tool_calls: None,
            },
            logprobs: None,
            finish_reason: Some("stop".to_string()),
        }],
    };
    
    let claude_events = converter.convert_stream_chunk(openai_chunk, "claude-3-sonnet").unwrap();
    
    assert_eq!(claude_events.len(), 3); // ContentBlockStop + MessageDelta + MessageStop
    
    // Check ContentBlockStop event
    if let ClaudeStreamEvent::ContentBlockStop { index } = &claude_events[0] {
        assert_eq!(*index, 0);
    } else {
        panic!("Expected ContentBlockStop event");
    }
    
    // Check MessageDelta event
    if let ClaudeStreamEvent::MessageDelta { delta, usage: _ } = &claude_events[1] {
        assert_eq!(delta.stop_reason, Some("end_turn".to_string()));
    } else {
        panic!("Expected MessageDelta event");
    }
    
    // Check MessageStop event
    if let ClaudeStreamEvent::MessageStop = &claude_events[2] {
        // Correct
    } else {
        panic!("Expected MessageStop event");
    }
}

#[test]
fn test_convert_error() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let openai_error = OpenAIError {
        message: "Invalid request".to_string(),
        error_type: "invalid_request_error".to_string(),
        param: Some("model".to_string()),
        code: Some("invalid_model".to_string()),
    };
    
    let claude_error = converter.convert_error(openai_error);
    
    assert_eq!(claude_error.error_type, "error");
    assert_eq!(claude_error.error.error_type, "invalid_request_error");
    assert_eq!(claude_error.error.message, "Invalid request");
}

#[test]
fn test_finish_reason_mapping() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    // Test various finish_reason mappings
    let test_cases = vec![
        ("stop", "end_turn"),
        ("length", "max_tokens"),
        ("content_filter", "stop_sequence"),
        ("tool_calls", "tool_use"),
        ("unknown", "end_turn"), // Unknown type should map to end_turn
    ];
    
    for (openai_reason, expected_claude_reason) in test_cases {
        let openai_response = OpenAIResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "gpt-4o".to_string(),
            choices: vec![OpenAIChoice {
                index: 0,
                message: OpenAIMessage {
                    role: "assistant".to_string(),
                    content: Some(OpenAIContent::Text("test".to_string())),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                logprobs: None,
                finish_reason: Some(openai_reason.to_string()),
            }],
            usage: OpenAIUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            system_fingerprint: None,
        };
        
        let claude_response = converter.convert_response(openai_response, "claude-3-sonnet").unwrap();
        assert_eq!(claude_response.stop_reason, Some(expected_claude_reason.to_string()));
    }
}

#[test]
fn test_model_mapping() {
    let mut custom_mapping = HashMap::new();
    custom_mapping.insert("claude-custom".to_string(), "gpt-custom".to_string());
    
    let mut settings = create_test_settings();
    settings.model_mapping.custom = custom_mapping;
    
    let converter = ApiConverter::new(settings);
    
    // Test predefined mappings
    let test_cases = vec![
        ("claude-3-haiku-20240307", "gpt-4o-mini"),
        ("claude-3-sonnet-20240229", "gpt-4o"),
        ("claude-3-opus-20240229", "gpt-4"),
        ("claude-custom", "gpt-custom"), // Custom mapping
            ("unknown-model", "gpt-4o"), // Unknown model should map to default sonnet
    ];
    
    for (claude_model, expected_openai_model) in test_cases {
        let claude_request = ClaudeRequest {
            model: claude_model.to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text("test".to_string()),
            }],
            ..Default::default()
        };
        
        let openai_request = converter.convert_request(claude_request).unwrap();
        assert_eq!(openai_request.model, expected_openai_model);
    }
}

#[test]
fn test_empty_response_handling() {
    let settings = create_test_settings();
    let converter = ApiConverter::new(settings);
    
    let openai_response = OpenAIResponse {
        id: "test".to_string(),
        object: "chat.completion".to_string(),
        created: 0,
        model: "gpt-4o".to_string(),
        choices: vec![], // Empty choices
        usage: OpenAIUsage {
            prompt_tokens: 1,
            completion_tokens: 0,
            total_tokens: 1,
        },
        system_fingerprint: None,
    };
    
    let result = converter.convert_response(openai_response, "claude-3-sonnet");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No choices"));
}

#[test]
fn test_content_extraction() {
    // Test ClaudeContent text extraction
    let text_content = ClaudeContent::Text("Hello world".to_string());
    assert_eq!(text_content.extract_text(), "Hello world");
    
    let blocks_content = ClaudeContent::Blocks(vec![
        ClaudeContentBlock::Text { text: "Hello ".to_string() },
        ClaudeContentBlock::Text { text: "world".to_string() },
    ]);
    assert_eq!(blocks_content.extract_text(), "Hello world");
    
    // Test image detection
    assert!(!text_content.has_images());
    
    let image_content = ClaudeContent::Blocks(vec![
        ClaudeContentBlock::Text { text: "Look at this:".to_string() },
        ClaudeContentBlock::Image {
            source: ClaudeImageSource {
                source_type: "base64".to_string(),
                media_type: "image/jpeg".to_string(),
                data: "test".to_string(),
            },
        },
    ]);
    assert!(image_content.has_images());
}