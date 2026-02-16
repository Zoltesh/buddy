# 078 — Replace Config Error String Matching with Structured Check

## Description

`buddy-core/src/config.rs` (lines 316-322) uses string matching on TOML error messages to provide friendly error messages:

```rust
let msg = e.to_string();
if msg.contains("missing field `models`") || msg.contains("missing field `chat`") {
    return "invalid config: [models.chat] section is required".to_string();
}
```

This is brittle — if the `toml` crate changes its error message wording in a future version, this check silently stops working and users get raw TOML parse errors instead of the friendly message.

## Goal

Replace string matching on error messages with a structural check that doesn't depend on error message text.

## Requirements

- In `Config::parse()`, instead of matching on the error message after a failed parse, use a two-step approach:
  1. First, attempt to parse the TOML as a generic `toml::Value` (which will succeed for any valid TOML, regardless of schema)
  2. If that succeeds, check whether the required fields exist in the `Value` tree:
     - Check that `value.get("models")` is `Some`
     - Check that `value["models"].get("chat")` is `Some`
     - If either is missing, return the friendly error: `"invalid config: [models.chat] section is required"`
  3. Then proceed with the full `toml::from_str::<Config>()` parse for type checking
- Alternatively, a simpler approach: attempt the parse, and if it fails, do a secondary parse to `toml::Value` to check which fields are missing, then return targeted error messages based on what's structurally absent.
- Remove all `msg.contains(...)` calls on error text.
- The resulting error messages should be the same as today (or better) for the end user.

## Files to Modify

- `buddy-core/src/config.rs` — rewrite error handling in `Config::parse()`

## Acceptance Criteria

- [ ] No `e.to_string()` + `.contains()` pattern exists in config parsing
- [ ] A TOML file missing `[models.chat]` still produces a clear, friendly error message
- [ ] A TOML file with other parse errors (invalid syntax, wrong types) still produces the underlying TOML error
- [ ] All existing config tests pass (`cargo test`)

## Test Cases

- [ ] Parse a TOML string missing the `[models]` section entirely; assert the error message mentions `[models.chat] section is required`
- [ ] Parse a TOML string with `[models]` but no `[models.chat]` subsection; assert the error message mentions `[models.chat] section is required`
- [ ] Parse a TOML string with a valid `[models.chat]` section; assert it parses successfully
- [ ] Parse a TOML string with invalid syntax (e.g., unclosed quote); assert it returns a TOML syntax error (not the friendly missing-field message)
- [ ] Run `cargo test`; assert all existing tests pass
