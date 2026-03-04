# Update Frontend to Use ToolsConfig

## Description

Update frontend components to reference `config.tools` instead of `config.skills` after the config rename.

## Requirements

- Update SkillsTab.svelte to reference `config.tools` for tool-related operations
- Update ToolsTab.svelte to reference `config.tools`
- Update any API calls that PUT/PATCH tool configs

## Files to Modify

- `frontend/src/lib/settings/SkillsTab.svelte`
- `frontend/src/lib/settings/ToolsTab.svelte`
- `frontend/src/lib/api.js` (if relevant)

## Dependencies

- Task 007: Rename SkillsConfig to ToolsConfig must be completed first

## Acceptance Criteria

- [x] SkillsTab.svelte uses config.tools for tool config section
- [x] ToolsTab.svelte uses config.tools for enabled check
- [x] No references to config.skills remain (except for skills section)

## Test Cases

- [x] Verify Tools tab shows tools as enabled/disabled based on config.tools
- [x] Verify Skills tab tool section works correctly
- [x] Run frontend build to ensure no errors
