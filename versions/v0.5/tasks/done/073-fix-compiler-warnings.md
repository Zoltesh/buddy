# 073 — Fix All Compiler Warnings

## Description

`cargo build` produces 12 compiler warnings across 4 crates. Zero warnings is the baseline — these should be cleaned up before any new work.

## Goal

`cargo build --workspace 2>&1 | grep "warning\[" | wc -l` returns 0.

## Requirements

### buddy-core warnings

1. **Dead code: `GeminiCandidate.finish_reason`** (`provider/gemini.rs:173`)
   - This field is deserialized from the API but never read.
   - Add `#[allow(dead_code)]` to the field with a comment: `// Deserialized from API response; kept for completeness`

2. **Dead code: `GeminiError.status`** (`provider/gemini.rs:202`)
   - Same situation — deserialized but unused.
   - Add `#[allow(dead_code)]` to the field with a comment: `// Deserialized from API response; kept for completeness`

### buddy-server warnings

3. **Unused imports: `ConversationApprovals`, `PendingApprovals`, `new_pending_approvals`** (`api/mod.rs:33`)
   - Remove these three items from the `use` statement. They are re-exported but never used.
   - Before removing, search the entire `buddy-server` crate to confirm nothing imports them from `api::`. If something does, keep those and remove only the truly unused ones.

4. **Unused variable: `embedder_b`** (`api/tests.rs:4482`)
   - The first `embedder_b` declaration (around line 4482) is unused. The second one (around line 4509) IS used.
   - Remove the unused declaration and its associated line.

### buddy-telegram warnings

5. **Dead code: `telegram_to_buddy`** (`adapter.rs:10`)
   - If this function is not called anywhere in the crate, remove it entirely.

6. **Dead code: `buddy_to_telegram`** (`adapter.rs:25`)
   - If this function is not called anywhere in the crate, remove it entirely.

7. **Dead code: `BotError::Provider(String)` field** (`handler.rs:43`)
   - The `String` field inside `Provider(String)` is constructed but the string is never read.
   - If the variant is constructed somewhere, keep the variant but check if the string contents are actually used. If the string is only created but never displayed/logged, either: (a) remove the `String` payload to make it a unit variant, or (b) add a `Display` impl that uses it.

8. **Dead code: `BotError::Store(String)` field** (`handler.rs:45`)
   - Same as above for `Store(String)`.

### buddy-whatsapp warnings

9. **Dead code: `WhatsAppMessage.timestamp`** (`adapter.rs:37`)
   - Deserialized from webhook payload but never read. Add `#[allow(dead_code)]` with comment: `// Deserialized from webhook payload; may be used in future`

10. **Dead code: `InteractiveReply.reply_type`** (`adapter.rs:47`)
    - Same — add `#[allow(dead_code)]` with comment.

11. **Dead code: `ButtonReply.title`** (`adapter.rs:55`)
    - Same — add `#[allow(dead_code)]` with comment.

### General rules

- For deserialized fields that exist for API/payload completeness: use `#[allow(dead_code)]` with a comment explaining why.
- For functions/variables that are truly unused: remove them.
- Do not change any function signatures or public APIs.

## Files to Modify

- `buddy-core/src/provider/gemini.rs` — annotate 2 struct fields
- `buddy-server/src/api/mod.rs` — remove unused imports
- `buddy-server/src/api/tests.rs` — remove unused variable
- `buddy-telegram/src/adapter.rs` — remove 2 dead functions
- `buddy-telegram/src/handler.rs` — fix 2 enum variant warnings
- `buddy-whatsapp/src/adapter.rs` — annotate 3 struct fields

## Acceptance Criteria

- [x] `cargo build --workspace` produces zero warnings
- [x] No functional behavior has changed
- [x] All existing tests pass (`cargo test`)
- [x] No `#[allow(dead_code)]` added without an explanatory comment

## Test Cases

- [x] Run `cargo build --workspace 2>&1`; assert no lines containing `warning[dead_code]` or `warning[unused`
- [x] Run `cargo test`; assert all tests pass (no regressions from removed code)
