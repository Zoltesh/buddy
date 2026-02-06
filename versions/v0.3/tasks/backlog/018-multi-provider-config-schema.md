# 018 — Multi-Provider Configuration Schema

## Description

Refactor the configuration system from a single `[provider]` section to per-concern model slots: `[models.chat]` and `[models.embedding]`. Each slot holds an ordered list of providers — first is the default, the rest are fallbacks.

## Goal

The config schema supports multiple model slots with multiple providers each, so buddy can independently configure chat and embedding models with fallback chains.

## Background

The current config has a single `ProviderConfig` with one `provider_type`, `model`, `endpoint`, and `api_key`. V0.3 requires two independent model concerns (chat and embedding), each supporting an ordered list of providers for fallback. The `system_prompt` field moves to a top-level or `[chat]` section since it applies to the chat concern, not a specific provider.

## Requirements

- Replace the `[provider]` config section with `[models.chat]` and `[models.embedding]`
- Each model slot contains a `providers` array, where each entry specifies:
  - `type` — provider type string (e.g. `"openai"`, `"lmstudio"`, `"local"`)
  - `model` — model identifier string
  - `endpoint` — URL string (required for remote providers, omitted for local)
  - `api_key_env` — optional environment variable name for the API key (replaces the raw `api_key` field for better security)
- `system_prompt` moves to `[chat]` section (or top-level) — it is not provider-specific
- Update the `Config` struct and all sub-structs to reflect the new schema
- Update `buddy.example.toml` with the new format and comments
- Update provider construction in `main.rs` to build the primary chat provider from `models.chat.providers[0]`
  - Fallback logic itself is deferred to task 019 — this task only parses and stores the full list
- The `[models.embedding]` section is optional — if omitted, embedding-dependent features are unavailable (no error, just a degraded mode)
- The `[skills]` section is unchanged
- Fail fast with clear errors if `[models.chat]` is missing or has an empty providers list
- Support reading API keys from environment variables via `api_key_env`

## Acceptance Criteria

- [ ] `buddy.toml` with the new `[models.chat]` / `[models.embedding]` schema parses into typed structs
- [ ] The primary chat provider is constructed from `models.chat.providers[0]`
- [ ] `models.embedding` is optional — omitting it does not cause a startup error
- [ ] Missing `[models.chat]` or empty `providers` list produces a clear error message
- [ ] `api_key_env` reads the key from the named environment variable
- [ ] `system_prompt` is parsed from `[chat]` section, not from provider config
- [ ] `buddy.example.toml` reflects the new schema with documentation comments
- [ ] All existing tests pass (updated for new config format)

## Test Cases

- Parse a config with `[models.chat]` containing two providers; assert both are stored in order
- Parse a config with `[models.embedding]` containing a local provider; assert it parses correctly
- Parse a config with no `[models.embedding]` section; assert config loads without error and embedding is `None`
- Parse a config with no `[models.chat]` section; assert error message mentions `models.chat`
- Parse a config with an empty `providers` list under `[models.chat]`; assert error message mentions empty providers
- Set `OPENAI_API_KEY=test123` in env; parse config with `api_key_env = "OPENAI_API_KEY"`; assert resolved key is `"test123"`
- Parse a config with `api_key_env` pointing to an unset variable; assert a clear error
- Assert `system_prompt` default value works when `[chat]` section is omitted
