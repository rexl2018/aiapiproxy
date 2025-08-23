//! Error handling module unit tests

use aiapiproxy::utils::error::*;
use aiapiproxy::utils::error::helpers::*;
use axum::http::StatusCode;
use serde_json;

#[test]
fn test_app_error_status_codes() {
    let test_cases = vec![
        (AppError::Authentication("test".to_string()), StatusCode::UNAUTHORIZED),
        (AppError::Authorization("test".to_string()), StatusCode::FORBIDDEN),
        (AppError::Validation("test".to_string()), StatusCode::BAD_REQUEST),
        (AppError::NotFound("test".to_string()), StatusCode::NOT_FOUND),
        (AppError::RateLimit, StatusCode::TOO_MANY_REQUESTS),
        (AppError::PayloadTooLarge, StatusCode::PAYLOAD_TOO_LARGE),
        (AppError::Timeout, StatusCode::REQUEST_TIMEOUT),
        (AppError::ServiceUnavailable("test".to_string()), StatusCode::SERVICE_UNAVAILABLE),
        (AppError::ExternalApi("test".to_string()), StatusCode::BAD_GATEWAY),
        (AppError::Internal("test".to_string()), StatusCode::INTERNAL_SERVER_ERROR),
        (AppError::Config(anyhow::anyhow!("test")), StatusCode::INTERNAL_SERVER_ERROR),
        (AppError::Conversion("test".to_string()), StatusCode::INTERNAL_SERVER_ERROR),
    ];
    
    for (error, expected_status) in test_cases {
        assert_eq!(error.status_code(), expected_status);
    }
}

#[test]
fn test_app_error_types() {
    let test_cases = vec![
        (AppError::Authentication("test".to_string()), "authentication_error"),
        (AppError::Authorization("test".to_string()), "permission_error"),
        (AppError::Validation("test".to_string()), "invalid_request_error"),
        (AppError::NotFound("test".to_string()), "not_found_error"),
        (AppError::RateLimit, "rate_limit_error"),
        (AppError::PayloadTooLarge, "invalid_request_error"),
        (AppError::Timeout, "timeout_error"),
        (AppError::ServiceUnavailable("test".to_string()), "overloaded_error"),
        (AppError::ExternalApi("test".to_string()), "api_error"),
        (AppError::Internal("test".to_string()), "api_error"),
        (AppError::Config(anyhow::anyhow!("test")), "api_error"),
        (AppError::Conversion("test".to_string()), "api_error"),
    ];
    
    for (error, expected_type) in test_cases {
        assert_eq!(error.error_type(), expected_type);
    }
}

#[test]
fn test_should_log_details() {
    // 认证和授权错误不应该记录详细信息
    assert!(!AppError::Authentication("test".to_string()).should_log_details());
    assert!(!AppError::Authorization("test".to_string()).should_log_details());
    
    // 其他错误应该记录详细信息
    assert!(AppError::Validation("test".to_string()).should_log_details());
    assert!(AppError::Internal("test".to_string()).should_log_details());
    assert!(AppError::ExternalApi("test".to_string()).should_log_details());
    assert!(AppError::RateLimit.should_log_details());
}

#[test]
fn test_to_claude_error() {
    let app_error = AppError::Validation("Invalid input parameter".to_string());
    let claude_error = app_error.to_claude_error();
    
    assert_eq!(claude_error.error_type, "error");
    assert_eq!(claude_error.error.error_type, "invalid_request_error");
    assert_eq!(claude_error.error.message, "请求验证失败: Invalid input parameter");
}

