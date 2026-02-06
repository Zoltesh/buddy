# 016 — Extract Shared Test Utilities Module

## Description

Consolidate duplicated mock implementations and test helpers into a shared `testutil` module behind `#[cfg(test)]`. This eliminates duplicate code, reduces maintenance burden when traits evolve, and follows the Rust convention of centralizing test support code.

## Goal

A single source of truth for mock providers, mock skills, and reusable test helpers that all test modules can import.

## Background

Currently, mock implementations are defined inline within individual `#[cfg(test)] mod tests` blocks:

- **Duplicated mock skills**: `EchoSkill` in `api.rs` and `MockSkill` in `skill/mod.rs` are functionally identical (both echo input). `FailingSkill` and `AnotherSkill` are also defined locally.
- **Mock providers**: `MockProvider`, `SequencedProvider`, and `MockResponse` are defined in `api.rs` tests but could be reused by future endpoint or middleware tests.
- **HTTP test helpers**: `parse_sse_events`, `post_chat`, `post_chat_raw`, `make_chat_body`, `make_chat_body_with_conversation` are defined in `api.rs` tests.

If the `Skill` or `Provider` trait signatures change, multiple mock implementations in different files must be updated independently.

## Requirements

- [x] Create `buddy-server/src/testutil.rs` gated with `#[cfg(test)]`
- [x] Add `#[cfg(test)] mod testutil;` to `main.rs` (or `lib.rs`)
- [x] Move shared mock skills into `testutil`:
  - Consolidate `EchoSkill` (api.rs) and `MockSkill` (skill/mod.rs) into a single `MockEchoSkill`
  - Move `FailingSkill` from api.rs
  - Move `AnotherSkill` from skill/mod.rs (rename to `MockNoOpSkill` or similar for clarity)
- [x] Move shared mock providers into `testutil`:
  - `MockProvider`
  - `MockResponse`
  - `SequencedProvider`
- [x] Move reusable HTTP test helpers into `testutil`:
  - `parse_sse_events`
  - `post_chat` / `post_chat_raw`
  - `make_chat_body` / `make_chat_body_with_conversation`
- [x] Update all existing test modules to import from `crate::testutil` instead of defining their own copies
- [x] All existing tests pass without modification to test logic

## Test Cases

- [x] `cargo test` passes with no test failures
- [x] `grep -r "struct EchoSkill" buddy-server/src/` returns only the testutil module (no duplicates)
- [x] `grep -r "struct MockSkill" buddy-server/src/` returns only the testutil module (no duplicates)
- [x] `grep -r "struct MockProvider" buddy-server/src/` returns only the testutil module (no duplicates)

## Notes

- Helper functions that are truly local to a single test module (e.g., `temp_db_path` in store.rs, `make_skill` in skill tests) should stay where they are — only extract what's shared or duplicated.
- The `testutil` module must be `#[cfg(test)]` so it's excluded from release builds.
