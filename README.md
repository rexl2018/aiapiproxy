# AI API Proxy

A high-performance HTTP proxy server written in Rust for converting Claude API requests to OpenAI API format.

## 🚀 Features

- **API Format Conversion**: Complete support for Claude API to OpenAI API request/response conversion
- **Streaming Response**: Support for Server-Sent Events streaming data transmission
- **Model Mapping**: Configurable Claude model to OpenAI model mapping
- **Authentication Security**: API key validation and security middleware
- **Error Handling**: Unified error format and Claude-compatible error responses
- **Health Checks**: Multi-level service status monitoring
- **Logging**: Structured logging and security event monitoring
- **High Performance**: Asynchronous architecture based on Rust and Axum

## 📋 Tech Stack

- **Language**: Rust
- **HTTP Framework**: Axum + Tokio
- **HTTP Client**: Reqwest (with retry mechanism)
- **Serialization**: Serde + serde_json
- **Configuration Management**: config + dotenv
- **Logging System**: tracing + tracing-subscriber
- **Error Handling**: anyhow + thiserror

## 🛠️ Quick Start

### Requirements

- Rust 1.75+
- OpenAI API Key

### Local Development

1. **Clone the project**
   ```bash
   git clone <repository-url>
   cd aiapiproxy
   ```

2. **Configure environment variables**
   ```bash
   cp .env.example .env
   # Edit the .env file and set your OpenAI API key
   ```

3. **Start the service**
   ```bash
   # Use startup script (recommended)
   ./scripts/start.sh
   
   # Or use cargo directly
   cargo run
   ```

4. **Verify the service**
   ```bash
   curl http://localhost:8082/health
   ```

### Docker Deployment

1. **Using Docker Compose (Recommended)**
   ```bash
   # Set environment variables
   export OPENAI_API_KEY=sk-your-api-key-here
   
   # Start the service
   docker-compose up -d
   ```

2. **Using Docker**
   ```bash
   # Build image
   docker build -t aiapiproxy .
   
   # Run container
   docker run -d \
     --name aiapiproxy \
     -p 8082:8082 \
     -e OPENAI_API_KEY=sk-your-api-key-here \
     aiapiproxy
   ```

## 📖 Usage Guide

### API Endpoints

- **Health Check**: `GET /health`
- **Readiness Check**: `GET /health/ready`
- **Liveness Check**: `GET /health/live`
- **Claude Messages API**: `POST /v1/messages`

### Usage Examples

Send Claude API format requests to the proxy server:

```bash
curl -X POST http://localhost:8082/v1/messages \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-your-claude-api-key" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "max_tokens": 1024,
    "messages": [
      {
        "role": "user",
        "content": "Hello, how are you?"
      }
    ]
  }'
```

### Streaming Requests

```bash
curl -X POST http://localhost:8082/v1/messages \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-your-claude-api-key" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "max_tokens": 1024,
    "stream": true,
    "messages": [
      {
        "role": "user",
        "content": "Tell me a story"
      }
    ]
  }'
```

## ⚙️ Configuration

### Environment Variables

| Variable Name | Description | Default Value | Required |
|---------------|-------------|---------------|----------|
| `OPENAI_API_KEY` | OpenAI API Key | - | ✅ |
| `SERVER_HOST` | Server listening address | `0.0.0.0` | ❌ |
| `SERVER_PORT` | Server port | `8082` | ❌ |
| `OPENAI_BASE_URL` | OpenAI API base URL | `https://api.openai.com/v1` | ❌ |
| `CLAUDE_HAIKU_MODEL` | OpenAI model corresponding to Claude Haiku | `gpt-4o-mini` | ❌ |
| `CLAUDE_SONNET_MODEL` | OpenAI model corresponding to Claude Sonnet | `gpt-4o` | ❌ |
| `CLAUDE_OPUS_MODEL` | OpenAI model corresponding to Claude Opus | `gpt-4` | ❌ |
| `REQUEST_TIMEOUT` | Request timeout (seconds) | `30` | ❌ |
| `MAX_REQUEST_SIZE` | Maximum request size (bytes) | `10485760` | ❌ |
| `MAX_CONCURRENT_REQUESTS` | Maximum concurrent requests | `100` | ❌ |
| `RUST_LOG` | Log level | `info` | ❌ |
| `LOG_FORMAT` | Log format (text/json) | `text` | ❌ |
| `ALLOWED_ORIGINS` | Allowed CORS origins | `*` | ❌ |
| `CORS_ENABLED` | Whether to enable CORS | `true` | ❌ |

