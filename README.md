# AI API Proxy

A high-performance HTTP proxy server written in Rust for converting Claude API requests to multiple AI model providers.

## 🚀 Features

- **Multi-Provider Support**: Route requests to OpenAI, ModelHub (Responses API & Gemini), and more
- **API Format Conversion**: Complete support for Claude API to OpenAI API request/response conversion
- **Streaming Response**: Support for Server-Sent Events streaming data transmission
- **Flexible Model Mapping**: Map Claude models to any provider/model via JSON configuration
- **JSON Configuration**: Simple `aiapiproxy.json` file for all settings
- **Error Handling**: Unified error format and Claude-compatible error responses
- **Health Checks**: Multi-level service status monitoring
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

2. **Configure the service**
   ```bash
   # Copy the example configuration
   cp aiapiproxy.example.json ~/.config/aiapiproxy/aiapiproxy.json
   # Or place it in the project directory as aiapiproxy.json
   
   # Edit the configuration file with your API keys and providers
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
   # Prepare config file
   mkdir -p ~/.config/aiapiproxy
   cp aiapiproxy.example.json ~/.config/aiapiproxy/aiapiproxy.json
   # Edit the config file with your settings
   
   # Start the service
   docker-compose up -d
   ```

2. **Using Docker**
   ```bash
   # Build image
   docker build -t aiapiproxy .
   
   # Run container (mount config file)
   docker run -d \
     --name aiapiproxy \
     -p 8082:8082 \
     -v ~/.config/aiapiproxy:/root/.config/aiapiproxy:ro \
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

### Configuration File

The service is configured via a JSON file. The config file is loaded from:
1. `~/.config/aiapiproxy/aiapiproxy.json` (recommended)
2. `./aiapiproxy.json` (current directory)

See `aiapiproxy.example.json` for a complete example.

### Configuration Structure

```json
{
  "providers": {
    "provider-name": {
      "type": "openai | modelhub",
      "baseUrl": "https://api.example.com/v1",
      "apiKey": "your-api-key",
      "options": {
        "apiKeyParam": "ak",
        "mode": "responses | gemini",
        "headers": {}
      },
      "models": {
        "model-id": {
          "name": "actual-model-name",
          "alias": "short-alias",
          "maxTokens": 8192
        }
      }
    }
  },
  "modelMapping": {
    "claude-3-5-sonnet-20241022": "provider-name/model-id",
    "sonnet": "provider-name/model-id"
  }
}
```

### Provider Types

| Type | Description | Mode Options |
|------|-------------|--------------|
| `openai` | Standard OpenAI API | - |
| `modelhub` | ModelHub proxy | `responses` (Responses API), `gemini` (Gemini via /v2/crawl) |

### Model Mapping

The `modelMapping` section maps Claude model names to `provider/model` paths:

```json
{
  "modelMapping": {
    "claude-3-5-sonnet-20241022": "modelhub-sg1/gpt-5-codex",
    "claude-3-opus-20240229": "openai/gpt-4o",
    "sonnet": "modelhub-sg1/gpt-5-codex"
  }
}
```

### Environment Variables

| Variable Name | Description | Default Value |
|---------------|-------------|---------------|
| `RUST_LOG` | Log level | `info` |

> **Note**: Server host and port are configured in the JSON configuration file, not via environment variables.

## 🏗️ Project Architecture

```
src/
├── config/          # Configuration management
│   ├── mod.rs
│   ├── file.rs      # JSON config loader
│   └── settings.rs  # Server settings
├── handlers/        # HTTP handlers
│   ├── health.rs    # Health checks
│   ├── mod.rs       # AppState & router setup
│   └── proxy.rs     # Claude API proxy handling
├── middleware/      # Middleware
│   ├── auth.rs      # Authentication middleware
│   ├── logging.rs   # Logging middleware
│   └── mod.rs
├── models/          # Data models
│   ├── claude.rs    # Claude API models
│   ├── mod.rs
│   └── openai.rs    # OpenAI API models
├── providers/       # Provider implementations
│   ├── mod.rs       # Provider trait
│   ├── openai.rs    # OpenAI provider
│   └── modelhub.rs  # ModelHub provider (responses & gemini modes)
├── services/        # Service layer
│   ├── client.rs    # HTTP client
│   ├── converter.rs # Claude <-> OpenAI converter
│   ├── router.rs    # Request router (model -> provider)
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
   - Check if `aiapiproxy.json` exists and is valid JSON
   - Confirm that port 8082 is not occupied
   - Check log output for detailed error information

2. **API Request Failure**
   - Verify that the request format complies with Claude API specifications
   - Check if the model is correctly mapped in `modelMapping`
   - Confirm that the provider API key is valid

3. **Model Not Found Error**
   - Add the Claude model to `modelMapping` section
   - Verify the target `provider/model` path exists in `providers`

4. **Performance Issues**
   - Check network latency and provider API response time
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

**Note**: This project is only used for API format conversion and does not store or record any user data. All requests are forwarded directly to the configured provider APIs.