# 017 — Organize api.rs Tests into Sub-modules

## Description

Split the large monolithic `#[cfg(test)] mod tests` block in `api.rs` (~800 lines) into logical nested sub-modules. This improves readability, makes it easier to run specific test groups with `cargo test`, and follows Rust conventions for organizing large test suites.

## Goal

A well-organized test module in `api.rs` where related tests are grouped into named sub-modules that can be run independently.

## Background

The current `api.rs` test module (lines 445–1258) contains ~30 test functions covering three distinct areas mixed into one flat block:

1. **Basic chat tests** — SSE streaming, malformed input, static file serving
2. **Tool-call loop tests** — single tool call, chained calls, max iterations, skill errors, unknown tools
3. **Conversation management tests** — CRUD operations, auto-creation, message persistence

These are separated by section comments but not by module boundaries. Running just the conversation tests, for example, requires filtering by name rather than module path.

## Requirements

- [x] Create nested sub-modules within `mod tests`:
  - `mod chat` — basic chat handler tests (SSE streaming, error responses, static files)
  - `mod tool_loop` — tool-call loop tests (single call, chaining, max iterations, errors)
  - `mod conversations` — conversation CRUD and persistence tests
- [x] Shared helper functions and imports remain at the `mod tests` level and are used by sub-modules via `use super::*`
- [x] All existing tests pass without modification to assertions or test logic
- [x] Each sub-module can be run independently: `cargo test api::tests::chat`, `cargo test api::tests::tool_loop`, `cargo test api::tests::conversations`

## Test Cases

- [x] `cargo test` passes with no test failures
- [x] `cargo test api::tests::chat` runs only chat-related tests
- [x] `cargo test api::tests::tool_loop` runs only tool-loop tests
- [x] `cargo test api::tests::conversations` runs only conversation tests
- [x] Total test count remains the same before and after

## Dependencies

- Should be done after task 016 (extract shared test utilities), since 016 will remove the mock definitions from `api.rs` tests, making the sub-module split cleaner.

## Notes

- This is a pure organizational refactor — no test logic changes.
- After task 016 extracts mocks and helpers to `testutil`, the remaining test code in `api.rs` will be mostly test functions, making the sub-module split straightforward.
