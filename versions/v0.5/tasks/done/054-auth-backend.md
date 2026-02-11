# 054 — Authentication Backend

## Description

buddy is a personal assistant — when exposed beyond localhost, it needs a simple auth gate to prevent unauthorized access. This task adds token-based authentication as an Axum middleware layer. Auth is deliberately minimal: a single bearer token, configured in `buddy.toml`, checked on every `/api/*` request. Auth is only enforced when the server binds to a non-localhost address; localhost-only instances skip auth entirely. This keeps the development and single-machine experience frictionless.

## Goal

When buddy is configured to bind to a non-localhost address, all API requests require a valid bearer token. Localhost instances work without authentication.

## Requirements

- Add an `[auth]` section to the config schema in `buddy-core/src/config.rs`:
  ```toml
  [auth]
  token_hash = "sha256:..."   # optional — if absent, auth is disabled
  ```
  - `token_hash`: a SHA-256 hash of the plaintext token, prefixed with `sha256:`
  - If `[auth]` is missing or `token_hash` is absent, auth is disabled
  - Add `AuthConfig` struct: `pub struct AuthConfig { pub token_hash: Option<String> }`
  - Add `auth: AuthConfig` field to the root `Config` struct (default: empty/disabled)
- Add auth middleware in `buddy-server/src/api/`:
  - Apply to all routes under `/api/*`
  - Do NOT apply to static file serving (frontend assets)
  - **Skip auth entirely** when `config.server.host` is `"127.0.0.1"` or `"localhost"` or `"::1"`
  - When auth is enabled:
    - Check for `Authorization: Bearer <token>` header
    - Hash the provided token with SHA-256 and compare to `config.auth.token_hash`
    - If valid: pass through to handler
    - If missing or invalid: return `401 Unauthorized` with `{ "error": "Unauthorized" }`
  - When auth is disabled (no `token_hash` configured): pass through to handler
- Add `POST /api/auth/verify` endpoint:
  - Exempt from auth middleware (must be accessible without a token to enable login)
  - Request: `{ "token": "plaintext-token" }`
  - Hashes the token and compares to `config.auth.token_hash`
  - Success: `{ "valid": true }`
  - Failure: `{ "valid": false }`
  - If auth is disabled: always returns `{ "valid": true }`
- Add `GET /api/auth/status` endpoint:
  - Exempt from auth middleware
  - Returns: `{ "required": true }` or `{ "required": false }`
  - `true` when auth is configured AND server is non-localhost
- Use the `sha2` crate for hashing — add it to dependencies
- Add a utility function to generate a token hash (for documentation or CLI use): `fn hash_token(plaintext: &str) -> String`
- Do not store plaintext tokens anywhere
- Do not implement token expiry, refresh, or rotation in this task

## Acceptance Criteria

- [x] `AuthConfig` struct exists in config with optional `token_hash` field
- [x] Config with `[auth]` section and `token_hash` parses correctly
- [x] Config without `[auth]` section parses correctly (auth disabled)
- [x] Auth middleware checks Bearer token on `/api/*` routes when auth is required
- [x] Auth middleware skips authentication when server binds to localhost
- [x] Auth middleware skips authentication when `token_hash` is not configured
- [x] Invalid or missing token returns 401 with `{ "error": "Unauthorized" }`
- [x] Valid token passes through to the handler
- [x] `POST /api/auth/verify` validates a plaintext token against the hash
- [x] `GET /api/auth/status` returns whether auth is required
- [x] Static file serving (frontend) is not affected by auth middleware
- [x] All existing tests pass (they use localhost, so auth is skipped)

## Test Cases

- [x] Parse a config with `[auth]` and `token_hash = "sha256:abc123..."`; assert `AuthConfig` has `Some("sha256:abc123...")`
- [x] Parse a config without `[auth]` section; assert `AuthConfig` has `token_hash: None`
- [x] Send a request to `/api/conversations` without a token, with auth enabled on a non-localhost bind; assert 401
- [x] Send a request to `/api/conversations` with a valid Bearer token, auth enabled; assert 200
- [x] Send a request to `/api/conversations` with an invalid Bearer token, auth enabled; assert 401
- [x] Send a request to `/api/conversations` without a token, server bound to `127.0.0.1`; assert 200 (auth skipped)
- [x] Send a request to `/api/conversations` without a token, auth not configured (`token_hash` absent); assert 200 (auth disabled)
- [x] POST to `/api/auth/verify` with `{ "token": "correct-token" }` where hash matches; assert `{ "valid": true }`
- [x] POST to `/api/auth/verify` with `{ "token": "wrong-token" }`; assert `{ "valid": false }`
- [x] GET `/api/auth/status` with auth enabled and non-localhost; assert `{ "required": true }`
- [x] GET `/api/auth/status` with auth enabled but localhost bind; assert `{ "required": false }`
- [x] GET `/api/auth/status` with auth not configured; assert `{ "required": false }`
