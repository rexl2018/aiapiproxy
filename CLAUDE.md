# CLAUDE.md
## Project Overview
aiapiproxy is a Rust-based API proxy service focused on handling request/response conversion (e.g., OpenAI-compatible formats), flexible model configuration, and robust error handling for streaming responses. Key goals include maintaining Rust idioms, ensuring compatibility with upstream APIs, and resolving parsing issues (like "missing field 'name'" in streaming data).

## Key Components & Files
- **Configuration**: Managed in `src/config/settings.rs` (parses env vars and settings).
- **Handlers**: `src/handlers/health.rs` (health checks) and route-specific handlers for API requests.
- **Services**: 
  - `src/services/client.rs` (upstream API client logic)
  - `src/services/converter.rs` (response conversion/fixing streaming errors)
- **Testing**: `tests/` directory (e.g., `config_tests.rs`, `converter_tests.rs`) uses Rust’s built-in test framework.

## Common Claude Code Workflows
1. **Build**: Run `cargo build` to compile the project.
2. **Test**: Execute `cargo test` (focus on modified test files like `tests/converter_tests.rs`).
3. **Lint/Type Check**: Use `cargo clippy` (linting) and `cargo check` (type validation) to ensure code quality.
4. **Edit Code**: Prefer modifying existing files (e.g., update conversion logic in `src/services/converter.rs`) over creating new ones. For config changes, edit `src/config/settings.rs`.

## Claude Code Notes
- **Relative Paths**: All file paths (e.g., `src/config/settings.rs`) are required for tool use.
- **Recent Changes**: Prioritize files modified in recent commits (e.g., fixing JSON parsing, handling streaming errors).
- **Avoid Proactive Creation**: Only create new files if explicitly requested (e.g., new handlers/services).

🤖 Generated with [Claude Code](https://claude.ai/code)
Co-Authored-By: Claude <noreply@anthropic.com>