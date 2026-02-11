# 050 — Move Domain Types and Config to buddy-core

## Description

The first code migration: `types.rs` and `config.rs` move from `buddy-server/src/` to `buddy-core/src/`. These are the foundational modules — every other module depends on them, and they depend on nothing within buddy. The `config.rs` module has one coupling point: `Config::load()` uses `clap` for CLI argument parsing. That function stays in `buddy-server` (it is the only consumer of CLI args). The pure parsing functions (`Config::parse()`, `Config::from_file()`) move to `buddy-core`.

## Goal

`buddy-core` exports the core domain types (`Message`, `Role`, `MessageContent`) and config types (`Config`, `ProviderEntry`, etc.). `buddy-server` imports them from `buddy-core`.

## Requirements

- Move `buddy-server/src/types.rs` → `buddy-core/src/types.rs`
- Move `buddy-server/src/config.rs` → `buddy-core/src/config.rs`
- In `buddy-core/src/config.rs`:
  - Remove the `Config::load()` function (it uses `clap`)
  - Remove the `Cli` struct and any `clap` imports
  - Keep `Config::parse()`, `Config::from_file()`, and all config structs
- In `buddy-server/src/main.rs`:
  - Add a local `Cli` struct with `clap` derive
  - Add a `load_config()` function that parses CLI args and calls `Config::from_file()`
  - Replace `Config::load()` calls with the local `load_config()`
- Update `buddy-core/src/lib.rs`:
  ```rust
  pub mod types;
  pub mod config;
  ```
- Update `buddy-core/Cargo.toml`: add dependencies that `types.rs` and `config.rs` need (e.g., `serde`, `toml`, `chrono`)
- Update all `use` statements in `buddy-server/src/` to import from `buddy_core::types` and `buddy_core::config` instead of `crate::types` and `crate::config`
- Remove the old files from `buddy-server/src/` after confirming all imports are updated
- `cargo build` must succeed
- `cargo test` must pass all existing tests
- `cargo test -p buddy-core` must pass config and types tests

## Acceptance Criteria

- [x] `buddy-core/src/types.rs` exists with `Message`, `Role`, `MessageContent` types
- [x] `buddy-core/src/config.rs` exists with `Config`, `ProviderEntry`, and all config structs
- [x] `buddy-core/src/config.rs` does NOT contain `clap` imports, `Cli` struct, or `Config::load()`
- [x] `buddy-server/src/main.rs` has a local `Cli` struct and `load_config()` function
- [x] `buddy-server/src/types.rs` and `buddy-server/src/config.rs` no longer exist
- [x] All `buddy-server` modules import types and config from `buddy_core`
- [x] `cargo test` passes all existing tests
- [x] `cargo test -p buddy-core` passes types and config tests

## Test Cases

- [x] Run `cargo test -p buddy-core`; assert all config parsing and types tests pass
- [x] Run `cargo test -p buddy-server`; assert all existing server tests pass
- [x] Verify `buddy-server/src/types.rs` does not exist (confirm it was removed, not just copied)
- [x] Verify `buddy-server/src/config.rs` does not exist
- [x] Verify `buddy-core/src/config.rs` does not contain the string `clap`
- [x] Run `cargo build -p buddy-core`; assert it compiles independently with no dependency on `buddy-server`
