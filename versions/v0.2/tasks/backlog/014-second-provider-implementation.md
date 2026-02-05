# 014 — Second Provider Implementation

## Description

Implement a second LLM provider (Anthropic Claude or Ollama) to validate that the `Provider` trait genuinely supports multiple backends. This proves the abstraction works and isn't accidentally coupled to OpenAI's API shape.

## Goal

A user can switch between two different LLM providers by changing their `buddy.toml` configuration. Both providers support streaming and tool calls.

## Requirements

- Add a `provider.type` field to config to select the provider:
  ```toml
  [provider]
  type = "openai"  # or "anthropic"
  api_key = "..."
  model = "..."
  endpoint = "..."
  ```
- Implement the chosen provider (recommend Anthropic for maximum API-shape difference):
  - Streaming responses via the provider's native streaming format
  - Tool-call support (Anthropic uses `tool_use` content blocks, distinct from OpenAI's `tool_calls`)
  - Error mapping to `ProviderError` variants
- The provider is selected at startup based on `provider.type`:
  - Refactor `main.rs` to construct the correct provider dynamically
  - Use `Box<dyn Provider>` or an enum dispatch — whichever keeps the code cleanest
- If `provider.type` is omitted, default to `"openai"` for backward compatibility
- Update `buddy.example.toml` with examples for both providers

## Acceptance Criteria

- [ ] Setting `provider.type = "anthropic"` (or `"ollama"`) uses the new provider
- [ ] Setting `provider.type = "openai"` (or omitting it) continues to use `OpenAiProvider`
- [ ] The new provider streams responses token by token
- [ ] The new provider supports tool calls and works with the tool-call loop
- [ ] Invalid `provider.type` values produce a clear startup error
- [ ] No OpenAI-specific logic exists in shared code paths (provider, API layer, etc.)
- [ ] The `Provider` trait was NOT modified to accommodate the new provider (if it was, document why)

## Test Cases

- Unit: construct a request body for the new provider; assert it matches that provider's API spec
- Unit: parse a sample streaming response from the new provider; assert tokens are extracted correctly
- Unit: parse a tool-call response from the new provider; assert `Token::ToolCall` is produced
- Unit: map error status codes (401, 429, 500) to correct `ProviderError` variants
- Integration (requires live key): send a short prompt via the new provider; assert a non-empty streamed response
- Config with `provider.type = "openai"`: assert `OpenAiProvider` is constructed
- Config with no `provider.type`: assert `OpenAiProvider` is constructed (backward compat)
- Config with `provider.type = "invalid"`: assert clear error at startup
