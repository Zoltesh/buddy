# 051 — Move Storage Layer to buddy-core

## Description

The storage layer — conversation store (`store.rs`), embedding system (`embedding/`), and vector memory (`memory/`) — moves to `buddy-core`. These modules depend only on `types.rs` (already in buddy-core) and external crates (`rusqlite`, `fastembed`). They have no Axum or HTTP dependencies.

## Goal

`buddy-core` exports the `Store`, `Embedder` trait, `LocalEmbedder`, `VectorStore` trait, and `SqliteVectorStore`. `buddy-server` imports them from `buddy-core`.

## Requirements

- Move `buddy-server/src/store.rs` → `buddy-core/src/store.rs`
- Move `buddy-server/src/embedding/` → `buddy-core/src/embedding/` (entire directory: `mod.rs`, `local.rs`)
- Move `buddy-server/src/memory/` → `buddy-core/src/memory/` (entire directory: `mod.rs`, `sqlite.rs`)
- Update `buddy-core/src/lib.rs` to export the new modules:
  ```rust
  pub mod types;
  pub mod config;
  pub mod store;
  pub mod embedding;
  pub mod memory;
  ```
- Update `buddy-core/Cargo.toml`: add dependencies (`rusqlite`, `fastembed`, `uuid`, etc.)
- Update all `use` statements in `buddy-server/src/` to import from `buddy_core::store`, `buddy_core::embedding`, `buddy_core::memory`
- Remove the old files/directories from `buddy-server/src/`
- `cargo build` must succeed
- `cargo test` must pass all existing tests
- `cargo test -p buddy-core` must pass store, embedding, and memory tests

## Acceptance Criteria

- [x] `buddy-core/src/store.rs` exists with `Store` and all conversation types
- [x] `buddy-core/src/embedding/` exists with `Embedder` trait and `LocalEmbedder`
- [x] `buddy-core/src/memory/` exists with `VectorStore` trait and `SqliteVectorStore`
- [x] `buddy-server/src/store.rs`, `buddy-server/src/embedding/`, and `buddy-server/src/memory/` no longer exist
- [x] All `buddy-server` modules import storage types from `buddy_core`
- [x] `cargo test` passes all existing tests
- [x] `cargo test -p buddy-core` passes store, embedding, and memory tests
- [x] `buddy-core` compiles independently (`cargo build -p buddy-core`)

## Test Cases

- [x] Run `cargo test -p buddy-core`; assert store, embedding, and memory tests pass
- [x] Run `cargo test -p buddy-server`; assert all existing server tests pass
- [x] Verify `buddy-server/src/store.rs` does not exist
- [x] Verify `buddy-server/src/embedding/` directory does not exist
- [x] Verify `buddy-server/src/memory/` directory does not exist
- [x] Run `cargo build -p buddy-core`; assert it compiles independently
