# 084 — Add Hot Reload Test Coverage

## Description

`buddy-server/src/reload.rs` has only 1 test (`reload_from_config_activates_local_embedder_when_external_removed`). The hot-reload system handles provider chain updates, skill registry changes, and embedder swaps — all critical paths with no test coverage for failure scenarios or edge cases.

## Goal

Add tests that cover the important hot-reload scenarios: provider changes, error handling, and state consistency.

## Requirements

First, read `buddy-server/src/reload.rs` to understand the `reload_from_config()` function signature, what it takes as input, and what it modifies. Then add the following tests in the existing `#[cfg(test)] mod tests` block in `reload.rs`.

### Test cases to add

1. **Provider chain reloads when model config changes**
   - Create an initial config with one chat provider (e.g., OpenAI with model "gpt-4")
   - Call `reload_from_config()` with a new config that has a different model or provider
   - Assert the provider chain in `AppState` reflects the new config

2. **Reload preserves existing conversations**
   - Create an `AppState` with a `Store` that has existing conversations
   - Call `reload_from_config()` with a changed config
   - Assert conversations in the `Store` are still accessible (reload doesn't drop the database)

3. **Reload with invalid provider config does not crash**
   - Call `reload_from_config()` with a config that has an invalid provider (e.g., missing API key env var)
   - Assert the function returns an error or logs a warning — does NOT panic
   - Assert the previous working state is still intact

4. **Reload updates system prompt**
   - Create initial config with `system_prompt = "Hello"`
   - Reload with `system_prompt = "Goodbye"`
   - Assert the active provider's system prompt reflects the new value

5. **Reload with empty provider list**
   - Create a config with zero chat providers
   - Assert `reload_from_config()` handles this gracefully (error, not panic)

### Guidelines

- Use the existing test helper patterns in `reload.rs` (look at how the existing test sets up state)
- Use `Store::open_in_memory()` for database state
- Use `MockProvider` or `MockEmbedder` from `testutil.rs` where needed
- Use `Config::parse(toml_str)` to create configs
- Each test should be independent — don't rely on shared mutable state between tests

## Files to Modify

- `buddy-server/src/reload.rs` — add 5 tests to the existing `#[cfg(test)] mod tests` block

## Acceptance Criteria

- [ ] At least 5 new tests exist in `reload.rs`
- [ ] Tests cover: provider changes, state preservation, error handling, system prompt update, empty config
- [ ] All tests pass (`cargo test`)
- [ ] Existing test still passes (no regression)

## Test Cases

- [ ] Run `cargo test -- reload`; assert 6+ tests pass (1 existing + 5 new)
