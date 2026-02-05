# 015 — End-to-End Integration

## Description

Wire all V0.2 components together and verify the full system works as a cohesive whole: skills execute correctly, tool calls render in the UI, conversations persist across restarts, the second provider works, and sandbox constraints are enforced.

## Goal

A user can start buddy, hold conversations that use tools, switch between conversations, restart the server, and resume where they left off — all with full transparency into what buddy is doing.

## Requirements

- All V0.2 components are integrated in `main.rs`:
  - Config loads skill sandbox settings
  - `SkillRegistry` is populated with configured skills
  - `Store` is initialized and passed to `AppState`
  - Provider is selected based on `provider.type`
  - The tool-call loop uses the registry and store
- Startup log includes: bind address, model name, provider type, number of registered skills, database path
- Skills configuration is validated at startup (e.g., allowed directories exist)
- Graceful degradation: if SQLite fails to initialize, log the error and exit cleanly
- Update `buddy.example.toml` to be a complete reference for all V0.2 configuration
- Update the `Makefile` if any build steps changed

## Acceptance Criteria

- [ ] `cargo run` starts the server with all V0.2 features active
- [ ] Sending a message that triggers a tool call (e.g., "Read /sandbox/test.txt") works end-to-end: skill executes, result fed to LLM, response streams to UI
- [ ] Tool-call activity is visible in the UI chat
- [ ] Conversations persist: send messages, restart the server, conversations are still listed and loadable
- [ ] Creating a new conversation and switching between conversations works
- [ ] The second provider works when configured (swap `provider.type` in config)
- [ ] Sandbox is enforced: asking buddy to read a file outside allowed directories results in a clear error in the LLM response (not a crash)
- [ ] `Ctrl+C` shuts the server down cleanly
- [ ] `cargo test` passes all unit and integration tests
- [ ] `buddy.example.toml` documents all configuration options

## Test Cases

- Start with valid config including skills; send "What's in /sandbox/hello.txt?"; verify buddy reads the file and responds with its contents
- Send "Write 'test' to /sandbox/output.txt"; verify the file is created with the correct content
- Send "Fetch https://allowlisted-domain.com"; verify buddy fetches and summarizes the page
- Ask buddy to read a file outside the sandbox; verify the LLM receives a forbidden error and communicates it to the user (no crash)
- Send 3 messages in a conversation; restart the server; navigate to the conversation; verify all messages are present
- Switch between two conversations; verify each loads its own history
- Configure the second provider; send a message; verify a response streams back
- Start with no `buddy.toml`; assert the process exits with a clear error
- Start with skills configured but an invalid allowed directory; assert a clear startup warning or error
- Run `cargo test`; assert all tests pass
