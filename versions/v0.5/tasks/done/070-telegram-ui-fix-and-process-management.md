# 070 — Telegram UI Fix and Process Management

## Description

The Interfaces page is broken and can't save Telegram configuration. `structuredClone()` fails on Svelte 5 reactive proxy objects, preventing both toggle and save actions. Additionally, the "Bot Token Env Var" field is an implementation detail that shouldn't be user-facing, and the Telegram bot binary (`buddy-telegram`) must currently be started manually as a separate process — there's no way to go from "configure token in UI" to "bot is listening" without using the terminal.

This task fixes the UI bugs, simplifies the Telegram form, and adds process management so buddy-server spawns/stops the Telegram bot when enabled/disabled via the UI.

## Goal

A user can enter their Telegram bot token in the UI, enable the interface, and immediately chat with their bot via Telegram — without running any terminal commands.

## Requirements

### 1. Fix `structuredClone` crash

- **Root cause:** `structuredClone()` cannot clone Svelte 5 `$state` proxy objects. Both `toggleEnabled()` (line 110) and `saveEditing()` (line 172) in `Interfaces.svelte` call `structuredClone(config.interfaces)` which throws.
- **Fix:** Replace all `structuredClone(config.interfaces)` calls with `$state.snapshot(config.interfaces)` (Svelte 5's built-in method for getting a plain-object copy of reactive state) or `JSON.parse(JSON.stringify(config.interfaces))`.
- `$state.snapshot()` is the idiomatic Svelte 5 approach and should be preferred.

### 2. Remove Bot Token Env Var from Telegram UI

- Remove the "Bot Token Env Var" input field and its label from the Telegram editing form.
- Remove the explanatory text about "Enter the token directly, or specify an environment variable name."
- In `startEditing('telegram')` and `startConfiguring('telegram')`, stop populating `editForm.bot_token_env`.
- In `saveEditing()`, preserve the existing `bot_token_env` value from config when saving (don't overwrite it from the form) — this keeps the env var fallback working for users who configure via `buddy.toml` directly, without exposing it in the UI.
- In the configured (non-editing) view, if there's a direct `bot_token`, show "Token: ••••••". If there's no direct token (env var fallback), show "Token: via environment variable" instead of exposing the raw env var name.

### 3. Auto-enable on save when configuring for the first time

- When the user clicks "Configure" and enters a bot token, the Enabled checkbox should default to checked (`true`). The current default is `false`, which means the user has to remember to also check the box — an easy mistake that leads to "I saved my token but nothing happened."
- Only applies to `startConfiguring()` (new setup), not `startEditing()` (editing existing config).

### 4. buddy-server spawns buddy-telegram as a child process

- **On startup:** After buddy-server initializes, check if `interfaces.telegram.enabled` is `true` and the bot token resolves successfully. If so, spawn `buddy-telegram` as a child process (using `tokio::process::Command`).
  - The child should inherit the same `--config` path.
  - Capture its stdout/stderr and log them with a `[telegram]` prefix.
  - Store the `Child` handle in `AppState` or a new field on the server state so it can be managed later.
- **On config change (hot-reload):** When `PUT /api/config/interfaces` updates the config and triggers `on_config_change`:
  - If Telegram was disabled and is now enabled (and token resolves): spawn the child process.
  - If Telegram was enabled and is now disabled: kill the child process (send SIGTERM, then SIGKILL after a short grace period).
  - If Telegram was enabled and is still enabled but the token changed: kill the old child and spawn a new one.
  - If Telegram is enabled but the token can't be resolved: don't spawn, log a warning.
- **On child exit:** If the child process exits unexpectedly, log the exit code. Do NOT auto-restart (that can be a separate future task to avoid retry loops during development).
- **On server shutdown:** Kill the child process during graceful shutdown (in `shutdown_signal()`).
- **Binary location:** Use `std::env::current_exe()` to find the server binary's directory and look for `buddy-telegram` adjacent to it. Alternatively, check `PATH`. If the binary is not found, log a warning and skip spawning.

### 5. Run health check after successful save

- After `saveEditing()` succeeds for Telegram, automatically run `runHealthCheck('telegram')` after a short delay (1-2 seconds, to give the child process time to start).
- This gives immediate feedback that the token is valid and the bot is connected.

### 6. Add Makefile targets

- `make telegram`: Build and run buddy-telegram in debug mode (`cargo run --bin buddy-telegram`)
- `make build-telegram`: Build buddy-telegram (`cargo build --release --bin buddy-telegram`)
- Update `build-server` to also build buddy-telegram: `cargo build --release --bin buddy-server --bin buddy-telegram`

## Acceptance Criteria

- [x] `structuredClone` error no longer occurs — toggle and save both work
- [x] "Bot Token Env Var" field is not visible in the Telegram UI
- [x] Saving a bot token and enabling Telegram persists to `buddy.toml`
- [x] First-time configure defaults the Enabled checkbox to checked
- [x] When Telegram is enabled with a valid token, buddy-server spawns `buddy-telegram` as a child process
- [x] When Telegram is disabled, the child process is killed
- [x] When the server shuts down, the child process is cleaned up
- [x] If `buddy-telegram` binary is not found, a warning is logged (not a crash)
- [x] After saving Telegram config, the health check runs automatically
- [ ] Sending a message to the bot via Telegram app results in a response
- [x] `make build` produces both `buddy-server` and `buddy-telegram` binaries
- [x] All existing tests pass

## Test Cases

- [x] Open Interfaces page, click Configure on Telegram, enter a token, click Save; assert no `structuredClone` error and config persists
- [x] Open Interfaces page with configured Telegram, toggle the Enabled switch; assert no error and enabled state changes
- [x] Open Interfaces page; assert "Bot Token Env Var" input is not present in the DOM
- [x] In `startConfiguring('telegram')`, assert `editForm.enabled` is `true`
- [x] Configure Telegram with a valid bot token and enable; assert `buddy-telegram` child process is running (check process list or server logs)
- [x] Disable Telegram via toggle; assert `buddy-telegram` child process is no longer running
- [x] Start buddy-server without `buddy-telegram` binary in PATH or adjacent directory; assert server starts with a warning, not a crash
- [ ] Send a message to the configured Telegram bot; assert a response is received (manual integration test)
