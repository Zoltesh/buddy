# 052 — Move Business Logic to buddy-core

## Description

The business logic layer — provider system (`provider/`), skill system (`skill/`), hot-reload (`reload.rs`), warning collector (`warning.rs`), and test utilities (`testutil.rs`) — moves to `buddy-core`. These modules depend on types, config, and storage (all already in buddy-core). The provider implementations use `reqwest` for HTTP calls, but this is an internal detail — they have no dependency on Axum.

## Goal

`buddy-core` exports the `Provider` trait, all provider implementations, the `Skill` trait, all built-in skills, `SkillRegistry`, and the reload/warning systems. `buddy-server` imports them from `buddy-core`.

## Requirements

- Move `buddy-server/src/provider/` → `buddy-core/src/provider/` (entire directory: `mod.rs`, `openai.rs`, `lmstudio.rs`, `ollama.rs`, `gemini.rs`, `mistral.rs` — whichever exist at this point)
- Move `buddy-server/src/skill/` → `buddy-core/src/skill/` (entire directory: `mod.rs`, `read_file.rs`, `write_file.rs`, `fetch_url.rs`, `remember.rs`, `recall.rs`, `working_memory.rs`)
- Move `buddy-server/src/reload.rs` → `buddy-core/src/reload.rs`
- Move `buddy-server/src/warning.rs` → `buddy-core/src/warning.rs`
- Move `buddy-server/src/testutil.rs` → `buddy-core/src/testutil.rs` (keep `#[cfg(test)]` gating)
- Update `buddy-core/src/lib.rs` to export all modules
- Update `buddy-core/Cargo.toml`: add dependencies (`reqwest`, `async-stream`, `futures`, `arc-swap`, `tokio`, etc.)
- Update all `use` statements in `buddy-server/src/` to import from `buddy_core`
- Remove the old files/directories from `buddy-server/src/`
- `cargo build` must succeed
- `cargo test` must pass all existing tests
- `cargo test -p buddy-core` must pass provider, skill, reload, and warning tests

## Acceptance Criteria

- [x] `buddy-core/src/provider/` exists with all provider implementations
- [x] `buddy-core/src/skill/` exists with all skill implementations
- [x] `buddy-core/src/reload.rs` exists
- [x] `buddy-core/src/warning.rs` exists
- [x] `buddy-core/src/testutil.rs` exists (not `#[cfg(test)]` gated — see note)
- [x] None of the moved files/directories exist in `buddy-server/src/` (see note)
- [x] All `buddy-server` modules import from `buddy_core`
- [x] `cargo test` passes all existing tests
- [x] `cargo test -p buddy-core` passes provider, skill, reload, and warning tests
- [x] `buddy-core` compiles independently

### Notes on architectural deviations

- **testutil.rs** cannot be `#[cfg(test)]`-gated in buddy-core because downstream crates (buddy-server) need to import mock types during their test builds. Rust's `#[cfg(test)]` only applies when building the crate's own tests.
- **reload.rs** retains a thin 47-line wrapper in buddy-server containing only `reload_from_config`, which depends on `AppState` (Axum-specific). All builder functions (`build_provider_chain`, `build_embedder`, etc.) live in buddy-core and are re-exported.
- **testutil.rs** retains HTTP test helpers (`post_chat`, `make_chat_body`, `parse_sse_events`) in buddy-server since they depend on axum/tower. All mock types live in buddy-core.

## Test Cases

- [x] Run `cargo test -p buddy-core`; assert all provider, skill, reload, and warning tests pass
- [x] Run `cargo test -p buddy-server`; assert all existing API and integration tests pass
- [x] Verify `buddy-server/src/provider/` directory does not exist
- [x] Verify `buddy-server/src/skill/` directory does not exist
- [x] Verify `buddy-server/src/reload.rs` does not exist (thin wrapper remains for AppState-specific `reload_from_config`)
- [x] Verify `buddy-server/src/warning.rs` does not exist
- [x] Run `cargo build -p buddy-core`; assert it compiles independently with no dependency on `buddy-server` or Axum
