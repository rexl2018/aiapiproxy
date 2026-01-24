//! Data model unit tests

use aiapiproxy::models::claude::*;
use aiapiproxy::models::openai::*;
use serde_json;
use std::collections::HashMap;

#[test]
fn test_claude_request_serialization() {
    let request = ClaudeRequest {
        model: "claude-3-sonnet".to_string(),
        max_tokens: 100,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Text("Hello".to_string()),
        }],
        system: Some(SystemPrompt::String("You are helpful".to_string())),
        temperature: Some(0.7),
        top_p: Some(0.9),
        top_k: Some(40),
        stop_sequences: Some(vec!["\n".to_string()]),
        stream: Some(false),
        tools: None,
        tool_choice: None,
        metadata: Some({
            let mut map = HashMap::new();
            map.insert("user_id".to_string(), serde_json::Value::String("123".to_string()));
            map
        }),
    };
    
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: ClaudeRequest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(request.model, deserialized.model);
    assert_eq!(request.max_tokens, deserialized.max_tokens);
    assert_eq!(request.system, deserialized.system);
    assert_eq!(request.temperature, deserialized.temperature);
    assert_eq!(request.top_p, deserialized.top_p);
    assert_eq!(request.top_k, deserialized.top_k);
    assert_eq!(request.stop_sequences, deserialized.stop_sequences);
    assert_eq!(request.stream, deserialized.stream);
}

