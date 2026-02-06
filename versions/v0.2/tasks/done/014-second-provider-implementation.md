# 014 — Second Provider Implementation

## Description

Implement a second LLM provider (Anthropic Claude or Ollama) to validate that the `Provider` trait genuinely supports multiple backends. This proves the abstraction works and isn't accidentally coupled to OpenAI's API shape.

## Goal

A user can switch between two different LLM providers by changing their `buddy.toml` configuration. Both providers support streaming and tool calls.

## Requirements

- Add a `provider.type` field to config to select the provider:
  ```toml
  [provider]
  type = "openai"  # or "lmstudio"
  api_key = "..."
  model = "..."
  endpoint = "..."
  ```
- Implement the chosen provider (LM Studio — OpenAI-compatible local server):
  - Streaming responses via SSE (same protocol as OpenAI)
  - Tool-call support (OpenAI-compatible format)
  - Error mapping to `ProviderError` variants
- The provider is selected at startup based on `provider.type`:
  - Refactor `main.rs` to construct the correct provider dynamically
  - Use an `AnyProvider` enum dispatch — keeps the code cleanest
- If `provider.type` is omitted, default to `"openai"` for backward compatibility
- Update `buddy.example.toml` with examples for both providers

## Acceptance Criteria

- [x] Setting `provider.type = "lmstudio"` uses the new provider
- [x] Setting `provider.type = "openai"` (or omitting it) continues to use `OpenAiProvider`
- [x] The new provider streams responses token by token
- [x] The new provider supports tool calls and works with the tool-call loop
- [x] Invalid `provider.type` values produce a clear startup error
- [x] No OpenAI-specific logic exists in shared code paths (provider, API layer, etc.)
- [x] The `Provider` trait was NOT modified to accommodate the new provider

## Test Cases

- Unit: construct a request body for the new provider; assert it matches that provider's API spec
- Unit: parse a sample streaming response from the new provider; assert tokens are extracted correctly
- Unit: parse a tool-call response from the new provider; assert `Token::ToolCall` is produced
- Unit: map error status codes (401, 429, 500) to correct `ProviderError` variants
- Integration (requires live key): send a short prompt via the new provider; assert a non-empty streamed response
- Config with `provider.type = "openai"`: assert `OpenAiProvider` is constructed
- Config with no `provider.type`: assert `OpenAiProvider` is constructed (backward compat)
- Config with `provider.type = "invalid"`: assert clear error at startup
