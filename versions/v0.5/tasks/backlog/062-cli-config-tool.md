# 062 — CLI Config Tool

## Description

With `buddy-core` extracted, a CLI tool can share the same config management logic as the web UI. This task creates `buddy-cli` — a command-line tool for viewing and modifying `buddy.toml` without opening the browser. It reads and writes the config file using `buddy-core`'s parsing and validation, ensuring the CLI and web UI always produce valid config.

## Goal

Users can view and modify their buddy configuration from the terminal using intuitive commands.

## Requirements

- Create `buddy-cli/` as a new workspace member
- `buddy-cli/Cargo.toml`:
  - Depends on `buddy-core` (path dependency)
  - Depends on `clap` (with derive feature) for CLI parsing
  - Depends on `serde_json`, `toml`
- Add `"buddy-cli"` to workspace members in root `Cargo.toml`
- Commands (using clap subcommands):
  - `buddy-cli config show` — pretty-print the full config as TOML
  - `buddy-cli config show <section>` — print a specific section (e.g., `models`, `chat`, `skills`, `server`, `auth`, `interfaces`)
  - `buddy-cli config get <key>` — get a specific value using dot notation (e.g., `chat.system_prompt`, `server.host`, `models.chat.providers`)
    - Print the value as a string (for scalars) or as TOML (for tables/arrays)
  - `buddy-cli config set <key> <value>` — set a specific scalar value
    - Parse the value as the appropriate type (string, integer, boolean)
    - Validate the updated config using `Config::parse()` before writing
    - Write the updated config back to the file
    - Print "Updated {key} = {value}" on success
    - Print error and do NOT write if validation fails
  - `buddy-cli config validate` — validate the config file and report errors
    - Print "Configuration is valid." or a list of errors
  - `buddy-cli hash-token <token>` — print the SHA-256 hash for a plaintext token (for use in `[auth]` config)
    - Output: `sha256:{hex_digest}`
- Default config path: `buddy.toml` in current directory, overridable with `--config <path>` global flag
- **Error handling:**
  - File not found: "Config file not found: {path}. Use --config to specify the path."
  - Parse error: "Config error: {message}"
  - Validation error on set: "Validation failed: {message}. Config was not modified."
- `cargo build -p buddy-cli` must succeed

## Acceptance Criteria

- [ ] `buddy-cli/` exists as a workspace member
- [ ] `buddy-cli config show` prints the full config as formatted TOML
- [ ] `buddy-cli config show <section>` prints only the specified section
- [ ] `buddy-cli config get <key>` prints the value for dot-notation keys
- [ ] `buddy-cli config set <key> <value>` updates the config file after validation
- [ ] `buddy-cli config set` does NOT write if validation fails
- [ ] `buddy-cli config validate` reports config validity
- [ ] `buddy-cli hash-token <token>` prints the SHA-256 hash
- [ ] `--config <path>` overrides the default config file location
- [ ] `cargo build -p buddy-cli` succeeds
- [ ] All existing tests pass

## Test Cases

- [ ] Run `buddy-cli config show` with a valid config; assert the output is valid TOML that round-trips through `Config::parse()`
- [ ] Run `buddy-cli config show models` with a valid config; assert only the models section is printed
- [ ] Run `buddy-cli config show nonexistent`; assert an error message about unknown section
- [ ] Run `buddy-cli config get chat.system_prompt`; assert it prints the system prompt string
- [ ] Run `buddy-cli config get server.port`; assert it prints the port number
- [ ] Run `buddy-cli config set chat.system_prompt "New prompt"` with a valid config; assert the file is updated and the new value persists
- [ ] Run `buddy-cli config set server.port abc` (invalid type); assert validation error and file is not modified
- [ ] Run `buddy-cli config validate` with a valid config; assert "Configuration is valid."
- [ ] Run `buddy-cli config validate` with an invalid config (e.g., empty chat providers); assert error message
- [ ] Run `buddy-cli hash-token "my-secret"`; assert output matches `sha256:{expected_hex}`
- [ ] Run `buddy-cli config show --config /nonexistent/path.toml`; assert "Config file not found" error