#[test]
fn test_claude_content_text() {
    let content = ClaudeContent::Text("Hello world".to_string());
    
    let json = serde_json::to_string(&content).unwrap();
    assert_eq!(json, "\"Hello world\"");
    
    let deserialized: ClaudeContent = serde_json::from_str(&json).unwrap();
    assert_eq!(content.extract_text(), "Hello world");
    assert!(!content.has_images());
    
    if let ClaudeContent::Text(text) = deserialized {
        assert_eq!(text, "Hello world");
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_claude_content_blocks() {
    let content = ClaudeContent::Blocks(vec![
        ClaudeContentBlock::Text {
            text: "Look at this image:".to_string(),
        },
        ClaudeContentBlock::Image {
            source: ClaudeImageSource {
                source_type: "base64".to_string(),
                media_type: "image/jpeg".to_string(),
                data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(),
            },
        },
    ]);
    
    let json = serde_json::to_string(&content).unwrap();
    let deserialized: ClaudeContent = serde_json::from_str(&json).unwrap();
    
    assert_eq!(content.extract_text(), "Look at this image:");
    assert!(content.has_images());
    
    if let ClaudeContent::Blocks(blocks) = deserialized {
        assert_eq!(blocks.len(), 2);
        
        if let ClaudeContentBlock::Text { text } = &blocks[0] {
            assert_eq!(text, "Look at this image:");
        } else {
            panic!("Expected text block");
        }
        
        if let ClaudeContentBlock::Image { source } = &blocks[1] {
            assert_eq!(source.source_type, "base64");
            assert_eq!(source.media_type, "image/jpeg");
        } else {
            panic!("Expected image block");
        }
    } else {
        panic!("Expected blocks content");
    }
}

#[test]
fn test_claude_response_serialization() {
    let response = ClaudeResponse {
        id: "msg_123".to_string(),
        response_type: "message".to_string(),
        role: "assistant".to_string(),
        content: vec![ClaudeContentBlock::Text {
            text: "Hello! How can I help?".to_string(),
        }],
        model: "claude-3-sonnet".to_string(),
        stop_reason: Some("end_turn".to_string()),
        stop_sequence: None,
        usage: ClaudeUsage {
            input_tokens: 10,
            output_tokens: 15,
        },
    };
    
    let json = serde_json::to_string(&response).unwrap();
    let deserialized: ClaudeResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(response.id, deserialized.id);
    assert_eq!(response.response_type, deserialized.response_type);
    assert_eq!(response.role, deserialized.role);
    assert_eq!(response.model, deserialized.model);
    assert_eq!(response.stop_reason, deserialized.stop_reason);
    assert_eq!(response.usage.input_tokens, deserialized.usage.input_tokens);
    assert_eq!(response.usage.output_tokens, deserialized.usage.output_tokens);
}

#[test]
fn test_claude_stream_events() {
    // Test MessageStart event
    let message_start = ClaudeStreamEvent::MessageStart {
        message: ClaudeStreamMessage {
            id: "msg_123".to_string(),
            message_type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![],
            model: "claude-3-sonnet".to_string(),
            stop_reason: None,
            stop_sequence: None,
            usage: ClaudeUsage {
                input_tokens: 10,
                output_tokens: 0,
            },
        },
    };
    
    let json = serde_json::to_string(&message_start).unwrap();
    let deserialized: ClaudeStreamEvent = serde_json::from_str(&json).unwrap();
    
    if let ClaudeStreamEvent::MessageStart { message } = deserialized {
        assert_eq!(message.id, "msg_123");
        assert_eq!(message.role, "assistant");
    } else {
        panic!("Expected MessageStart event");
    }
    
    // Test ContentBlockDelta event
    let content_delta = ClaudeStreamEvent::ContentBlockDelta {
        index: 0,
        delta: ClaudeContentDelta::TextDelta {
            text: "Hello".to_string(),
        },
    };
    
    let json = serde_json::to_string(&content_delta).unwrap();
    let deserialized: ClaudeStreamEvent = serde_json::from_str(&json).unwrap();
    
    if let ClaudeStreamEvent::ContentBlockDelta { index, delta } = deserialized {
        assert_eq!(index, 0);
        if let ClaudeContentDelta::TextDelta { text } = delta {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected text delta");
        }
    } else {
        panic!("Expected ContentBlockDelta event");
    }
}

#[test]
fn test_claude_error_response() {
    let error_response = ClaudeErrorResponse {
        error_type: "error".to_string(),
        error: ClaudeError {
            error_type: "authentication_error".to_string(),
            message: "Invalid API key".to_string(),
        },
    };
    
    let json = serde_json::to_string(&error_response).unwrap();
    let deserialized: ClaudeErrorResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(error_response.error_type, deserialized.error_type);
    assert_eq!(error_response.error.error_type, deserialized.error.error_type);
    assert_eq!(error_response.error.message, deserialized.error.message);
}

#[test]
fn test_openai_request_serialization() {
    let request = OpenAIRequest {
        model: "gpt-4".to_string(),
        messages: vec![OpenAIMessage {
            role: "user".to_string(),
            content: Some(OpenAIContent::Text("Hello".to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        max_tokens: Some(100),
        temperature: Some(0.7),
        top_p: Some(0.9),
        n: Some(1),
        stop: Some(vec!["\n".to_string()]),
        stream: Some(false),
        presence_penalty: Some(0.0),
        frequency_penalty: Some(0.0),
        logit_bias: Some({
            let mut map = HashMap::new();
            map.insert("50256".to_string(), -100.0);
            map
        }),
        user: Some("user123".to_string()),
        response_format: Some(OpenAIResponseFormat {
            format_type: "json_object".to_string(),
        }),
        seed: Some(42),
        tools: None,
        tool_choice: None,
    };
    
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: OpenAIRequest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(request.model, deserialized.model);
    assert_eq!(request.max_tokens, deserialized.max_tokens);
    assert_eq!(request.temperature, deserialized.temperature);
    assert_eq!(request.top_p, deserialized.top_p);
    assert_eq!(request.n, deserialized.n);
    assert_eq!(request.stop, deserialized.stop);
    assert_eq!(request.stream, deserialized.stream);
    assert_eq!(request.user, deserialized.user);
    assert_eq!(request.seed, deserialized.seed);
}

#[test]
fn test_openai_content_types() {
    // Test text content
    let text_content = OpenAIContent::Text("Hello world".to_string());
    let json = serde_json::to_string(&text_content).unwrap();
    let deserialized: OpenAIContent = serde_json::from_str(&json).unwrap();
    
    assert_eq!(text_content.extract_text(), "Hello world");
    assert!(!text_content.has_images());
    
    // Test array content
    let array_content = OpenAIContent::Array(vec![
        OpenAIContentPart::Text {
            text: "Look at this:".to_string(),
        },
        OpenAIContentPart::ImageUrl {
            image_url: OpenAIImageUrl {
                url: "data:image/jpeg;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(),
                detail: Some("high".to_string()),
            },
        },
    ]);
    
    let json = serde_json::to_string(&array_content).unwrap();
    let deserialized: OpenAIContent = serde_json::from_str(&json).unwrap();
    
    assert_eq!(array_content.extract_text(), "Look at this:");
    assert!(array_content.has_images());
    
    if let OpenAIContent::Array(parts) = deserialized {
        assert_eq!(parts.len(), 2);
        
        if let OpenAIContentPart::Text { text } = &parts[0] {
            assert_eq!(text, "Look at this:");
        } else {
            panic!("Expected text part");
        }
        
        if let OpenAIContentPart::ImageUrl { image_url } = &parts[1] {
            assert!(image_url.url.starts_with("data:image/jpeg"));
            assert_eq!(image_url.detail, Some("high".to_string()));
        } else {
            panic!("Expected image URL part");
        }
    } else {
        panic!("Expected array content");
    }
}

#[test]
fn test_openai_response_serialization() {
    let response = OpenAIResponse {
        id: "chatcmpl-123".to_string(),
        object: "chat.completion".to_string(),
        created: 1677652288,
        model: "gpt-4".to_string(),
        choices: vec![OpenAIChoice {
            index: 0,
            message: OpenAIMessage {
                role: "assistant".to_string(),
                content: Some(OpenAIContent::Text("Hello!".to_string())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            logprobs: None,
            finish_reason: Some("stop".to_string()),
        }],
        usage: OpenAIUsage {
            prompt_tokens: 9,
            completion_tokens: 12,
            total_tokens: 21,
        },
        system_fingerprint: Some("fp_123".to_string()),
    };
    
    let json = serde_json::to_string(&response).unwrap();
    let deserialized: OpenAIResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(response.id, deserialized.id);
    assert_eq!(response.object, deserialized.object);
    assert_eq!(response.created, deserialized.created);
    assert_eq!(response.model, deserialized.model);
    assert_eq!(response.system_fingerprint, deserialized.system_fingerprint);
    assert_eq!(response.usage.prompt_tokens, deserialized.usage.prompt_tokens);
    assert_eq!(response.usage.completion_tokens, deserialized.usage.completion_tokens);
    assert_eq!(response.usage.total_tokens, deserialized.usage.total_tokens);
}

#[test]
fn test_openai_stream_response() {
    let stream_response = OpenAIStreamResponse {
        id: "chatcmpl-123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1677652288,
        model: "gpt-4".to_string(),
        system_fingerprint: Some("fp_123".to_string()),
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
    };
    
    let json = serde_json::to_string(&stream_response).unwrap();
    let deserialized: OpenAIStreamResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(stream_response.id, deserialized.id);
    assert_eq!(stream_response.object, deserialized.object);
    assert_eq!(stream_response.created, deserialized.created);
    assert_eq!(stream_response.model, deserialized.model);
    
    assert_eq!(stream_response.choices.len(), 1);
    let choice = &deserialized.choices[0];
    assert_eq!(choice.index, 0);
    assert_eq!(choice.delta.role, Some("assistant".to_string()));
    assert_eq!(choice.delta.content, Some("Hello".to_string()));
    assert_eq!(choice.finish_reason, None);
}

#[test]
fn test_openai_error_response() {
    let error_response = OpenAIErrorResponse {
        error: OpenAIError {
            message: "Invalid request".to_string(),
            error_type: "invalid_request_error".to_string(),
            param: Some("model".to_string()),
            code: Some("invalid_model".to_string()),
        },
    };
    
    let json = serde_json::to_string(&error_response).unwrap();
    let deserialized: OpenAIErrorResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(error_response.error.message, deserialized.error.message);
    assert_eq!(error_response.error.error_type, deserialized.error.error_type);
    assert_eq!(error_response.error.param, deserialized.error.param);
    assert_eq!(error_response.error.code, deserialized.error.code);
}

#[test]
fn test_tool_calling_structures() {
    let tool = OpenAITool {
        tool_type: "function".to_string(),
        function: OpenAIFunction {
            name: "get_weather".to_string(),
            description: Some("Get current weather".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city name"
                    }
                },
                "required": ["location"]
            })),
        },
    };
    
    let json = serde_json::to_string(&tool).unwrap();
    let deserialized: OpenAITool = serde_json::from_str(&json).unwrap();
    
    assert_eq!(tool.tool_type, deserialized.tool_type);
    assert_eq!(tool.function.name, deserialized.function.name);
    assert_eq!(tool.function.description, deserialized.function.description);
    
    let tool_call = OpenAIToolCall {
        id: Some("call_123".to_string()),
        tool_type: Some("function".to_string()),
        function: OpenAIFunctionCall {
            name: Some("get_weather".to_string()),
            arguments: Some("{\"location\": \"San Francisco\"}".to_string()),
        },
    };
    
    let json = serde_json::to_string(&tool_call).unwrap();
    let deserialized: OpenAIToolCall = serde_json::from_str(&json).unwrap();
    
    assert_eq!(tool_call.id, deserialized.id);
    assert_eq!(tool_call.tool_type, deserialized.tool_type);
    assert_eq!(tool_call.function.name, deserialized.function.name);
    assert_eq!(tool_call.function.arguments, deserialized.function.arguments);
}

#[test]
fn test_default_implementations() {
    // Test ClaudeRequest default implementation
    let claude_default = ClaudeRequest::default();
    assert_eq!(claude_default.model, ""); // Default model may be empty
    assert_eq!(claude_default.max_tokens, 1000); // Default max_tokens may be 1000
    assert!(claude_default.messages.is_empty());
    assert_eq!(claude_default.system, None);
    
    // Test OpenAIRequest default implementation
    let openai_default = OpenAIRequest::default();
    assert_eq!(openai_default.model, "gpt-4o");
    assert!(openai_default.messages.is_empty());
    assert_eq!(openai_default.max_tokens, None);
}

#[test]
fn test_optional_fields_serialization() {
    // Test optional field serialization behavior
    let minimal_claude_request = ClaudeRequest {
        model: "claude-3-sonnet".to_string(),
        max_tokens: 100,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Text("Hello".to_string()),
        }],
        ..Default::default()
    };
    
    let json = serde_json::to_string(&minimal_claude_request).unwrap();
    
    // Ensure optional fields don't appear in JSON
    assert!(!json.contains("system"));
    assert!(!json.contains("temperature"));
    assert!(!json.contains("top_p"));
    assert!(!json.contains("stop_sequences"));
    
    let minimal_openai_request = OpenAIRequest {
        model: "gpt-4".to_string(),
        messages: vec![OpenAIMessage {
            role: "user".to_string(),
            content: Some(OpenAIContent::Text("Hello".to_string())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        ..Default::default()
    };
    
    let json = serde_json::to_string(&minimal_openai_request).unwrap();
    
    // Ensure optional fields don't appear in JSON
    assert!(!json.contains("max_tokens"));
    assert!(!json.contains("temperature"));
    assert!(!json.contains("top_p"));
    assert!(!json.contains("stop"));
}

#[test]
fn test_content_extraction_edge_cases() {
    // Test empty content
    let empty_text = ClaudeContent::Text("".to_string());
    assert_eq!(empty_text.extract_text(), "");
    assert!(!empty_text.has_images());
    
    let empty_blocks = ClaudeContent::Blocks(vec![]);
    assert_eq!(empty_blocks.extract_text(), "");
    assert!(!empty_blocks.has_images());
    
    // Test image-only content
    let image_only = ClaudeContent::Blocks(vec![
        ClaudeContentBlock::Image {
            source: ClaudeImageSource {
                source_type: "base64".to_string(),
                media_type: "image/jpeg".to_string(),
                data: "test".to_string(),
            },
        },
    ]);
    assert_eq!(image_only.extract_text(), "");
    assert!(image_only.has_images());
    
    // Test mixed content
    let mixed_content = ClaudeContent::Blocks(vec![
        ClaudeContentBlock::Text { text: "Before ".to_string() },
        ClaudeContentBlock::Image {
            source: ClaudeImageSource {
                source_type: "base64".to_string(),
                media_type: "image/jpeg".to_string(),
                data: "test".to_string(),
            },
        },
        ClaudeContentBlock::Text { text: "after".to_string() },
    ]);
    assert_eq!(mixed_content.extract_text(), "Before after");
    assert!(mixed_content.has_images());
}

#[test]
fn test_json_compatibility() {
    // Test compatibility with real API responses
    let claude_api_response = r#"{
        "id": "msg_01ABC123",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "Hello! I'm Claude, an AI assistant created by Anthropic. How can I help you today?"
            }
        ],
        "model": "claude-3-5-sonnet-20241022",
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 10,
            "output_tokens": 25
        }
    }"#;
    
    let parsed: ClaudeResponse = serde_json::from_str(claude_api_response).unwrap();
    assert_eq!(parsed.id, "msg_01ABC123");
    assert_eq!(parsed.role, "assistant");
    assert_eq!(parsed.stop_reason, Some("end_turn".to_string()));
    
    let openai_api_response = r#"{
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4",
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I assist you today?"
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 9,
            "completion_tokens": 12,
            "total_tokens": 21
        }
    }"#;
    
    let parsed: OpenAIResponse = serde_json::from_str(openai_api_response).unwrap();
    assert_eq!(parsed.id, "chatcmpl-123");
    assert_eq!(parsed.object, "chat.completion");
    assert_eq!(parsed.choices.len(), 1);
    assert_eq!(parsed.usage.total_tokens, 21);
}