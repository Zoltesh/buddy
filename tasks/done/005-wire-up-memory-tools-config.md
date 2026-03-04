# Wire Up Memory Tools to Config System

## Description

Wire up `memory_read` and `memory_write` tools to the config system so they can be enabled/disabled and have their settings configured through the Settings UI.

Currently these tools exist in the codebase but are not registered in `build_tool_registry()` and have no config options in `SkillsConfig`.

## Requirements

- Add `memory_read` and `memory_write` config options to `SkillsConfig` struct in `buddy-core/src/config.rs`
- Update `build_tool_registry()` in `buddy-core/src/skill/mod.rs` to register memory tools
- Memory tools should be toggleable (enabled/disabled) like other tools

## Files to Modify

- `buddy-core/src/config.rs` — Add config options
- `buddy-core/src/skill/mod.rs` — Register memory tools
- `frontend/src/lib/settings/ToolsTab.svelte` — Add memory tools to the UI list

## Acceptance Criteria

- [x] memory_read has config option in SkillsConfig
- [x] memory_write has config option in SkillsConfig
- [x] Both tools are registered in build_tool_registry()
- [x] Tools appear in Tools tab UI
- [x] Tools can be toggled on/off

## Test Cases

- [ ] Verify memory tools appear in Tools tab when configured
- [x] Run `cargo test` to ensure no regressions