### Model Mapping

The proxy server automatically maps Claude models to corresponding OpenAI models:

| Claude Model | Default OpenAI Model | Description |
|--------------|---------------------|-------------|
| claude-3-haiku-* | gpt-4o-mini | Fast, economical model |
| claude-3-sonnet-* | gpt-4o | Balanced performance and cost |
| claude-3-opus-* | gpt-4 | Highest quality model |

## 🏗️ Project Architecture

```
src/
├── config/          # Configuration management
│   ├── mod.rs
│   └── settings.rs
├── handlers/        # HTTP handlers
│   ├── health.rs    # Health checks
│   ├── mod.rs
│   └── proxy.rs     # Proxy request handling
├── middleware/      # Middleware
│   ├── auth.rs      # Authentication middleware
│   ├── logging.rs   # Logging middleware
│   └── mod.rs
├── models/          # Data models
│   ├── claude.rs    # Claude API models
│   ├── mod.rs
│   └── openai.rs    # OpenAI API models
├── services/        # Service layer
│   ├── client.rs    # HTTP client
│   ├── converter.rs # API converter
│   └── mod.rs
├── utils/           # Utility modules
│   ├── error.rs     # Error handling
│   └── mod.rs
├── lib.rs           # Library entry point
└── main.rs          # Program entry point
```

## 🧪 Testing

```bash
# Run unit tests
cargo test

# Run integration tests
cargo test --test integration_tests

# Run performance tests
cargo bench
```

## 📊 Monitoring

### Health Check Endpoints

- **Basic Health Check**: `GET /health`
  - Returns basic service status
  
- **Readiness Check**: `GET /health/ready`
  - Checks if the service is ready to receive requests
  - Includes OpenAI API connection status
  
- **Liveness Check**: `GET /health/live`
  - Checks if the service is still running
  - Includes uptime and memory usage information

### Logging

The service supports structured logging, configurable via the `LOG_FORMAT` environment variable:

- `text`: Human-readable format (development environment)
- `json`: JSON format (production environment)

## 🔒 Security Features

- **API Key Validation**: Supports Bearer token and direct API key formats
- **Request Size Limits**: Prevents oversized request attacks
- **CORS Configuration**: Configurable cross-origin resource sharing
- **Security Logging**: Records suspicious requests and security events
- **Rate Limiting**: Client-based request frequency control

## 🚀 Performance Optimization

- **Asynchronous Architecture**: High-concurrency processing based on Tokio
- **Connection Reuse**: HTTP client connection pooling
- **Streaming Processing**: Support for large files and long response streaming
- **Memory Optimization**: Zero-copy and efficient memory management
- **Retry Mechanism**: Automatic retry of failed requests

## 🐛 Troubleshooting

### Common Issues

1. **Service Startup Failure**
   - Check if `OPENAI_API_KEY` is correctly set
   - Confirm that port 8082 is not occupied
   - Check log output for detailed error information

2. **API Request Failure**
   - Verify that the request format complies with Claude API specifications
   - Check if the authentication header is correct
   - Confirm that the OpenAI API key is valid and has sufficient balance

3. **Performance Issues**
   - Adjust the `MAX_CONCURRENT_REQUESTS` parameter
   - Check network latency and OpenAI API response time
   - Monitor memory and CPU usage

### Debug Mode

```bash
# Enable debug logging
export RUST_LOG=debug
./scripts/start.sh

# Or use development mode
export DEV_MODE=true
export LOG_FORMAT=text
cargo run
```

## 📝 Development Guide

### Adding New Features

1. Add code to the appropriate module
2. Write unit tests
3. Update documentation
4. Run the complete test suite

### Code Style

The project uses standard Rust code style:

```bash
# Format code
cargo fmt

# Check code
cargo clippy
```

## 📄 License

MIT License

## 🤝 Contributing

Welcome to submit Issues and Pull Requests!

## 📞 Support

If you encounter problems or need help, please:

1. Check the troubleshooting section of this documentation
2. Search existing Issues
3. Create a new Issue with detailed information

---

**Note**: This project is only used for API format conversion and does not store or record any user data. All requests are forwarded directly to the OpenAI API.