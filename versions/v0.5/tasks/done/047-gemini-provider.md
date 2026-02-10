# 047 — Google Gemini Provider

## Description

Google Gemini uses a different API format than OpenAI. Messages use `contents` with `parts` arrays, roles differ (`"model"` instead of `"assistant"`), the system prompt is a separate field, and streaming uses a different SSE format. This task implements a Gemini provider from scratch (no reuse of OpenAI parsing) with full support for chat completions and tool calling.

## Goal

Users can add a Google Gemini provider in the Models settings and use Gemini models (e.g., gemini-2.0-flash) for chat, including tool-calling skills.

## Requirements

- Add `provider/gemini.rs` with a `GeminiProvider` struct
- Constructor: `GeminiProvider::new(api_key: String, model: String, endpoint: String, system_prompt: String)`
  - Default endpoint (when config `endpoint` is absent): `https://generativelanguage.googleapis.com`
- API call: `POST {endpoint}/v1beta/models/{model}:streamGenerateContent?alt=sse&key={api_key}`
- Request body mapping from buddy's `Message` types to Gemini format:
  - `Role::User` → `role: "user"`
  - `Role::Assistant` → `role: "model"`
  - `Role::System` → not included in `contents`; placed in `systemInstruction: { parts: [{ text }] }`
  - `MessageContent::Text` → `parts: [{ text: "..." }]`
  - `MessageContent::ToolCall` → `parts: [{ functionCall: { name, args } }]`
  - `MessageContent::ToolResult` → `parts: [{ functionResponse: { name, response: { content } } }]`
  - Consecutive messages with the same role must be merged into a single `contents` entry (Gemini requires alternating roles)
- Tool definitions mapping: buddy's OpenAI-format tool JSON → Gemini's `tools: [{ functionDeclarations: [...] }]`
  - Each declaration: `{ name, description, parameters }` (parameters is the JSON Schema object)
- SSE response parsing:
  - Each SSE `data:` line contains a JSON object with `candidates[0].content.parts`
  - Text parts: `{ text: "..." }` → emit `Token::Text`
  - Function call parts: `{ functionCall: { name, args } }` → emit `Token::ToolCall`
  - `finishReason: "STOP"` → stream ends
- Error handling:
  - HTTP 400 with `{ error: { message, status } }` → `ProviderError::Auth` for auth errors, `ProviderError::MalformedResponse` for others
  - HTTP 429 → `ProviderError::RateLimit`
  - Connection failures → `ProviderError::Network`
- Add `Gemini` variant to `ProviderType` enum in `config.rs` (serializes as `"gemini"`)
- Add `Gemini(GeminiProvider)` variant to `AnyProvider` enum
- Update `reload::build_provider_chain()` for `"gemini"` type
- Update `POST /api/config/test-provider` for `"gemini"` type
- Gemini config in `buddy.toml`:
  ```toml
  [[models.chat.providers]]
  type = "gemini"
  model = "gemini-2.0-flash"
  api_key_env = "GEMINI_API_KEY"
  endpoint = "https://generativelanguage.googleapis.com"  # optional, this is the default
  ```

## Acceptance Criteria

- [x] `provider/gemini.rs` exists with `GeminiProvider` implementing the `Provider` trait
- [x] Messages are correctly mapped to Gemini's `contents` format with proper role names
- [x] System prompt is sent as `systemInstruction`, not in `contents`
- [x] Consecutive same-role messages are merged before sending
- [x] Tool definitions are mapped to Gemini's `functionDeclarations` format
- [x] SSE responses are parsed into `Token::Text` and `Token::ToolCall` correctly
- [x] `functionCall` responses produce valid `Token::ToolCall` with correct name and arguments
- [x] API key is sent as a query parameter
- [x] HTTP errors are mapped to appropriate `ProviderError` variants
- [x] Config with `type = "gemini"` parses correctly
- [x] All existing tests pass

## Test Cases

- [x] Parse a config TOML with `type = "gemini"`, `model = "gemini-2.0-flash"`, `api_key_env = "GEMINI_API_KEY"`; assert `ProviderType::Gemini`
- [x] Build a Gemini request body from a conversation with user, assistant, and system messages; assert system message is in `systemInstruction`, user/assistant are in `contents` with correct roles
- [x] Build a Gemini request body from a conversation where two consecutive user messages exist; assert they are merged into one `contents` entry with multiple parts
- [x] Build a Gemini request body with tools; assert `tools[0].functionDeclarations` contains the correct function names and parameter schemas
- [x] Parse a Gemini SSE stream containing text chunks; assert `Token::Text` tokens are emitted with the correct text
- [x] Parse a Gemini SSE stream containing a `functionCall` part; assert a `Token::ToolCall` is emitted with the correct name and arguments JSON
- [x] Send a request through `GeminiProvider` to a mock HTTP server; assert the URL contains the model name and API key as query parameter
- [x] Send a request to an unreachable endpoint; assert `ProviderError::Network`
- [x] POST to `/api/config/test-provider` with `type: "gemini"` and a missing env var; assert error mentioning the env var name
