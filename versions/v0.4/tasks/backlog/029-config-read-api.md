# 029 — Config Read API

## Description

Add a `GET /api/config` endpoint that returns the current `buddy.toml` configuration as structured JSON. This is the foundation for the Settings UI — the frontend needs to read the current configuration before it can display or edit anything.

## Goal

The frontend can fetch the full server configuration (models, skills, chat, memory, server) as JSON to populate the Settings UI.

## Requirements

- Add a `GET /api/config` endpoint that returns the current `Config` as JSON
- Derive `Serialize` on all config structs (`Config`, `ServerConfig`, `ModelsConfig`, `ModelSlot`, `ProviderEntry`, `ChatConfig`, `SkillsConfig`, `ReadFileConfig`, `WriteFileConfig`, `FetchUrlConfig`, `StorageConfig`, `MemoryConfig`, `ApprovalPolicy`)
- API key env var names (`api_key_env`) are returned as-is (the env var name, not the resolved secret) — secrets must never be sent to the frontend
- The response shape must match the TOML structure so the frontend can reason about config sections
- Store the parsed `Config` in `AppState` (wrapped in `Arc<RwLock<Config>>`) so it can be read at runtime and later mutated by the write API
- Return `200 OK` with `Content-Type: application/json`

## Acceptance Criteria

- [ ] `GET /api/config` returns the current config as JSON
- [ ] All config structs implement `Serialize`
- [ ] API key env var names are returned but resolved secrets are not
- [ ] Config is stored in `AppState` behind `Arc<RwLock<Config>>`
- [ ] Response structure mirrors the TOML file layout

## Test Cases

- [ ] Call `GET /api/config` with a full config (all sections populated); assert 200 and JSON contains `models.chat.providers`, `skills`, `chat.system_prompt`, `server.host`, `server.port`
- [ ] Call `GET /api/config` with a minimal config (no embedding, no skills); assert 200 and JSON has `models.embedding` as null, `skills` sections as null
- [ ] Assert `api_key_env` field is present in provider entries but no resolved key value appears anywhere in the response
- [ ] Assert the returned JSON can be deserialized back into a `Config` struct (round-trip sanity check)
