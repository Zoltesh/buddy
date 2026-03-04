# Fix Task 005: Memory Tools Should Not Need Config

## Description

Task 005 incorrectly added memory_read and memory_write to SkillsConfig as toggleable tools. Per the architecture, these are atomic tools that operate on per-conversation scratchpad and should NOT require configuration.

## What Was Done Wrong

- Added memory_read and memory_write to SkillsConfig
- Created empty MemoryToolConfig struct
- Made memory tools conditional on config

## What Needs to Be Done

- REVERT the SkillsConfig changes (memory_read, memory_write, MemoryToolConfig)
- Ensure memory_read and memory_write are ALWAYS registered in the tool registry
- memory_read/memory_write are like file tools but without sandboxing needs - they're per-conversation and non-persistent

## Files to Modify

- `buddy-core/src/config.rs` - remove memory_read, memory_write, MemoryToolConfig

## Acceptance Criteria

- [x] Memory tools NOT in SkillsConfig/ToolsConfig
- [x] memory_read and memory_write always available
- [x] No toggle for memory tools in UI (they're always on)

## Test Cases

- [x] Memory tools work without any config
- [x] cargo test passes
