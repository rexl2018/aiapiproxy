# Plan: AIAPIProxy Provider/Model Refactor

## Context

### Original Request
Refactor `aiapiproxy` to support multiple providers and models via a 2-level configuration (`provider` -> `model`), similar to `opencode`. Add a new `modelhub` provider that supports both OpenAI and Gemini protocols (specifically the `opencode` implementation).

### Interview Summary
**Key Decisions**:
- **Configuration**: Migrate model config from `.env` to `~/.config/aiapiproxy/aiapiproxy.json`.
- **Addressing**: Use `{provider}/{model}` format in API requests.
- **ModelHub Provider**: Implement `modelhub` with `ak` param auth and two modes: `responses` (OpenAI-compatible) and `gemini` (custom adapter).
- **Scope**: Explicitly exclude complex text-based tool parsing (`[tool_call: ...]`) found in `opencode`; support standard `tool_calls` only.

### Metis Review
**Identified Gaps** (addressed):
- **Config Lifecycle**: Assumed "Restart on Change" (no hot reload).
- **Migration**: Partial migration - `SERVER_*` and `LOG_*` stay in `.env`; Upstream config moves to JSON.
- **Protocol Parity**: Implement `thought_signature` pass-through for Gemini mode.

---

## Work Objectives

### Core Objective
Refactor `aiapiproxy` from a single-endpoint proxy to a multi-provider routing gateway, adding robust support for the `modelhub` provider (including its Gemini protocol adapter).

### Concrete Deliverables
1.  `src/config/mod.rs` & `file.rs`: New JSON configuration loading logic.
2.  `src/providers/mod.rs`: `Provider` trait definition.
3.  `src/providers/openai.rs`: Standard OpenAI implementation.
4.  `src/providers/modelhub.rs`: ModelHub implementation (OpenAI + Gemini adapter).
5.  `src/services/router.rs`: Router logic to dispatch requests based on `{provider}/{model}`.
6.  `~/.config/aiapiproxy/aiapiproxy.json`: Example configuration.

### Definition of Done
- [x] `aiapiproxy` loads config from JSON file on startup.
- [x] Request to `provider-a/model-x` is routed to correct upstream.
- [x] `modelhub` provider works in `responses` mode (OpenAI pass-through).
- [x] `modelhub` provider works in `gemini` mode (transforms `/v2/crawl` & SSE).
- [x] Existing `Claude -> OpenAI` conversion logic is preserved and used.

### Must Have
- `ak` query parameter support for ModelHub.
- `thought_signature` preservation in Gemini mode (for multi-turn tools).
- Tool schema sanitization for Gemini mode (drop `anyOf`/`allOf`).
- JSON configuration structure matching `opencode`.

### Must NOT Have (Guardrails)
- Hot reloading of configuration.
- Support for `[tool_call: ...]` text-based parsing (standard JSON tool calls only).
- Support for other `modelhub` modes (only `responses` and `gemini`).
- Modification of `SERVER_` or `LOG_` environment variable handling.

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: Yes (Rust `cargo test`).
- **User wants tests**: Implied (robust refactor).
- **Approach**: TDD for the adapters (Gemini protocol transformation).

### If TDD Enabled
- **Gemini Adapter**: Write tests for `OpenAIRequest -> GeminiRequest` transformation first.
- **SSE Parser**: Write tests for `Gemini SSE -> OpenAI SSE` transformation first.

### Manual Verification Procedures

**1. OpenAI Mode Verification:**
- [ ] Configure `modelhub` in `responses` mode in `aiapiproxy.json`.
- [ ] Run `curl` to `localhost:8082/v1/messages` with model `modelhub-openai/gpt-4`.
- [ ] Verify upstream receives `ak` param.
- [ ] Verify response is correct.

**2. Gemini Mode Verification:**
- [ ] Configure `modelhub` in `gemini` mode.
- [ ] Run `curl` with a tool-use prompt.
- [ ] Verify request to upstream is sent to `/v2/crawl`.
- [ ] Verify tool schema is sanitized (no `anyOf`).
- [ ] Verify `thought_signature` is handled if returned.

---

## Task Flow
```
Config Structs -> Provider Trait -> OpenAI Provider
                                 ↘ ModelHub Provider (Gemini) -> Router Integration -> Cleanup
```

## TODOs

