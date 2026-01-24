# CLAUDE.md

## Project Overview
aiapiproxy is a Rust-based API proxy service that converts Claude API requests to multiple AI model providers (OpenAI, ModelHub, etc.). It supports flexible model routing via JSON configuration, streaming responses, and robust error handling.

## Key Components & Files

### Configuration
- `src/config/file.rs` - JSON configuration loader (`~/.config/aiapiproxy/aiapiproxy.json`)
- `src/config/settings.rs` - Server settings (host, port)
- `aiapiproxy.example.json` - Example configuration file

### Providers
- `src/providers/mod.rs` - Provider trait definition
- `src/providers/openai.rs` - Standard OpenAI API provider
- `src/providers/modelhub.rs` - ModelHub provider with two modes:
  - `responses`: OpenAI Responses API (`/responses` endpoint)
  - `gemini`: Gemini via OpenAI chat format (`/v2/crawl` endpoint)

### Services
- `src/services/router.rs` - Request router (resolves model -> provider/model)
- `src/services/converter.rs` - Claude <-> OpenAI request/response conversion
- `src/services/client.rs` - HTTP client (legacy, mostly unused)

### Handlers
- `src/handlers/proxy.rs` - Claude API proxy endpoint (`/v1/messages`)
- `src/handlers/health.rs` - Health check endpoints
- `src/handlers/mod.rs` - AppState and Axum router setup

### Models
- `src/models/claude.rs` - Claude API request/response structures
- `src/models/openai.rs` - OpenAI API request/response structures

## Configuration Structure

```json
{
  "server": {
    "host": "127.0.0.1",
    "port": 8082
  },
  "providers": {
    "provider-name": {
      "type": "openai | modelhub",
      "baseUrl": "https://api.example.com",
      "apiKey": "...",
      "options": { "mode": "responses | gemini", "apiKeyParam": "ak" },
      "models": { "model-id": { "name": "actual-name", "maxTokens": 8192 } }
    }
  },
  "modelMapping": {
    "claude-3-5-sonnet-20241022": "provider-name/model-id"
  }
}
```

> **Security**: Default `host: "127.0.0.1"` only accepts local connections (e.g., Claude Code). Set `"0.0.0.0"` to allow remote access.

## Common Workflows

1. **Build**: `cargo build`
2. **Test**: `cargo test`
3. **Run**: `cargo run` (requires config file)
4. **Lint**: `cargo clippy`

## Request Flow

```
Client (Claude API format)
    ↓
/v1/messages handler
    ↓
ApiConverter (Claude -> OpenAI)
    ↓
Router.resolve_model() (modelMapping lookup)
    ↓
Provider.chat_complete() or Provider.chat_stream()
    ↓
ApiConverter (OpenAI -> Claude)
    ↓
Client (Claude API format response)
```

## Notes
- Config file is required (no fallback to env vars for model settings)
- Provider type determines which Provider implementation is used
- ModelHub `responses` mode uses `/responses` endpoint with Responses API format
- ModelHub `gemini` mode uses `/v2/crawl` endpoint with OpenAI chat format

🤖 Generated with [Claude Code](https://claude.ai/code)
Co-Authored-By: Claude <noreply@anthropic.com>
