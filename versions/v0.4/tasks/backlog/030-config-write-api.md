# 030 — Config Write API

## Description

Add endpoints to update configuration sections and persist changes to `buddy.toml`. Each section (models, skills, chat, server, memory) has its own `PUT` endpoint that validates the incoming data before writing. The TOML file remains the source of truth.

## Goal

The Settings UI can save configuration changes through backend API endpoints. Changes are validated, written atomically to `buddy.toml`, and the in-memory config is updated.

## Requirements

- Add the following endpoints:
  - `PUT /api/config/models` — update `[models]` section (chat and embedding slots with their provider arrays)
  - `PUT /api/config/skills` — update `[skills]` section (read_file, write_file, fetch_url configs)
  - `PUT /api/config/chat` — update `[chat]` section (system_prompt)
  - `PUT /api/config/server` — update `[server]` section (host, port)
  - `PUT /api/config/memory` — update `[memory]` section (auto_retrieve, limits, threshold)
- Derive `Deserialize` on all config structs if not already present
- Validate before writing:
  - `models.chat.providers` must not be empty
  - `provider_type` must be a recognized value (`"openai"`, `"lmstudio"`, `"local"`)
  - `model` must not be empty
  - `server.port` must be 1–65535
  - Skill `allowed_directories` paths must be valid (exist and be directories)
  - Skill `allowed_domains` must not be empty strings
  - Return `400 Bad Request` with a structured error listing all validation failures
- Validation error response format:
  ```json
  { "errors": [{ "field": "models.chat.providers[0].model", "message": "must not be empty" }] }
  ```
- Write atomically: write to a temp file in the same directory, then rename to `buddy.toml`
- After successful write, update the in-memory `Config` in `AppState`
- Implement a `Config::to_toml_string(&self) -> String` method that serializes the full config to a valid TOML string
- Derive `Serialize` on `ApprovalPolicy` and any remaining structs needed for TOML serialization
- Store the config file path in `AppState` so the write endpoint knows where to persist

## Acceptance Criteria

- [ ] `PUT /api/config/models` updates the models section and persists to `buddy.toml`
- [ ] `PUT /api/config/skills` updates the skills section and persists
- [ ] `PUT /api/config/chat` updates the chat section and persists
- [ ] `PUT /api/config/server` updates the server section and persists
- [ ] `PUT /api/config/memory` updates the memory section and persists
- [ ] Validation rejects empty `models.chat.providers` with a structured error
- [ ] Validation rejects unknown `provider_type` values
- [ ] Validation rejects empty `model` strings
- [ ] Write is atomic (temp file + rename)
- [ ] In-memory config is updated after successful write
- [ ] Existing config sections not included in the request body are preserved

## Test Cases

- [ ] PUT valid models config; read `buddy.toml` from disk; assert it contains the updated providers
- [ ] PUT models with empty `providers` array; assert 400 with validation error on `models.chat.providers`
- [ ] PUT models with unknown `provider_type`; assert 400 with validation error
- [ ] PUT models with empty `model` string; assert 400 with validation error
- [ ] PUT valid skills config; assert skills section is updated and other sections are unchanged
- [ ] PUT chat config with new `system_prompt`; assert it persists
- [ ] PUT server config with valid port; assert it persists
- [ ] PUT memory config; assert it persists
- [ ] Call `GET /api/config` after a successful PUT; assert the in-memory config reflects the change
- [ ] Assert validation errors return all failures (not just the first one)