- [x] 1. Define Configuration Structures
  **What to do**:
  - Create `src/config/file.rs`.
  - Define `AppConfig`, `ProviderConfig`, `ModelConfig` structs deriving `Deserialize`.
  - Implement `AppConfig::load(path: &Path)`.
  - Ensure compatibility with `opencode` JSON structure (see draft).

  **References**:
  - `opencode.json` structure from conversation history.
  - `src/config/settings.rs` (existing).

  **Acceptance Criteria**:
  - [ ] `cargo test` passes for loading a sample `aiapiproxy.json`.
  - [ ] Correctly parses `options.apiKeyParam` and `models`.

- [x] 2. Define Provider Trait
  **What to do**:
  - Create `src/providers/mod.rs`.
  - Define `trait Provider: Send + Sync`.
  - Methods:
    - `chat_complete(&self, req: OpenAIRequest, model_config: &ModelConfig) -> Result<OpenAIResponse>`.
    - `chat_stream(&self, req: OpenAIRequest, model_config: &ModelConfig) -> Result<BoxStream<OpenAIStreamResponse>>`.

  **References**:
  - `src/services/client.rs` (current implementation).

  **Acceptance Criteria**:
  - [ ] Trait compiles and covers both streaming and non-streaming.

- [x] 3. Implement OpenAI Provider
  **What to do**:
  - Create `src/providers/openai.rs`.
  - Move logic from `src/services/client.rs` into this provider.
  - Implement `Provider` trait.

  **References**:
  - `src/services/client.rs` (source of truth).

  **Acceptance Criteria**:
  - [ ] Existing tests for client pass (after adaptation).

- [x] 4. Implement ModelHub Provider (Skeleton & OpenAI Mode)
  **What to do**:
  - Create `src/providers/modelhub.rs`.
  - Implement `Provider` trait.
  - Handle `ak` parameter injection (from config or env).
  - Implement `responses` mode (delegates to internal OpenAI logic but with `ak` param).

  **References**:
  - `packages/opencode/src/provider/custom-loaders/modelhub.ts` (logic).

  **Acceptance Criteria**:
  - [ ] Request includes `?ak=...`.
  - [ ] Headers include `HTTP-Referer` and `X-Title`.

- [x] 5. Implement Gemini Adapter Logic (TDD)
  **What to do**:
  - In `src/providers/modelhub.rs`.
  - Implement `convert_request(OpenAIRequest) -> GeminiRequest`.
  - Implement `sanitize_tools(tools)`.
  - Implement `parse_gemini_sse(stream) -> OpenAIStream`.
  - **CRITICAL**: Handle `thought_signature` injection in tool calls.

  **References**:
  - `packages/opencode/src/provider/sdk/modelhub-gemini/modelhub-gemini-language-model.ts` (logic source).

  **Acceptance Criteria**:
  - [ ] Unit tests for `sanitize_tools` pass.
  - [ ] Unit tests for SSE parsing pass (simulated Gemini stream).

- [x] 6. Integrate Gemini Mode into ModelHub Provider
  **What to do**:
  - Wire up the adapter logic in `chat_complete` and `chat_stream` when `mode == "gemini"`.
  - Use `/v2/crawl` endpoint.

  **Acceptance Criteria**:
  - [ ] `mode="gemini"` triggers correct path.

- [x] 7. Implement Router & Integration
  **What to do**:
  - Create `src/services/router.rs`.
  - Holds `HashMap<String, Box<dyn Provider>>`.
  - Method `route(model_path: &str) -> (Provider, ModelConfig)`.
  - Update `src/handlers/proxy.rs` to use Router.
  - Parse `model` field as `{provider}/{model}`.

  **Acceptance Criteria**:
  - [ ] Request to `modelhub-sg1/gpt-5` routes to `modelhub-sg1` provider with `gpt-5` config.

- [x] 8. Update Main & Cleanup
  **What to do**:
  - Update `main.rs` to load JSON config.
  - Initialize Router.
  - Deprecate/Remove old `OPENAI_*` env var usage (or keep as fallback provider "openai").

  **Acceptance Criteria**:
  - [ ] Server starts with valid JSON config.
  - [ ] Config loading failure logs error and exits.

---

## Success Criteria
- [x] All unit tests pass (`cargo test --lib`).
- [x] Configuration is successfully loaded from `~/.config/aiapiproxy/aiapiproxy.json`.
- [x] Requests to `provider/model` are successfully routed and executed.
