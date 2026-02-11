# 049 — Create Cargo Workspace

## Description

The first step in extracting `buddy-core` is converting the project to a Cargo workspace. This is a purely structural change: a root `Cargo.toml` becomes the workspace manifest, `buddy-server` moves into its own workspace member, and a new `buddy-core` library crate is created (initially empty). No source code moves in this task — the goal is a green build with the new workspace structure.

## Goal

The project is a Cargo workspace with `buddy-server` and `buddy-core` as members, everything compiles, and all existing tests pass.

## Requirements

- Create a root `Cargo.toml` workspace manifest:
  ```toml
  [workspace]
  members = ["buddy-server", "buddy-core"]
  resolver = "2"
  ```
- Create `buddy-core/Cargo.toml`:
  ```toml
  [package]
  name = "buddy-core"
  version = "0.5.0"
  edition = "2024"

  [dependencies]
  ```
- Create `buddy-core/src/lib.rs` with a placeholder:
  ```rust
  // buddy-core: shared library for the buddy application.
  ```
- Update `buddy-server/Cargo.toml`:
  - Add `buddy-core = { path = "../buddy-core" }` to `[dependencies]`
  - Ensure the existing `[package]` section is intact (name, version, edition, etc.)
- The root `Cargo.toml` must NOT have a `[package]` section — it is workspace-only
- Move the existing root `Cargo.toml` content (which is `buddy-server`'s manifest) into `buddy-server/Cargo.toml` if not already there
- Update any paths in `buddy-server/Cargo.toml` that reference relative directories (e.g., `build.rs`, `src/`)
- Update the `Makefile` if it references `cargo` commands — they should work from the workspace root
- `cargo build` from the workspace root must succeed
- `cargo test` from the workspace root must pass all existing tests
- `cargo build -p buddy-core` must succeed (empty lib)
- `cargo build -p buddy-server` must succeed
- Do not move any source files in this task

## Acceptance Criteria

- [x] Root `Cargo.toml` is a workspace manifest with members `buddy-server` and `buddy-core`
- [x] `buddy-core/Cargo.toml` exists with correct package metadata
- [x] `buddy-core/src/lib.rs` exists
- [x] `buddy-server/Cargo.toml` depends on `buddy-core`
- [x] `cargo build` succeeds from the workspace root
- [x] `cargo test` passes all existing tests from the workspace root
- [x] `cargo build -p buddy-core` succeeds
- [x] `cargo build -p buddy-server` succeeds
- [x] The `Makefile` works with the workspace structure
- [x] No source code has been moved or modified (only Cargo.toml files and Makefile)

## Test Cases

- [x] Run `cargo build` from the workspace root; assert exit code 0
- [x] Run `cargo test` from the workspace root; assert all existing tests pass with exit code 0
- [x] Run `cargo build -p buddy-core` from the workspace root; assert exit code 0
- [x] Run `cargo build -p buddy-server` from the workspace root; assert exit code 0
- [x] Run `make build` from the project root; assert it succeeds
- [x] Verify `buddy-core/src/lib.rs` exists and `buddy-core/Cargo.toml` lists `buddy-core` as the package name
