# 032 — Provider Connection Test Endpoint

## Description

Add a `POST /api/config/test-provider` endpoint that takes a provider configuration and attempts to verify connectivity. This lets the Settings UI show success/failure inline when a user configures a model, before they commit the change.

## Goal

Users can test whether a provider endpoint is reachable and the credentials are valid directly from the Settings UI, getting immediate feedback without saving the config.

## Requirements

- Add `POST /api/config/test-provider` endpoint
- Request body matches a single `ProviderEntry`:
  ```json
  {
    "type": "openai",
    "model": "gpt-4",
    "endpoint": "https://api.openai.com/v1",
    "api_key_env": "OPENAI_API_KEY"
  }
  ```
- The endpoint resolves the API key from the env var (if provided), constructs a temporary provider instance, and attempts a lightweight connectivity check:
  - For OpenAI-compatible providers: send a minimal chat completion request (e.g., single user message "hi", `max_tokens: 1`) and check for a successful response
  - For LM Studio: same approach (it uses the OpenAI-compatible API)
  - For local embedding providers: attempt to embed a short test string
- Return a structured result:
  - Success: `{ "status": "ok", "message": "Connected successfully" }`
  - Failure: `{ "status": "error", "message": "Connection refused: ..." }` or `{ "status": "error", "message": "Authentication failed: invalid API key" }`
- The test must have a short timeout (5 seconds) to avoid blocking the UI
- If `api_key_env` is provided but the env var is not set, return an error immediately: `"Environment variable OPENAI_API_KEY is not set"`
- Do not persist anything — this is a dry-run connectivity check

## Acceptance Criteria

- [x] `POST /api/config/test-provider` accepts a provider entry and returns a result
- [x] Successful connection returns `{ "status": "ok" }`
- [x] Connection failure returns `{ "status": "error", "message": "..." }` with a descriptive message
- [x] Missing env var returns an immediate error without attempting connection
- [x] Unknown provider type returns a validation error
- [x] Request times out after 5 seconds if the endpoint is unresponsive
- [x] No state is modified by the test

## Test Cases

- [x] POST a provider entry with an unreachable endpoint (e.g., `http://127.0.0.1:1`); assert `status: "error"` with a connection error message
- [x] POST a provider entry with a missing `api_key_env` value (env var not set); assert `status: "error"` mentioning the env var name
- [x] POST a provider entry with an unknown `type`; assert 400 validation error
- [x] POST a provider entry with an empty `model`; assert 400 validation error
- [x] Assert the test endpoint does not modify the in-memory config or `buddy.toml`
- [x] Assert the request completes within the timeout even if the endpoint hangs (use a test server that never responds)
