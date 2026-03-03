# Refactor Skill Trait to Tool

## Description

Currently, everything is implemented as a `Skill` in `buddy-core/src/skill/`. This conflates tools (atomic capabilities) with skills (composite operations). The first step is to rename the `Skill` trait to `Tool` to establish clear terminology.

## Goal

Rename the `Skill` trait to `Tool` in the codebase to properly represent atomic, low-level capabilities.

## Requirements

- Rename `Skill` trait → `Tool` in `buddy-core/src/skill/mod.rs`
- Rename `SkillError` → `ToolError`
- Rename `SkillRegistry` → `ToolRegistry`
- Rename `build_registry` → `build_tool_registry`
- Update all references throughout the codebase (buddy-core, buddy-server)
- Update test function names accordingly
- Ensure all tests pass after renaming

## Files to Modify

- `buddy-core/src/skill/mod.rs` — rename trait and related types
- `buddy-core/src/skill/read_file.rs`
- `buddy-core/src/skill/write_file.rs`
- `buddy-core/src/skill/fetch_url.rs`
- `buddy-core/src/skill/recall.rs`
- `buddy-core/src/skill/remember.rs`
- `buddy-core/src/skill/working_memory.rs`
- `buddy-core/src/reload.rs`
- `buddy-server/src/api/tests.rs`
- Any other files that reference the old names

## Acceptance Criteria

- [x] `Skill` trait renamed to `Tool`
- [x] `SkillError` renamed to `ToolError`
- [x] `SkillRegistry` renamed to `ToolRegistry`  
- [x] All existing tests pass
- [x] No broken references in the codebase

## Test Cases

- [x] Run `cargo test` and verify all tests pass
- [x] Verify tool definitions are still generated correctly
- [x] Verify existing tool execution still works
