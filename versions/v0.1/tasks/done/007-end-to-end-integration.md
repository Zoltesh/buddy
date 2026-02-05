# 007 — End-to-End Integration

## Description

Wire all components together and verify that the full path — user input through the UI, to the backend API, to the LLM provider, and back as a streamed response — works correctly as a cohesive system.

## Goal

A single `cargo run` (after building the frontend) starts buddy and a user can hold a multi-turn conversation in their browser.

## Requirements

- The server initializes: parse config, construct provider, bind HTTP server, serve frontend
- Startup logs clearly indicate: what address the server is listening on, which provider/model is configured
- Graceful error on startup if the config is missing or invalid (no panic, no stack trace)
- `Ctrl+C` shuts the server down cleanly
- The system prompt is configurable in `buddy.toml` (`provider.system_prompt`, optional, has a sensible default)
- The frontend and backend agree on the streaming protocol — no version mismatch or silent failures

## Acceptance Criteria

- [x] `cargo run` starts the server; opening `http://127.0.0.1:3000` shows the chat UI
- [x] Sending a message produces a streamed response from the configured LLM
- [x] A multi-turn conversation (3+ exchanges) works without errors or dropped context
- [x] Starting with a missing `buddy.toml` prints a clear error and exits with a non-zero code
- [x] Starting with an invalid API key results in a user-visible error in the chat UI (not a silent failure)
- [x] `Ctrl+C` stops the server without error output
- [x] Startup log includes the bind address and model name

## Test Cases

- Start with valid config; send "What is 2+2?"; verify a coherent response streams back
- Start with valid config; send 3 messages in sequence; verify the assistant references earlier messages (context is maintained within the session)
- Start with no `buddy.toml`; assert process exits with code 1 and stderr contains "config"
- Start with a valid config but an invalid API key; send a message; assert the UI displays an error, not a hang
- Start the server; press `Ctrl+C`; assert the process exits with code 0
