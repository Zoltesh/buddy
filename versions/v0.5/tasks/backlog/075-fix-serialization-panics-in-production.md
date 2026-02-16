# 075 — Fix Serialization Panics in Production Paths

## Description

Several production code paths use `.unwrap()` or `.expect()` on operations that could theoretically fail, causing a server panic instead of a graceful error.

The most concerning is `buddy-core/src/skill/read_file.rs:51` which has a TOCTOU (time-of-check-time-of-use) race: `validate_path()` checks that a directory exists in its first pass, then calls `canonicalize(dir).unwrap()` in the second pass — if the directory is deleted between the two calls, the server panics.

The other sites (`store.rs:419`, `config.rs:332`, `fetch_url.rs:20`) use `.expect()` on operations that are extremely unlikely to fail, but should still be reviewed.

## Goal

All `.unwrap()` and `.expect()` calls in the listed production code paths either propagate errors or have a clear justification comment.

## Requirements

### Must fix

1. **`buddy-core/src/skill/read_file.rs:51`** — `canonicalize(dir).unwrap()`
   - Replace with `canonicalize(dir).map_err(|e| SkillError::ExecutionFailed(format!("failed to resolve directory: {e}")))?`
   - This eliminates the TOCTOU race panic

### Review and decide

2. **`buddy-core/src/store.rs:419`** — `serde_json::to_string(content).expect("MessageContent should always serialize")`
   - `MessageContent` is an internal enum with only known variants. Serialization failure is a programming bug, not a runtime condition.
   - **Decision:** Keep the `.expect()` — this is intentional defensive programming. The message is descriptive. Add a one-line comment above: `// MessageContent is a known enum; serialization failure indicates a code bug`

3. **`buddy-core/src/config.rs:332`** — `toml::to_string_pretty(self).expect("Config should always be serializable to TOML")`
   - Config is always constructed from valid TOML or defaults. Same reasoning.
   - **Decision:** Keep the `.expect()`. Add a one-line comment above: `// Config round-trips through TOML; serialization failure indicates a code bug`

4. **`buddy-core/src/skill/fetch_url.rs:20`** — `Client::builder().build().expect("failed to build HTTP client")`
   - This runs at skill construction time (server startup). If the HTTP client can't be built, the server can't function.
   - **Decision:** Keep the `.expect()`. Panicking at startup for fundamental infrastructure failure is appropriate. No change needed.

## Files to Modify

- `buddy-core/src/skill/read_file.rs` — replace `.unwrap()` with error propagation
- `buddy-core/src/store.rs` — add justification comment (optional)
- `buddy-core/src/config.rs` — add justification comment (optional)

## Acceptance Criteria

- [ ] `read_file.rs` no longer has `.unwrap()` on `canonicalize()` — uses `?` with error mapping instead
- [ ] The remaining `.expect()` calls have clear justification comments
- [ ] All existing tests pass (`cargo test`)

## Test Cases

- [ ] Call `ReadFileSkill` with a valid file path; assert it returns success (existing behavior preserved)
- [ ] Call `ReadFileSkill` where the allowed directory has been removed between construction and execution; assert it returns a `SkillError`, not a panic
- [ ] Run `cargo test`; assert all existing tests still pass
