# 082 — Replace Frontend String-Matching Tests

## Description

`buddy-server/src/api/tests.rs` contains ~23 tests (in modules like `app_shell_navigation` and `auth_frontend`) that use `include_str!()` to read Svelte source files and then assert that certain strings exist in the source code. These tests verify that code was written, not that it functions — they always pass as long as the string literals exist in the source files, regardless of whether the UI actually works.

Example pattern:
```rust
#[test]
fn sidebar_has_brand_and_nav_items() {
    let source = include_str!("../../../frontend/src/lib/Sidebar.svelte");
    assert!(source.contains("buddy"));
    // ...
}
```

These tests provide a false sense of coverage and slow down future refactoring (renaming a CSS class breaks a Rust test).

## Goal

Remove the string-matching frontend tests and document what actual frontend testing strategy should replace them (if any).

## Requirements

1. **Identify all string-matching frontend tests** — Search `buddy-server/src/api/tests.rs` for all tests that use `include_str!` to read `.svelte`, `.ts`, or `.js` files. List them.

2. **Remove them** — Delete all identified test functions and their containing modules (e.g., `mod app_shell_navigation`, `mod auth_frontend`).

3. **Clean up imports** — If removing these tests leaves unused imports or empty modules, clean those up.

4. **Do NOT add replacement tests in this task** — Frontend behavioral testing (e.g., with Playwright or vitest) is a separate discussion. This task only removes the misleading tests.

5. **Update test count expectations** — If any CI or documentation references a specific test count, note the reduction.

## Files to Modify

- `buddy-server/src/api/tests.rs` — remove string-matching test functions and their modules

## Acceptance Criteria

- [ ] No tests in the Rust codebase use `include_str!` to read frontend source files
- [ ] All remaining tests pass (`cargo test`)
- [ ] No empty test modules left behind
- [ ] No unused imports from the removal

## Test Cases

- [ ] Search the codebase for `include_str!` combined with `.svelte`; assert zero matches in test code
- [ ] Run `cargo test`; assert all remaining tests pass
- [ ] Run `cargo build --workspace`; assert no new warnings from unused imports