#[test]
fn test_error_helpers() {
    // 测试认证错误助手
    let auth_err = auth_error("Invalid token");
    assert!(matches!(auth_err, AppError::Authentication(_)));
    assert_eq!(auth_err.to_string(), "认证失败: Invalid token");
    
    // 测试验证错误助手
    let validation_err = validation_error("Missing field");
    assert!(matches!(validation_err, AppError::Validation(_)));
    assert_eq!(validation_err.to_string(), "请求验证失败: Missing field");
    
    // 测试转换错误助手
    let conversion_err = conversion_error("Format mismatch");
    assert!(matches!(conversion_err, AppError::Conversion(_)));
    assert_eq!(conversion_err.to_string(), "API转换失败: Format mismatch");
    
    // 测试外部API错误助手
    let external_err = external_api_error("OpenAI API failed");
    assert!(matches!(external_err, AppError::ExternalApi(_)));
    assert_eq!(external_err.to_string(), "外部API错误: OpenAI API failed");
    
    // 测试内部错误助手
    let internal_err = internal_error("Database connection failed");
    assert!(matches!(internal_err, AppError::Internal(_)));
    assert_eq!(internal_err.to_string(), "内部服务器错误: Database connection failed");
    
    // 测试服务不可用错误助手
    let unavailable_err = service_unavailable_error("Service overloaded");
    assert!(matches!(unavailable_err, AppError::ServiceUnavailable(_)));
    assert_eq!(unavailable_err.to_string(), "服务暂时不可用: Service overloaded");
}

#[tokio::test]
async fn test_error_context_trait() {
    // 测试验证上下文
    let result: Result<(), std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found"
    ));
    
    let app_result = result.validation_context("Failed to read config");
    assert!(app_result.is_err());
    
    if let Err(AppError::Validation(msg)) = app_result {
        assert!(msg.contains("Failed to read config"));
        assert!(msg.contains("file not found"));
    } else {
        panic!("Expected validation error");
    }
    
    // Test conversion context
    let json_error: serde_json::Error = serde_json::from_str::<serde_json::Value>("{invalid json").unwrap_err();
    let result: Result<(), serde_json::Error> = Err(json_error);
    
    let app_result = result.conversion_context("Failed to parse JSON");
    assert!(app_result.is_err());
    
    if let Err(AppError::Conversion(msg)) = app_result {
        assert!(msg.contains("Failed to parse JSON"));
    } else {
        panic!("Expected conversion error");
    }
    
    // Test external API context
    let client = reqwest::Client::new();
    let reqwest_error = client.get("http://invalid-url-that-does-not-exist.invalid")
        .send()
        .await
        .unwrap_err();
    let result: Result<(), reqwest::Error> = Err(reqwest_error);
    
    let app_result = result.external_api_context("OpenAI request failed");
    assert!(app_result.is_err());
    
    if let Err(AppError::ExternalApi(msg)) = app_result {
        assert!(msg.contains("OpenAI request failed"));
    } else {
        panic!("Expected external API error");
    }
    
    // 测试内部上下文
    let result: Result<(), std::fmt::Error> = Err(std::fmt::Error);
    
    let app_result = result.internal_context("Formatting failed");
    assert!(app_result.is_err());
    
    if let Err(AppError::Internal(msg)) = app_result {
        assert!(msg.contains("Formatting failed"));
    } else {
        panic!("Expected internal error");
    }
}

#[test]
fn test_error_response_serialization() {
    let error_response = ErrorResponse {
        error_type: "invalid_request_error".to_string(),
        message: "Missing required field".to_string(),
        code: Some("missing_field".to_string()),
        details: Some(serde_json::json!({
            "field": "model",
            "location": "body"
        })),
        request_id: Some("req_123".to_string()),
    };
    
    let json = serde_json::to_string(&error_response).unwrap();
    let deserialized: ErrorResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(deserialized.error_type, "invalid_request_error");
    assert_eq!(deserialized.message, "Missing required field");
    assert_eq!(deserialized.code, Some("missing_field".to_string()));
    assert_eq!(deserialized.request_id, Some("req_123".to_string()));
    assert!(deserialized.details.is_some());
}

