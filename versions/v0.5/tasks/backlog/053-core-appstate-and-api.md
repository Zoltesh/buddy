# 053 — Extract AppState and Finalize buddy-core Public API

## Description

The final extraction step: `AppState` moves from `buddy-server/src/api/mod.rs` to `buddy-core`. `AppState` is the central application state container — it holds providers, skills, embedders, stores, and config. Despite living in the `api/` module, it has no Axum dependencies (it uses `arc_swap`, `tokio::sync`, and standard library types). Moving it to `buddy-core` means any consumer (web server, Telegram bot, CLI tool) can construct and use the same application state.

## Goal

`buddy-core` is a complete, standalone library crate with a clean public API. `buddy-server` is a thin Axum shell that constructs an `AppState` from `buddy-core` and maps HTTP requests to core operations. A new consumer (e.g., `buddy-telegram`) could depend on `buddy-core` and have full access to all application logic.

## Requirements

- Move `AppState` from `buddy-server/src/api/mod.rs` → `buddy-core/src/state.rs`
  - Include all associated types: `WorkingMemoryMap`, `SharedWarnings`, `PendingApprovals`, `ConversationApprovals`, `ApprovalPolicy`
  - Include `AppState::new()` constructor (create one if it doesn't exist) that takes `Config`, database path, and returns `Result<Arc<AppState>, String>`
  - The constructor should call the `reload::build_*` functions to initialize all components
- `buddy-server/src/api/mod.rs` retains only:
  - HTTP-specific types: `ChatRequest`, `ChatEvent`, `ApiError`, SSE event types
  - Handler function signatures
  - Imports `AppState` from `buddy_core::state`
- Update `buddy-core/src/lib.rs` with the final public module structure:
  ```rust
  pub mod types;
  pub mod config;
  pub mod store;
  pub mod embedding;
  pub mod memory;
  pub mod provider;
  pub mod skill;
  pub mod reload;
  pub mod warning;
  pub mod state;

  #[cfg(test)]
  pub mod testutil;
  ```
- `buddy-core` must NOT depend on `axum`, `tower`, `tower-http`, or any HTTP framework crate
- `buddy-server/Cargo.toml` should be the only place with `axum` as a dependency
- Verify the dependency tree: `cargo tree -p buddy-core` must not show `axum`
- `cargo build` must succeed
- `cargo test` must pass all existing tests
- `cargo test -p buddy-core` must pass all core tests
- `cargo build -p buddy-core` must succeed independently

## Acceptance Criteria

- [ ] `buddy-core/src/state.rs` exists with `AppState` and associated types
- [ ] `AppState::new()` constructor exists and initializes all components from config
- [ ] `buddy-server/src/api/mod.rs` imports `AppState` from `buddy_core::state`
- [ ] `buddy-server/src/api/mod.rs` contains only HTTP-specific types and handlers
- [ ] `buddy-core` does NOT depend on `axum`, `tower`, or `tower-http`
- [ ] `cargo tree -p buddy-core` shows no Axum dependency
- [ ] `buddy-core/src/lib.rs` exports all modules in a clean structure
- [ ] `cargo test` passes all existing tests
- [ ] `cargo test -p buddy-core` passes all core tests
- [ ] A hypothetical new binary crate could depend on `buddy-core` and construct an `AppState`

## Test Cases

- [ ] Run `cargo test -p buddy-core`; assert all tests pass
- [ ] Run `cargo test -p buddy-server`; assert all tests pass
- [ ] Run `cargo tree -p buddy-core`; assert the output does not contain `axum`
- [ ] Run `cargo build -p buddy-core`; assert it compiles independently
- [ ] Construct an `AppState` in a buddy-core test using `AppState::new()` with a test config and in-memory stores; assert it succeeds
- [ ] Verify `buddy-server/src/api/mod.rs` does not define `AppState` (grep for `pub struct AppState` — should only be in `buddy-core`)
