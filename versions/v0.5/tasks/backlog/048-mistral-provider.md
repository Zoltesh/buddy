# 048 — Mistral Provider

## Description

Mistral's API is OpenAI-compatible — same endpoint structure, same request/response format, same SSE streaming. Like the Ollama provider, this task adds a thin wrapper that reuses the existing OpenAI parsing logic with Mistral-specific defaults: a different base endpoint and the `"mistral"` provider type.

## Goal

Users can add a Mistral provider in the Models settings and use Mistral models (e.g., mistral-large-latest) for chat.

## Requirements

- Add `provider/mistral.rs` with a `MistralProvider` struct
- Constructor: `MistralProvider::new(api_key: String, model: String, endpoint: String, system_prompt: String)`
  - Default endpoint (when config `endpoint` is absent): `https://api.mistral.ai`
- The provider calls `{endpoint}/v1/chat/completions` (standard OpenAI-compatible path)
- Reuse the request body building and SSE response parsing from `openai.rs`:
  - Same approach as the Ollama provider (task 046) — call shared `pub(crate)` functions
  - Do NOT duplicate parsing logic
- API key is sent as a Bearer token in the `Authorization` header (same as OpenAI)
- Implement the `Provider` trait for `MistralProvider`
- Add `Mistral` variant to `ProviderType` enum in `config.rs` (serializes as `"mistral"`)
- Add `Mistral(MistralProvider)` variant to `AnyProvider` enum
- Update `reload::build_provider_chain()` for `"mistral"` type
- Update `POST /api/config/test-provider` for `"mistral"` type
- Mistral config in `buddy.toml`:
  ```toml
  [[models.chat.providers]]
  type = "mistral"
  model = "mistral-large-latest"
  api_key_env = "MISTRAL_API_KEY"
  endpoint = "https://api.mistral.ai"   # optional, this is the default
  ```

## Acceptance Criteria

- [ ] `provider/mistral.rs` exists with `MistralProvider` implementing the `Provider` trait
- [ ] `MistralProvider` reuses OpenAI request building and SSE parsing — no duplicated parsing logic
- [ ] `ProviderType` enum has a `Mistral` variant that serializes as `"mistral"`
- [ ] `AnyProvider` enum has a `Mistral` variant
- [ ] `build_provider_chain()` constructs `MistralProvider` for `"mistral"` type
- [ ] API key is sent as a Bearer token in the Authorization header
- [ ] Default endpoint is `https://api.mistral.ai` when config `endpoint` is absent
- [ ] `POST /api/config/test-provider` works for `"mistral"` type
- [ ] Config with `type = "mistral"` parses correctly
- [ ] All existing tests pass

## Test Cases

- [ ] Parse a config TOML with `type = "mistral"`, `model = "mistral-large-latest"`, `api_key_env = "MISTRAL_API_KEY"`; assert `ProviderType::Mistral` and default endpoint `https://api.mistral.ai`
- [ ] Parse a config TOML with `type = "mistral"` and a custom endpoint; assert the custom endpoint is used
- [ ] Build a provider chain with a Mistral entry; assert the chain contains a `MistralProvider`
- [ ] Send a request through `MistralProvider` to a mock HTTP server; assert the request hits `{endpoint}/v1/chat/completions`, has an `Authorization: Bearer {key}` header, and the body matches OpenAI chat completion format
- [ ] Send a request through `MistralProvider` to an unreachable endpoint; assert `ProviderError::Network`
- [ ] Send a request through `MistralProvider` with an empty API key; verify the request is sent (Mistral returns an auth error, not a client-side failure)
- [ ] POST to `/api/config/test-provider` with `type: "mistral"` and a missing env var; assert error mentioning the env var name
