# 074 — Replace Mutex `.unwrap()` with Error Propagation

## Description

Production code uses `.lock().unwrap()` on `Mutex` throughout `store.rs` (12 calls), `working_memory.rs` (2 calls), and `process.rs` (5 calls). If any thread panics while holding a lock, the mutex becomes poisoned and every subsequent `.unwrap()` panics, cascading through the server.

The fix is to replace `.lock().unwrap()` with proper error handling so a poisoned mutex results in a returned error, not a server crash.

## Goal

All `.lock().unwrap()` calls in production code (non-test) are replaced with error propagation using `?` or `.map_err()`.

## Requirements

### buddy-core/src/store.rs (~12 calls)

- Every method that calls `self.conn.lock().unwrap()` should instead use:
  ```rust
  let conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
  ```
- Add a `LockPoisoned` variant to `StoreError` (or whatever error type `Store` methods return). If methods currently return `String` errors, use:
  ```rust
  let conn = self.conn.lock().map_err(|_| "database lock poisoned".to_string())?;
  ```
- Apply this to all 12 call sites: `migrate()`, `create_conversation()`, `create_conversation_with_source()`, `list_conversations()`, `get_conversation()`, `delete_conversation()`, `append_message()`, `get_conversation_id_for_telegram_chat()`, `set_telegram_chat_mapping()`, `get_conversation_id_for_whatsapp_phone()`, `set_whatsapp_chat_mapping()`, `update_conversation_title()`
- Check the return type of each method. If a method currently returns `Result<T, SomeError>`, add the lock error to `SomeError`. If a method doesn't return `Result`, you'll need to change it to return `Result` — update callers accordingly.

### buddy-core/src/skill/working_memory.rs (2 calls)

- `MemoryWriteSkill::execute()` (line ~143): Replace `self.map.lock().unwrap()` with error handling that returns a `SkillError`
- `MemoryReadSkill::execute()` (line ~240): Same treatment
- Use: `self.map.lock().map_err(|_| SkillError::ExecutionFailed("memory lock poisoned".into()))?`

### buddy-server/src/process.rs (~5 calls in production, some in tests)

- `stop_telegram()` (line ~85): Replace with error propagation
- `manage_telegram()` (line ~101): Replace with error propagation
- `manage_telegram_on_config_change()` (line ~281): Replace with error propagation
- Test-only `.lock().unwrap()` calls (lines ~184, ~211) can stay as `.unwrap()` — panicking in tests is acceptable

### Rules

- Do NOT change test code — `.unwrap()` in `#[cfg(test)]` blocks is fine
- Each error message should identify which lock failed (e.g., "database lock poisoned", "memory lock poisoned", "telegram process lock poisoned")
- Do not change any public API signatures unless necessary for error propagation
- If a function's return type changes, update all callers

## Files to Modify

- `buddy-core/src/store.rs` — 12 call sites + possible error type addition
- `buddy-core/src/skill/working_memory.rs` — 2 call sites
- `buddy-server/src/process.rs` — 3 production call sites
- Any callers that need updating due to return type changes

## Acceptance Criteria

- [ ] Zero `.lock().unwrap()` calls remain in non-test production code across these 3 files
- [ ] Each replaced call site propagates a descriptive error instead of panicking
- [ ] Test code still uses `.unwrap()` (no unnecessary changes)
- [ ] All existing tests pass (`cargo test`)
- [ ] `cargo build --workspace` compiles without new warnings

## Test Cases

- [ ] Call `Store::list_conversations()` on a healthy store; assert it returns `Ok(...)` (existing behavior preserved)
- [ ] Call `MemoryWriteSkill::execute()` normally; assert it returns success (existing behavior preserved)
- [ ] Run `cargo test`; assert all existing tests still pass (regression check)
