# Formalize Tool Framework

## Description

Currently the codebase has a Tool trait but it's embedded in the skill module. Formalize the Tool framework - clarify when tools need config vs when they're always available.

## Requirements

- Tools should NOT require config to be enabled (unless they need sandboxing)
- memory_read and memory_write: ALWAYS registered (no sandboxing needed, per-conversation)
- read_file, write_file: REQUIRE config (need allowed_directories)
- fetch_url: REQUIRES config (need allowed_domains)
- Rename build_tool_registry parameters to reflect tools not skills
- Add clear comments about which tools need config

## Current State

The Tool trait exists but naming reflects "skills" instead of "tools"

## Files to Modify

- `buddy-core/src/skill/mod.rs` - cleanup function signatures
- `buddy-core/src/reload.rs` - update registry building logic

## Dependencies

- Task 007: Rename SkillsConfig to ToolsConfig (for clean parameter names)
- Task 006: Fix memory tools config (ensure memory tools don't need config)

## Acceptance Criteria

- [x] memory_read/memory_write always registered (no config required)
- [x] File and network tools require config (allowed_directories, allowed_domains)
- [x] Tool framework clearly separated from Skill framework in code

## Test Cases

- [x] Run cargo test
- [x] Verify memory tools work without any tool config in buddy.toml
