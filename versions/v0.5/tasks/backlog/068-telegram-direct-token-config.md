# 068 — Telegram Direct Token Configuration

## Description

Currently `TelegramConfig` only stores `bot_token_env` — the name of an environment variable containing the bot token. This forces the user to set the env var externally before running `buddy-telegram`. For a smoother setup experience, allow the user to enter the bot token directly in the UI. The direct token is stored in `buddy.toml` and takes priority over the env var, mirroring the `api_key` / `api_key_env` pattern already used by `ProviderEntry`.

## Goal

Users can enter their Telegram bot token directly in the Interfaces UI. The token is persisted in config and used by `buddy-telegram` without requiring an external environment variable.

## Requirements

- **Config struct changes** (`buddy-core/src/config.rs`):
  - Add `bot_token: Option<String>` field to `TelegramConfig` (default: `None`)
  - Add `TelegramConfig::resolve_bot_token() -> Result<String, String>` method:
    1. If `bot_token` is `Some` and non-empty, return it
    2. Otherwise, read from the env var named by `bot_token_env`
    3. If both are absent/empty, return an error
  - Serialize/deserialize: `bot_token` is optional in TOML, omitted when `None`
- **Telegram binary changes** (`buddy-telegram/src/main.rs`):
  - Replace the manual `std::env::var(&telegram.bot_token_env)` call with `telegram.resolve_bot_token()`
  - Error message should indicate which resolution path failed
- **Frontend changes** (`frontend/src/lib/Interfaces.svelte`):
  - Telegram edit form adds a "Bot Token" field (password-type input, masked by default)
  - Keep the "Bot Token Env Var" field below it
  - Add a help text: "Enter the token directly, or specify an environment variable name. Direct token takes priority."
  - When saving: if the bot token field is non-empty, include `bot_token` in the payload; if empty, omit it (send `null`)
  - When displaying configured state: show "Token: ••••••" (masked) if a direct token is set, or "Token env: TELEGRAM_BOT_TOKEN" if using env var
- **Security note in UI**: Show a subtle hint below the direct token field: "Stored in buddy.toml on disk."

## Acceptance Criteria

- [ ] `TelegramConfig` has a `bot_token` field (optional)
- [ ] `resolve_bot_token()` returns the direct token when set
- [ ] `resolve_bot_token()` falls back to the env var when no direct token is set
- [ ] `resolve_bot_token()` returns an error when neither is available
- [ ] `buddy-telegram` uses `resolve_bot_token()` to obtain the token
- [ ] Frontend edit form shows both the direct token field and the env var name field
- [ ] Direct token field is a password input (masked)
- [ ] Saving with a direct token persists it in the config
- [ ] Configured card shows masked token or env var name appropriately
- [ ] Empty direct token is omitted from the saved config (not stored as empty string)
- [ ] All existing tests pass

## Test Cases

- [ ] Parse config with `bot_token = "123:ABC"` set; call `resolve_bot_token()`; assert returns `"123:ABC"`
- [ ] Parse config with no `bot_token` and `bot_token_env = "MY_TG_TOKEN"`; set env var `MY_TG_TOKEN=secret`; call `resolve_bot_token()`; assert returns `"secret"`
- [ ] Parse config with `bot_token = "direct"` and `bot_token_env = "MY_TG_TOKEN"` (env var also set); call `resolve_bot_token()`; assert returns `"direct"` (direct takes priority)
- [ ] Parse config with no `bot_token` and env var not set; call `resolve_bot_token()`; assert returns error mentioning the env var name
- [ ] Parse config with `bot_token = ""` (empty string); assert it behaves the same as `None` (falls through to env var)
- [ ] Serialize config with `bot_token = None`; assert the TOML output does not contain a `bot_token` key
- [ ] Round-trip: parse config with `bot_token = "tok123"`, serialize to TOML, re-parse; assert `bot_token` is preserved
- [ ] `PUT /api/config/interfaces` with `bot_token: "tok"` in payload; assert config file contains `bot_token = "tok"` in `[interfaces.telegram]`
- [ ] `PUT /api/config/interfaces` with `bot_token: null` in payload; assert config file does not contain `bot_token` in `[interfaces.telegram]`