#[test]
fn test_claude_error_response_serialization() {
    let claude_error = ClaudeErrorResponse {
        error_type: "error".to_string(),
        error: ClaudeError {
            error_type: "authentication_error".to_string(),
            message: "Invalid API key".to_string(),
        },
    };
    
    let json = serde_json::to_string(&claude_error).unwrap();
    let deserialized: ClaudeErrorResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(deserialized.error_type, "error");
    assert_eq!(deserialized.error.error_type, "authentication_error");
    assert_eq!(deserialized.error.message, "Invalid API key");
}

#[test]
fn test_error_chain() {
    // 测试错误链
    let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied");
    let config_error = anyhow::Error::from(io_error);
    let app_error = AppError::Config(config_error);
    
    assert_eq!(app_error.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(app_error.error_type(), "api_error");
    assert!(app_error.should_log_details());
    
    let error_string = app_error.to_string();
    assert!(error_string.contains("配置错误"));
}

#[tokio::test]
async fn test_error_from_conversions() {
    // 测试从anyhow::Error的转换
    let anyhow_error = anyhow::anyhow!("Configuration failed");
    let app_error: AppError = anyhow_error.into();
    assert!(matches!(app_error, AppError::Config(_)));
    
    // Test conversion from reqwest::Error
    // Create a reqwest error by making an invalid request
    let client = reqwest::Client::new();
    let reqwest_error = client.get("http://invalid-url-that-does-not-exist.invalid")
        .send()
        .await
        .unwrap_err();
    let app_error: AppError = reqwest_error.into();
    assert!(matches!(app_error, AppError::HttpClient(_)));
    
    // Test conversion from serde_json::Error
    let json_error: serde_json::Error = serde_json::from_str::<serde_json::Value>("{invalid json").unwrap_err();
    let app_error: AppError = json_error.into();
    assert!(matches!(app_error, AppError::Serialization(_)));
}

#[test]
fn test_app_result_type() {
    // 测试AppResult类型别名
    let success: AppResult<String> = Ok("success".to_string());
    assert!(success.is_ok());
    assert_eq!(success.unwrap(), "success");
    
    let failure: AppResult<String> = Err(AppError::Validation("test".to_string()));
    assert!(failure.is_err());
    
    if let Err(AppError::Validation(msg)) = failure {
        assert_eq!(msg, "test");
    } else {
        panic!("Expected validation error");
    }
}

#[test]
fn test_error_display_formatting() {
    let errors = vec![
        AppError::Authentication("Invalid token".to_string()),
        AppError::Authorization("Insufficient permissions".to_string()),
        AppError::Validation("Missing field".to_string()),
        AppError::Conversion("Format error".to_string()),
        AppError::ExternalApi("API unavailable".to_string()),
        AppError::RateLimit,
        AppError::ServiceUnavailable("Overloaded".to_string()),
        AppError::Internal("Database error".to_string()),
        AppError::Timeout,
        AppError::NotFound("Resource not found".to_string()),
        AppError::PayloadTooLarge,
    ];
    
    for error in errors {
        let display_string = error.to_string();
        assert!(!display_string.is_empty());
        
        // 确保错误消息包含中文描述
        match error {
            AppError::Authentication(_) => assert!(display_string.contains("认证失败")),
            AppError::Authorization(_) => assert!(display_string.contains("权限不足")),
            AppError::Validation(_) => assert!(display_string.contains("请求验证失败")),
            AppError::Conversion(_) => assert!(display_string.contains("API转换失败")),
            AppError::ExternalApi(_) => assert!(display_string.contains("外部API错误")),
            AppError::RateLimit => assert!(display_string.contains("请求过于频繁")),
            AppError::ServiceUnavailable(_) => assert!(display_string.contains("服务暂时不可用")),
            AppError::Internal(_) => assert!(display_string.contains("内部服务器错误")),
            AppError::Timeout => assert!(display_string.contains("请求超时")),
            AppError::NotFound(_) => assert!(display_string.contains("资源未找到")),
            AppError::PayloadTooLarge => assert!(display_string.contains("请求体过大")),
            _ => {}
        }
    }
}