# 056 — Telegram Bot Crate Setup

## Description

This task creates the `buddy-telegram` binary crate — a Telegram Bot API client that consumes `buddy-core`. It establishes the crate structure, config integration, message adapter (translating between Telegram messages and buddy-core's `Message` type), and a basic polling loop that connects to Telegram and receives updates. No conversation processing yet — that comes in the next task. This task focuses on infrastructure: crate setup, config, adapter, and connectivity.

## Goal

A `buddy-telegram` binary connects to the Telegram Bot API, receives text messages, and logs them. The message adapter can translate between Telegram and buddy-core message formats.

## Requirements

- Create `buddy-telegram/` as a new workspace member
- `buddy-telegram/Cargo.toml`:
  - Depends on `buddy-core` (path dependency)
  - Depends on `teloxide` (Telegram bot framework for Rust) with features for long polling
  - Depends on `tokio`, `serde`, `serde_json`, `log`, `env_logger`
- Add `"buddy-telegram"` to the workspace members in root `Cargo.toml`
- Add `[interfaces.telegram]` config section in `buddy-core/src/config.rs`:
  ```toml
  [interfaces.telegram]
  enabled = false
  bot_token_env = "TELEGRAM_BOT_TOKEN"
  ```
  - `enabled`: boolean, default `false`
  - `bot_token_env`: name of the environment variable containing the bot token
  - Add `InterfacesConfig` and `TelegramConfig` structs to config
  - Add `interfaces: InterfacesConfig` to root `Config` (default: all disabled)
- Create `buddy-telegram/src/main.rs`:
  - Load config from `buddy.toml` (reuse `Config::from_file()`)
  - Read bot token from env var specified in config
  - Connect to Telegram Bot API via teloxide
  - Start long-polling update loop
  - On receiving a text message: log the message (sender, chat_id, text)
  - Graceful shutdown on SIGINT/SIGTERM
- Create `buddy-telegram/src/adapter.rs`:
  - `fn telegram_to_buddy(message: &teloxide::types::Message) -> Option<buddy_core::types::Message>` — extracts text from a Telegram message and converts to buddy-core `Message` with `Role::User`
  - `fn buddy_to_telegram(message: &buddy_core::types::Message) -> String` — converts a buddy-core `Message` to a plain text string suitable for Telegram (strips tool calls, formats text)
  - Handle the case where Telegram messages contain no text (photos, stickers, etc.) — return `None`
- `cargo build -p buddy-telegram` must succeed
- The binary must start and immediately shut down gracefully when no bot token is configured

## Acceptance Criteria

- [ ] `buddy-telegram/` exists as a workspace member
- [ ] `buddy-telegram/Cargo.toml` depends on `buddy-core` and `teloxide`
- [ ] `[interfaces.telegram]` config section parses correctly with `enabled` and `bot_token_env` fields
- [ ] Config without `[interfaces.telegram]` parses correctly (defaults to disabled)
- [ ] `buddy-telegram/src/main.rs` loads config and connects to Telegram when enabled
- [ ] `buddy-telegram/src/adapter.rs` converts Telegram messages to buddy-core `Message` type
- [ ] `buddy-telegram/src/adapter.rs` converts buddy-core `Message` to Telegram-compatible text
- [ ] `cargo build -p buddy-telegram` succeeds
- [ ] The binary exits gracefully when the bot token env var is not set
- [ ] All existing tests pass (`cargo test` from workspace root)

## Test Cases

- [ ] Parse a config with `[interfaces.telegram]` enabled and `bot_token_env = "TELEGRAM_BOT_TOKEN"`; assert `TelegramConfig` has correct values
- [ ] Parse a config without `[interfaces.telegram]`; assert `TelegramConfig` defaults to `enabled: false`
- [ ] Call `telegram_to_buddy()` with a mock Telegram text message "Hello"; assert it returns a buddy-core `Message` with `Role::User` and text "Hello"
- [ ] Call `telegram_to_buddy()` with a mock Telegram message that has no text (e.g., a photo); assert it returns `None`
- [ ] Call `buddy_to_telegram()` with a buddy-core `Message` containing `MessageContent::Text`; assert it returns the text string
- [ ] Call `buddy_to_telegram()` with a buddy-core `Message` containing `MessageContent::ToolCall`; assert it returns a formatted string (e.g., "Using tool: {name}...")
- [ ] Run `cargo build -p buddy-telegram`; assert exit code 0
