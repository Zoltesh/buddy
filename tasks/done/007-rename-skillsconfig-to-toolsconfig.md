# Rename SkillsConfig to ToolsConfig

## Description

The config struct `SkillsConfig` incorrectly contains tool configurations (read_file, write_file, fetch_url). According to the tools/skills architecture, these are atomic tools, not skills. Rename to properly reflect that these are tool sandbox configurations.

## Requirements

- Rename `SkillsConfig` struct to `ToolsConfig` in `buddy-core/src/config.rs`
- Rename the `skills` field in `Config` struct to `tools`
- Update all references throughout buddy-core
- Update `buddy-server/src/api/config.rs` - rename `validate_skills` to `validate_tools`
- Update any API endpoint references

## Files to Modify

- `buddy-core/src/config.rs`
- `buddy-core/src/skill/mod.rs`
- `buddy-server/src/api/config.rs`
- Any other files referencing SkillsConfig

## Breaking Change

This is a breaking config change - existing buddy.toml files use `[skills]` section which will need to become `[tools]`.

## Dependencies

- Task 006: Fix memory tools config bug (must be done first to avoid conflicts)

## Acceptance Criteria

- [x] SkillsConfig renamed to ToolsConfig in config.rs
- [x] config.skills field renamed to config.tools
- [x] validate_skills renamed to validate_tools
- [x] All Rust code references updated

## Test Cases

- [x] Run cargo test to ensure no regressions
- [x] Verify config parsing works with new structure
