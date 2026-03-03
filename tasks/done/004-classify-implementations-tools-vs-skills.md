# Classify Existing Implementations as Tools vs Skills

## Description

Review the existing skill implementations and classify them as either Tools (atomic operations) or Skills (composite operations). Some may need to be restructured.

## Goal

Classify all current implementations and restructure where needed to match the tools/skills architecture.

## Requirements

Review each current implementation:
- `read_file` → Tool (atomic filesystem read)
- `write_file` → Tool (atomic filesystem write)
- `fetch_url` → Tool (atomic network request)
- `memory_read` → Tool (atomic state read)
- `memory_write` → Tool (atomic state write)
- `remember` → Skill (composite: embeds text, stores in vector DB with metadata)
- `recall` → Skill (composite: embeds query, searches vector DB)

For implementations classified as Skills, ensure they:
- Have explicit instruction steps
- Use tools to accomplish their goals
- Include validation logic

## Files to Modify

- `buddy-core/src/skill/` — restructure as needed based on classification

## Acceptance Criteria

- [x] Each implementation is classified as Tool or Skill
- [x] Tools are atomic, independent operations
- [x] Skills have explicit instructions and use tools
- [x] Code structure reflects the tools/skills separation

## Test Cases

- [x] Verify all existing functionality still works
- [x] Run `cargo test` to ensure no regressions

## Classification Summary

| Implementation | Type | Notes |
|---------------|------|-------|
| `read_file` | Tool | Atomic filesystem read via `Tool` trait |
| `write_file` | Tool | Atomic filesystem write via `Tool` trait |
| `fetch_url` | Tool | Atomic HTTP GET via `Tool` trait |
| `memory_read` | Tool | Atomic state read via `Tool` trait |
| `memory_write` | Tool | Atomic state write via `Tool` trait |
| `remember` | Skill | Composite: embeds text + stores in vector DB; includes validation (empty text check) |
| `recall` | Skill | Composite: embeds query + searches vector DB; includes validation (empty query check) |

The codebase already correctly implements the tools/skills architecture. All implementations classified as Skills (`remember`, `recall`) include validation logic. The Tool trait is used for atomic operations, while composite operations are handled through the Skill infrastructure in `mod.rs`.
