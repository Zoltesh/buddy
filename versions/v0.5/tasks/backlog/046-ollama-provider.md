# 046 — Ollama Provider

## Description

Ollama is a popular tool for running LLMs locally. It exposes an OpenAI-compatible API at `http://localhost:11434/v1/chat/completions`, making integration straightforward. This task adds an Ollama provider that reuses the existing OpenAI request-building and SSE-parsing logic (from `provider/openai.rs`) with Ollama-specific defaults: no API key, a different default endpoint, and the `"ollama"` provider type.

## Goal

Users can add an Ollama provider in the Models settings, point it at their Ollama instance, and use any locally-running Ollama model for chat.

## Requirements

- Add `provider/ollama.rs` with an `OllamaProvider` struct
- Constructor: `OllamaProvider::new(model: String, endpoint: String, system_prompt: String)`
  - No `api_key` parameter — Ollama does not require authentication
  - Default endpoint (used when config `endpoint` is absent): `http://localhost:11434`
- The provider calls `{endpoint}/v1/chat/completions` (Ollama's OpenAI compatibility layer)
- Reuse the request body building and SSE response parsing from `openai.rs`:
  - Import and call `build_request_body()` and `parse_sse_stream()` (or equivalent shared functions)
  - If these functions are not already public, make them `pub(crate)` in `openai.rs`
  - Do NOT duplicate the parsing logic
- Implement the `Provider` trait for `OllamaProvider`
- Add `Ollama` variant to `ProviderType` enum in `config.rs` (serializes/deserializes as `"ollama"`)
- Add `Ollama(OllamaProvider)` variant to the `AnyProvider` enum in `provider/mod.rs`
- Update `reload::build_provider_chain()` to construct `OllamaProvider` when `provider_type` is `"ollama"`
- Update `POST /api/config/test-provider` to handle `"ollama"` type (same as OpenAI test but without API key)
- Ollama provider config in `buddy.toml`:
  ```toml
  [[models.chat.providers]]
  type = "ollama"
  model = "llama3"
  endpoint = "http://localhost:11434"   # optional, this is the default
  ```
  - `api_key_env` is accepted but ignored (for consistency, not an error if present)
- Do not add model discovery (that is a separate future task)

## Acceptance Criteria

- [ ] `provider/ollama.rs` exists with `OllamaProvider` implementing the `Provider` trait
- [ ] `OllamaProvider` reuses OpenAI request building and SSE parsing — no duplicated parsing logic
- [ ] `ProviderType` enum has an `Ollama` variant that serializes as `"ollama"`
- [ ] `AnyProvider` enum has an `Ollama` variant
- [ ] `build_provider_chain()` constructs `OllamaProvider` for `"ollama"` type
- [ ] No API key is sent in requests to Ollama
- [ ] Default endpoint is `http://localhost:11434` when config `endpoint` is absent
- [ ] `POST /api/config/test-provider` works for `"ollama"` type
- [ ] Config with `type = "ollama"` parses correctly
- [ ] All existing tests pass

## Test Cases

- [ ] Parse a config TOML with `type = "ollama"`, `model = "llama3"`, no endpoint; assert `ProviderType::Ollama` and default endpoint `http://localhost:11434`
- [ ] Parse a config TOML with `type = "ollama"` and a custom endpoint; assert the custom endpoint is used
- [ ] Build a provider chain with an Ollama entry; assert the chain contains an `OllamaProvider`
- [ ] Send a request through `OllamaProvider` to a mock HTTP server; assert the request hits `{endpoint}/v1/chat/completions`, has no `Authorization` header, and the body matches OpenAI chat completion format
- [ ] Send a request through `OllamaProvider` to an unreachable endpoint; assert `ProviderError::Network`
- [ ] POST to `/api/config/test-provider` with `type: "ollama"` and an unreachable endpoint; assert `{ "status": "error" }` with a connection error message
